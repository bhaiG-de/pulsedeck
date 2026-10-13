use rustfft::{num_complex::Complex, FftPlanner};
use std::{f32::consts::PI, sync::Arc};
pub(crate) struct Spectrum {
    pub size: usize,
    fft: Arc<dyn rustfft::Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    window: Vec<f32>,
    pub magnitudes: Vec<f32>,
}
impl Spectrum {
    pub fn new(size: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(size);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self {
            size,
            fft,
            buffer: vec![Complex::default(); size],
            scratch,
            window: (0..size)
                .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / (size - 1) as f32).cos())
                .collect(),
            magnitudes: vec![0.0; size / 2 + 1],
        }
    }
    pub fn process(&mut self, samples: &[f32]) {
        let mean = samples.iter().take(self.size).sum::<f32>() / self.size as f32;
        for (i, x) in self.buffer.iter_mut().enumerate() {
            *x = Complex::new(
                (samples.get(i).copied().unwrap_or(0.0) - mean) * self.window[i],
                0.0,
            );
        }
        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);
        for (i, m) in self.magnitudes.iter_mut().enumerate() {
            *m = self.buffer[i].norm();
        }
    }
}
/// Integrating source samples before decimation avoids hard stride sampling.
/// Either channel is retained when summing would cancel antiphase material.
pub(crate) fn downsample(
    samples: &[[f32; 2]],
    start: usize,
    end: usize,
    target_rate: u32,
    source_rate: u32,
) -> Vec<f32> {
    let length = ((end - start) as f64 * target_rate as f64 / source_rate as f64).floor() as usize;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    let mut mono_energy = 0.0;
    for frame in samples[start..end].iter().step_by(64) {
        let l = safe(frame[0]);
        let r = safe(frame[1]);
        left_energy += l * l;
        right_energy += r * r;
        mono_energy += (l + r) * (l + r) * 0.25;
    }
    let single = mono_energy < left_energy.max(right_energy) * 0.1;
    let use_left = left_energy >= right_energy;
    let mut result = Vec::with_capacity(length);
    // Two cascaded RC sections suppress out-of-band energy before integrating.
    let cutoff = (target_rate as f32 * 0.40).min(source_rate as f32 * 0.40);
    let alpha = 1.0 - (-2.0 * PI * cutoff / source_rate as f32).exp();
    let mut state = [0.0_f32; 2];
    let mut input_index = start;
    for i in 0..length {
        let next =
            (start + ((i + 1) as u64 * source_rate as u64 / target_rate as u64) as usize).min(end);
        let mut value = 0.0;
        let mut count = 0;
        while input_index < next {
            let frame = samples[input_index];
            let x = if single {
                safe(frame[if use_left { 0 } else { 1 }])
            } else {
                (safe(frame[0]) + safe(frame[1])) * 0.5
            };
            state[0] += alpha * (x - state[0]);
            state[1] += alpha * (state[0] - state[1]);
            value += state[1];
            count += 1;
            input_index += 1;
        }
        result.push(if count > 0 {
            value / count as f32
        } else {
            state[1]
        });
    }
    result
}
pub(crate) fn safe(x: f32) -> f32 {
    if x.is_finite() {
        x.clamp(-8.0, 8.0)
    } else {
        0.0
    }
}

//! Continuous, bounded native time/pitch processing over already decoded RAM.
//! The source clock belongs to Deck, never to an FFT/cache worker. Starts/seeks
//! prime the processor from future RAM samples using its declared split latency,
//! so output and quantized transport share the same source-time origin.
use std::{ffi::c_void, ptr::NonNull, sync::OnceLock};
const TAPS: usize = 96;
const PHASES: usize = 256;
const CUTOFF_RATIOS: [f64; 13] = [
    1., 1.03, 1.06, 1.125, 1.25, 1.5, 2., 3., 4., 8., 16., 32., 64.,
];
const BLOCK: usize = 64;
const MAX_INPUT: usize = 8192;
static SINCS: OnceLock<Vec<Vec<[f32; TAPS]>>> = OnceLock::new();
fn tables() -> &'static [Vec<[f32; TAPS]>] {
    SINCS.get_or_init(|| {
        CUTOFF_RATIOS
            .iter()
            .map(|r| {
                (0..PHASES)
                    .map(|phase| {
                        let mut row = [0.; TAPS];
                        let frac = phase as f64 / PHASES as f64;
                        let cutoff = 0.95 / r;
                        let mut sum = 0.;
                        for (i, c) in row.iter_mut().enumerate() {
                            let x = i as f64 - (TAPS / 2 - 1) as f64 - frac;
                            let angle = 2. * std::f64::consts::PI * i as f64 / (TAPS - 1) as f64;
                            let w = 0.35875 - 0.48829 * angle.cos() + 0.14128 * (2. * angle).cos()
                                - 0.01168 * (3. * angle).cos();
                            let z = std::f64::consts::PI * x * cutoff;
                            let v = if z.abs() < 1e-12 {
                                cutoff * w
                            } else {
                                cutoff * w * z.sin() / z
                            };
                            *c = v as f32;
                            sum += v;
                        }
                        for c in &mut row {
                            *c /= sum as f32;
                        }
                        row
                    })
                    .collect()
            })
            .collect()
    })
}
/// Blackman-Harris windowed-sinc reconstruction into immutable RAM.
/// Precomputed anti-alias banks conservatively cover the current source step.
/// No filter allocation/reconfiguration happens when tempo or direction changes.
#[inline]
pub(crate) fn sample(
    samples: &[[f32; 2]],
    position: f64,
    step: f64,
    region: Option<(f64, f64)>,
) -> [f32; 2] {
    let integer = position.floor() as i64;
    let fractional = position - position.floor();
    let phase_position = fractional * PHASES as f64;
    let phase = phase_position.floor() as usize;
    let blend = (phase_position - phase as f64) as f32;
    let base = integer - TAPS as i64 / 2 + 1;
    let bank = CUTOFF_RATIOS
        .partition_point(|r| *r < step.abs())
        .min(CUTOFF_RATIOS.len() - 1);
    let coeffs = &tables()[bank][phase];
    let next = &tables()[bank][(phase + 1) % PHASES];
    let mut out = [0.; 2];
    // Most samples are inside RAM and outside a loop: validate the window once.
    let inside_region = region.is_none_or(|(start, end)| {
        let width = (end - start).round() as i64;
        width <= 0
            || (base >= start.round() as i64 && base + (TAPS as i64) < start.round() as i64 + width)
    });
    if inside_region && base >= 0 && base as usize + TAPS < samples.len() && phase + 1 < PHASES {
        let frames = &samples[base as usize..base as usize + TAPS];
        for ((frame, a), b) in frames.iter().zip(coeffs).zip(next) {
            let c = *a * (1. - blend) + *b * blend;
            out[0] += frame[0] * c;
            out[1] += frame[1] * c;
        }
        return out;
    }
    for i in 0..=TAPS {
        let a = if i < TAPS { coeffs[i] } else { 0. };
        let b = if phase == PHASES - 1 {
            if i > 0 {
                next[i - 1]
            } else {
                0.
            }
        } else if i < TAPS {
            next[i]
        } else {
            0.
        };
        let c = a * (1. - blend) + b * blend;
        let mut index = base + i as i64;
        if let Some((start, end)) = region {
            let width = (end - start).round() as i64;
            if width > 0 {
                index = start.round() as i64 + (index - start.round() as i64).rem_euclid(width);
            }
        }
        if index >= 0 && (index as usize) < samples.len() {
            let frame = samples[index as usize];
            out[0] += frame[0] * c;
            out[1] += frame[1] * c;
        }
    }
    out
}
#[cfg(test)]
pub(crate) fn sample_reference(
    samples: &[[f32; 2]],
    position: f64,
    step: f64,
    region: Option<(f64, f64)>,
) -> [f32; 2] {
    let integer = position.floor() as i64;
    let fractional = position - position.floor();
    let phase_position = fractional * PHASES as f64;
    let phase = phase_position.floor() as usize;
    let blend = (phase_position - phase as f64) as f32;
    let base = integer - TAPS as i64 / 2 + 1;
    let bank = CUTOFF_RATIOS
        .partition_point(|r| *r < step.abs())
        .min(CUTOFF_RATIOS.len() - 1);
    let coeffs = &tables()[bank][phase];
    let next = &tables()[bank][(phase + 1) % PHASES];
    let mut out = [0.; 2];
    for i in 0..=TAPS {
        let a = if i < TAPS { coeffs[i] } else { 0. };
        let b = if phase == PHASES - 1 {
            if i > 0 {
                next[i - 1]
            } else {
                0.
            }
        } else if i < TAPS {
            next[i]
        } else {
            0.
        };
        let c = a * (1. - blend) + b * blend;
        let mut index = base + i as i64;
        if let Some((start, end)) = region {
            let width = (end - start).round() as i64;
            if width > 0 {
                index = start.round() as i64 + (index - start.round() as i64).rem_euclid(width);
            }
        }
        if index >= 0 && (index as usize) < samples.len() {
            let frame = samples[index as usize];
            out[0] += frame[0] * c;
            out[1] += frame[1] * c;
        }
    }
    out
}
unsafe extern "C" {
    fn pd_stretch_new() -> *mut c_void;
    fn pd_stretch_delete(handle: *mut c_void);
    fn pd_stretch_seek_length(handle: *mut c_void, rate: f32) -> i32;
    fn pd_stretch_pitch(handle: *mut c_void, factor: f32);
    fn pd_stretch_seek(handle: *mut c_void, left: *const f32, right: *const f32, frames: i32);
    fn pd_stretch_process(
        handle: *mut c_void,
        left: *const f32,
        right: *const f32,
        input: i32,
        out_left: *mut f32,
        out_right: *mut f32,
        output: i32,
    );
}
pub(crate) struct Processor {
    handle: NonNull<c_void>,
    input: [[f32; MAX_INPUT]; 2],
    output: [[f32; BLOCK]; 2],
    offset: usize,
    valid: bool,
    revision: u64,
    keylock: bool,
    pitch: f64,
    pitch_factor: f64,
    fractional_input: f64,
    input_position: f64,
    pub resets: u64,
    pub blocks: u64,
}
// Exclusive ownership follows RenderState. The native processor has no thread
// affinity or global state; it is never called concurrently.
unsafe impl Send for Processor {}
impl Processor {
    pub fn new() -> Self {
        let _ = tables();
        Self {
            handle: NonNull::new(unsafe { pd_stretch_new() })
                .expect("Native time/pitch processor allocation failed"),
            input: [[0.; MAX_INPUT]; 2],
            output: [[0.; BLOCK]; 2],
            offset: BLOCK,
            valid: false,
            revision: 0,
            keylock: false,
            pitch: 0.,
            pitch_factor: 1.,
            fractional_input: 0.,
            input_position: 0.,
            resets: 0,
            blocks: 0,
        }
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
    fn fill(
        &mut self,
        samples: &[[f32; 2]],
        position: f64,
        step: f64,
        frames: usize,
        region: Option<(f64, f64)>,
    ) {
        for i in 0..frames {
            let s = sample(samples, position + i as f64 * step, step, region);
            self.input[0][i] = s[0];
            self.input[1][i] = s[1];
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        samples: &[[f32; 2]],
        position: f64,
        source_rate: u32,
        output_rate: u32,
        speed: f64,
        pitch: f64,
        keylock: bool,
        revision: u64,
        region: Option<(f64, f64)>,
    ) -> [f32; 2] {
        let unit_step = source_rate as f64 / output_rate as f64;
        let active = keylock || pitch.abs() > 0.0001;
        if !active || speed <= 0. {
            self.valid = false;
            return sample(samples, position, unit_step * speed, region);
        }
        let h = self.handle.as_ptr();
        if self.pitch != pitch {
            self.pitch = pitch;
            self.pitch_factor = 2_f64.powf(pitch / 12.);
        }
        let factor = self.pitch_factor * if keylock { 1. } else { speed };
        let changed = !self.valid || self.revision != revision || self.keylock != keylock;
        if changed {
            unsafe {
                pd_stretch_pitch(h, factor as f32);
            }
            let frames = unsafe { pd_stretch_seek_length(h, speed as f32) } as usize;
            // Public tempo range is 0.1..2; sync correction is bounded ±0.08.
            // The fixed buffer also covers the total priming length at that max.
            debug_assert!(frames <= MAX_INPUT);
            self.fill(samples, position, unit_step, frames, region);
            unsafe {
                pd_stretch_seek(
                    h,
                    self.input[0].as_ptr(),
                    self.input[1].as_ptr(),
                    frames as i32,
                );
            }
            self.offset = BLOCK;
            self.fractional_input = 0.;
            self.input_position = position + frames as f64 * unit_step;
            self.valid = true;
            self.revision = revision;
            self.keylock = keylock;
            self.resets += 1;
        }
        if self.offset == BLOCK {
            unsafe {
                pd_stretch_pitch(h, factor as f32);
            }
            let desired = speed * BLOCK as f64 + self.fractional_input;
            let count = desired.floor() as usize;
            self.fractional_input = desired - count as f64;
            // Continue exactly after the consumed input, including integer
            // seek pre-roll. Re-anchoring to transport each block adds phase jumps.
            self.fill(samples, self.input_position, unit_step, count, region);
            unsafe {
                pd_stretch_process(
                    h,
                    self.input[0].as_ptr(),
                    self.input[1].as_ptr(),
                    count as i32,
                    self.output[0].as_mut_ptr(),
                    self.output[1].as_mut_ptr(),
                    BLOCK as i32,
                );
            }
            self.input_position += count as f64 * unit_step;
            self.offset = 0;
            self.blocks += 1;
        }
        let out = [self.output[0][self.offset], self.output[1][self.offset]];
        self.offset += 1;
        out
    }
}
impl Drop for Processor {
    fn drop(&mut self) {
        unsafe {
            pd_stretch_delete(self.handle.as_ptr());
        }
    }
}

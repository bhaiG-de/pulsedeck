use crate::spectral::safe;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WaveformBands {
    pub low: Vec<f32>,
    pub mid: Vec<f32>,
    pub high: Vec<f32>,
}
#[derive(Clone, Debug)]
pub struct WaveformAnalysis {
    pub peaks: Vec<f32>,
    pub bands: WaveformBands,
}
/// Second-order Butterworth LP300 / HP250→LP1200 / HP1200→LP9000 bands. Band RMS is
/// computed from both stereo channels and uses one shared fullband scale, never
/// independently normalized fake colors. Original PCM is neither modified nor saved.
pub fn analyze_waveform(samples: &[[f32; 2]], rate: u32, bins: usize) -> WaveformAnalysis {
    let bins = bins.clamp(1, 16384);
    let mut peaks = vec![0.0_f32; bins];
    let mut energies = vec![[0.0_f64; 3]; bins];
    let mut count = vec![0_usize; bins];
    let mut low = [Biquad::new(300.0, rate, false); 2];
    let mut mid = [[
        Biquad::new(250.0, rate, true),
        Biquad::new(1200.0, rate, false),
    ]; 2];
    let mut high = [[
        Biquad::new(1200.0, rate, true),
        Biquad::new(9000.0, rate, false),
    ]; 2];
    for (index, frame) in samples.iter().enumerate() {
        let bin = (index * bins / samples.len()).min(bins - 1);
        count[bin] += 1;
        for channel in 0..2 {
            let x = safe(frame[channel]);
            peaks[bin] = peaks[bin].max(x.abs());
            let l = low[channel].process(x);
            let m = mid[channel][0].process(x);
            let m = mid[channel][1].process(m);
            let h = high[channel][0].process(x);
            let h = high[channel][1].process(h);
            let values = [l, m, h];
            for band in 0..3 {
                energies[bin][band] += values[band] as f64 * values[band] as f64 * 0.5;
            }
        }
    }
    let scale = peaks.iter().copied().fold(0.0_f32, f32::max).max(1.0);
    let mut bands = WaveformBands {
        low: vec![0.0; bins],
        mid: vec![0.0; bins],
        high: vec![0.0; bins],
    };
    for i in 0..bins {
        peaks[i] = (peaks[i] / scale).clamp(0.0, 1.0);
        let rms = energies[i].map(|x| (x / count[i].max(1) as f64).sqrt() as f32 / scale);
        bands.low[i] = rms[0].clamp(0.0, 1.0);
        bands.mid[i] = rms[1].clamp(0.0, 1.0);
        bands.high[i] = rms[2].clamp(0.0, 1.0);
    }
    WaveformAnalysis { peaks, bands }
}
#[derive(Clone, Copy)]
struct Biquad {
    coefficient: [f32; 5],
    state: [f32; 2],
}
impl Biquad {
    fn new(cutoff: f32, rate: u32, highpass: bool) -> Self {
        let omega =
            2.0 * std::f32::consts::PI * cutoff.min(rate as f32 * 0.45) / rate.max(1) as f32;
        let cos = omega.cos();
        let alpha = omega.sin() / std::f32::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let b0 = if highpass {
            (1.0 + cos) * 0.5
        } else {
            (1.0 - cos) * 0.5
        };
        let b1 = if highpass { -(1.0 + cos) } else { 1.0 - cos };
        Self {
            coefficient: [
                b0 / a0,
                b1 / a0,
                b0 / a0,
                -2.0 * cos / a0,
                (1.0 - alpha) / a0,
            ],
            state: [0.0; 2],
        }
    }
    fn process(&mut self, x: f32) -> f32 {
        let y = self.coefficient[0] * x + self.state[0];
        self.state[0] = self.coefficient[1] * x - self.coefficient[3] * y + self.state[1];
        self.state[1] = self.coefficient[2] * x - self.coefficient[4] * y;
        y
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_band_energy_distinguishes_bass_voice_and_cymbal_frequencies() {
        for (freq, dominant) in [(80.0, 0), (700.0, 1), (4000.0, 2)] {
            let pcm = (0..48000)
                .map(|i| {
                    let x =
                        (2.0 * std::f64::consts::PI * freq * i as f64 / 48000.0).sin() as f32 * 0.6;
                    [x, -x]
                })
                .collect::<Vec<_>>();
            let a = analyze_waveform(&pcm, 48000, 256);
            let bands = [a.bands.low, a.bands.mid, a.bands.high];
            let means = bands.map(|v| v[32..].iter().sum::<f32>() / 224.0);
            let best = means
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0;
            assert_eq!(best, dominant, "{freq}: {means:?}");
            assert!(means[dominant] > 0.35);
            assert!(a.peaks.iter().all(|p| p.is_finite() && *p <= 1.0));
        }
    }
    #[test]
    fn empty_and_nonfinite_waveforms_are_silent() {
        for samples in [vec![], vec![[f32::NAN, f32::INFINITY]; 16]] {
            let a = analyze_waveform(&samples, 48000, 64);
            assert!(a.peaks.iter().all(|x| *x == 0.0));
            assert!(a.bands.low.iter().all(|x| *x == 0.0));
        }
    }
}

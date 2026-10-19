//! Full-source measurements on decoded PCM, always outside the render callback.
use serde::{Deserialize, Serialize};
pub const MEASUREMENTS_VERSION: u32 = 1;
const SOUND_THRESHOLD: f32 = 0.001; // -60 dBFS, independent of musical cue points.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioMeasurements {
    pub version: u32,
    pub sample_rate: u32,
    pub frames: u64,
    pub audible_start_frame: Option<u64>,
    /// Exclusive frame endpoint, preserving one-frame precision.
    pub audible_end_frame: Option<u64>,
    pub threshold_dbfs: f64,
    pub integrated_lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    pub normalization_gain_db: f64,
    pub error: Option<String>,
}
impl AudioMeasurements {
    pub fn current(&self, rate: u32, frames: usize) -> bool {
        self.version == MEASUREMENTS_VERSION
            && self.sample_rate == rate
            && self.frames == frames as u64
    }
    pub fn audible_start_seconds(&self) -> Option<f64> {
        self.audible_start_frame
            .map(|n| n as f64 / self.sample_rate.max(1) as f64)
    }
    pub fn audible_end_seconds(&self) -> Option<f64> {
        self.audible_end_frame
            .map(|n| n as f64 / self.sample_rate.max(1) as f64)
    }
}
pub fn measure_audio(pcm: &[[f32; 2]], rate: u32) -> AudioMeasurements {
    let mut result = AudioMeasurements {
        version: MEASUREMENTS_VERSION,
        sample_rate: rate,
        frames: pcm.len() as u64,
        audible_start_frame: None,
        audible_end_frame: None,
        threshold_dbfs: -60.,
        integrated_lufs: None,
        true_peak_dbtp: None,
        normalization_gain_db: 0.,
        error: None,
    };
    let audible = |f: &[f32; 2]| f.iter().any(|s| s.is_finite() && s.abs() > SOUND_THRESHOLD);
    result.audible_start_frame = pcm.iter().position(audible).map(|n| n as u64);
    result.audible_end_frame = pcm.iter().rposition(audible).map(|n| n as u64 + 1);
    let measured = (|| -> Result<(f64, f64), String> {
        let mut meter = ebur128::EbuR128::new(
            2,
            rate,
            ebur128::Mode::I | ebur128::Mode::TRUE_PEAK | ebur128::Mode::HISTOGRAM,
        )
        .map_err(|e| e.to_string())?;
        let mut interleaved = Vec::with_capacity(16384);
        for chunk in pcm.chunks(8192) {
            interleaved.clear();
            interleaved.extend(
                chunk
                    .iter()
                    .flatten()
                    .map(|s| if s.is_finite() { *s } else { 0. }),
            );
            meter
                .add_frames_f32(&interleaved)
                .map_err(|e| e.to_string())?;
        }
        let loudness = meter.loudness_global().map_err(|e| e.to_string())?;
        let peak = meter
            .true_peak(0)
            .map_err(|e| e.to_string())?
            .max(meter.true_peak(1).map_err(|e| e.to_string())?);
        Ok((loudness, 20. * peak.log10()))
    })();
    match measured {
        Ok((lufs, peak)) => {
            result.integrated_lufs = lufs.is_finite().then_some(lufs);
            result.true_peak_dbtp = peak.is_finite().then_some(peak);
            if let (Some(lufs), Some(peak)) = (result.integrated_lufs, result.true_peak_dbtp) {
                // Static programme matching, with bounded boost and true-peak headroom.
                result.normalization_gain_db = (-18. - lufs)
                    .clamp(-60., 12.)
                    .min(-1. - peak)
                    .clamp(-60., 12.);
            }
        }
        Err(e) => result.error = Some(e),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_are_exact_and_silence_has_no_infinite_json_numbers() {
        let mut pcm = vec![[0.; 2]; 48000];
        pcm[123] = [0.01, 0.];
        pcm[4567] = [0., -0.01];
        let m = measure_audio(&pcm, 48000);
        assert_eq!(m.audible_start_frame, Some(123));
        assert_eq!(m.audible_end_frame, Some(4568));
        let silent = measure_audio(&vec![[0.; 2]; 48000], 48000);
        assert!(silent.audible_start_frame.is_none() && silent.integrated_lufs.is_none());
        assert_eq!(silent.normalization_gain_db, 0.);
        assert!(serde_json::to_value(&silent).unwrap()["integrated_lufs"].is_null());
    }
    #[test]
    fn programme_gain_matches_levels_and_respects_true_peak() {
        let signal = |gain: f32| {
            (0..144000)
                .map(|i| {
                    let x = gain * (i as f32 * 1000. * std::f32::consts::TAU / 48000.).sin();
                    [x, x]
                })
                .collect::<Vec<_>>()
        };
        let a = measure_audio(&signal(0.1), 48000);
        let b = measure_audio(&signal(0.2), 48000);
        assert!((b.integrated_lufs.unwrap() - a.integrated_lufs.unwrap() - 6.0206).abs() < 0.05);
        assert!((a.normalization_gain_db - b.normalization_gain_db - 6.0206).abs() < 0.05);
        assert!(b.true_peak_dbtp.unwrap() + b.normalization_gain_db <= -1. + 1e-9);
        let loud = measure_audio(&signal(1.), 48000);
        assert!(loud.normalization_gain_db < -12.);
        assert!((loud.integrated_lufs.unwrap() + loud.normalization_gain_db + 18.).abs() < 0.05);
    }
}

//! Offline waveform-similarity overlap-add. This prepares a renderable cache;
//! it deliberately never runs on CoreAudio's callback. Stereo correlation uses
//! one shared alignment so left/right phase relationships are preserved.
use crate::decode::TrackData;
use std::sync::Arc;
pub(crate) fn prepare(
    source: &TrackData,
    tempo: f64,
    pitch_semitones: f64,
    keylock: bool,
    cancel: impl Fn() -> bool,
) -> Option<Arc<TrackData>> {
    if !within_memory_budget(source.samples.len(), tempo, pitch_semitones, keylock) {
        return None;
    }
    let tonal_ratio = 2.0_f64.powf(pitch_semitones / 12.0) * if keylock { 1.0 } else { tempo };
    let mut shifted = Vec::new();
    let input = if (tonal_ratio - 1.0).abs() > 1e-8 {
        let length = (source.samples.len() as f64 / tonal_ratio).ceil() as usize;
        shifted.try_reserve_exact(length).ok()?;
        for i in 0..length {
            if i % 65536 == 0 && cancel() {
                return None;
            }
            let pos = i as f64 * tonal_ratio;
            let index = (pos as usize).min(source.samples.len() - 1);
            let next = (index + 1).min(source.samples.len() - 1);
            let fraction = (pos - index as f64) as f32;
            shifted.push([
                source.samples[index][0] * (1.0 - fraction) + source.samples[next][0] * fraction,
                source.samples[index][1] * (1.0 - fraction) + source.samples[next][1] * fraction,
            ]);
        }
        shifted.as_slice()
    } else {
        source.samples.as_slice()
    };
    let stretch_rate = tempo / tonal_ratio;
    let length = (source.samples.len() as f64 / tempo).ceil() as usize;
    let samples = if (stretch_rate - 1.0).abs() < 1e-8 {
        let mut samples = input.to_vec();
        samples.resize(length, [0.0; 2]);
        samples
    } else {
        wsola(input, source.sample_rate, stretch_rate, length, &cancel)?
    };
    Some(Arc::new(TrackData {
        samples,
        sample_rate: source.sample_rate,
        analysis: source.analysis.clone(),
    }))
}
pub(crate) fn within_memory_budget(frames: usize, tempo: f64, pitch: f64, keylock: bool) -> bool {
    if !(keylock || pitch.abs() > 0.0001) {
        return true;
    }
    let tonal = 2.0_f64.powf(pitch / 12.0) * if keylock { 1.0 } else { tempo };
    frames as f64 / tonal + frames as f64 / tempo <= 64_000_000.0
}
fn wsola(
    input: &[[f32; 2]],
    sample_rate: u32,
    rate: f64,
    length: usize,
    cancel: &impl Fn() -> bool,
) -> Option<Vec<[f32; 2]>> {
    let window = (sample_rate as usize * 40 / 1000).max(64);
    let overlap = window / 2;
    let hop = window - overlap;
    let search = (sample_rate as usize * 6 / 1000).max(8);
    let mut output = Vec::new();
    output.try_reserve_exact(length + window).ok()?;
    output.resize(length + window, [0.0; 2]);
    let first = window.min(input.len()).min(output.len());
    output[..first].copy_from_slice(&input[..first]);
    let mut position = hop;
    let mut grain = 0;
    while position < length {
        if grain % 16 == 0 && cancel() {
            return None;
        }
        let expected = (position as f64 * rate).round() as usize;
        let low = expected
            .saturating_sub(search)
            .min(input.len().saturating_sub(1));
        let high = (expected + search).min(input.len().saturating_sub(1));
        let mut best = expected.min(input.len().saturating_sub(1));
        let mut best_score = f64::NEG_INFINITY;
        // Coarse-to-fine search, subsampled normalized correlation. A distance
        // penalty prevents silent intervals accumulating arbitrary cursor drift.
        let score = |candidate: usize| -> f64 {
            let mut cross = 0.0_f64;
            let mut a_energy = 0.0_f64;
            let mut b_energy = 0.0_f64;
            for i in (0..overlap.min(input.len() - candidate)).step_by(8) {
                for channel in 0..2 {
                    let a = output[position + i][channel] as f64;
                    let b = input[candidate + i][channel] as f64;
                    cross += a * b;
                    a_energy += a * a;
                    b_energy += b * b;
                }
            }
            cross / (a_energy * b_energy).sqrt().max(1e-12)
                - (candidate.abs_diff(expected) as f64 / search as f64) * 0.001
        };
        for candidate in (low..=high).step_by(16) {
            let value = score(candidate);
            if value > best_score {
                best_score = value;
                best = candidate;
            }
        }
        let fine_low = best.saturating_sub(16).max(low);
        let fine_high = (best + 16).min(high);
        for candidate in fine_low..=fine_high {
            let value = score(candidate);
            if value > best_score {
                best_score = value;
                best = candidate;
            }
        }
        for i in 0..window.min(input.len() - best) {
            let destination = position + i;
            if destination >= output.len() {
                break;
            }
            if i < overlap {
                let weight = i as f32 / overlap as f32;
                for channel in 0..2 {
                    output[destination][channel] = output[destination][channel] * (1.0 - weight)
                        + input[best + i][channel] * weight;
                }
            } else {
                output[destination] = input[best + i];
            }
        }
        position += hop;
        grain += 1;
    }
    output.truncate(length);
    Some(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keylock_holds_tone_and_changes_duration() {
        let mut source = crate::decode::demo_track(0);
        let data = Arc::get_mut(&mut source).unwrap();
        data.samples = (0..96000)
            .map(|i| {
                let x = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48000.0).sin() as f32;
                [x, x]
            })
            .collect();
        let result = prepare(data, 1.25, 0.0, true, || false).unwrap();
        assert_eq!(result.samples.len(), 76800);
        let start = 12000;
        let end = 60000;
        let crossings = result.samples[start..end]
            .windows(2)
            .filter(|p| p[0][0] <= 0.0 && p[1][0] > 0.0)
            .count();
        assert!((crossings as f64 - 440.0).abs() < 3.0);
    }
    #[test]
    fn cancellation_aborts_preparation() {
        let source = crate::decode::demo_track(0);
        assert!(prepare(&source, 1.2, 3.0, true, || true).is_none());
    }
}

#[cfg(test)]
mod independent_pitch_tests {
    use super::*;
    #[test]
    fn pitch_shift_preserves_tempo_and_stereo_phase() {
        let mut source = crate::decode::demo_track(0);
        let data = Arc::get_mut(&mut source).unwrap();
        data.samples = (0..96000)
            .map(|i| {
                let x = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48000.0).sin() as f32;
                [x, -x]
            })
            .collect();
        let result = prepare(data, 1.0, 3.0, true, || false).unwrap();
        assert_eq!(result.samples.len(), 96000);
        let crossings = result.samples[24000..72000]
            .windows(2)
            .filter(|p| p[0][0] <= 0.0 && p[1][0] > 0.0)
            .count();
        assert!(
            (crossings as f64 - 523.251).abs() < 4.0,
            "frequency {crossings}"
        );
        assert!(result.samples.iter().all(|s| (s[0] + s[1]).abs() < 1e-6));
    }
}

use crate::spectral::{downsample, Spectrum};
use serde::{Deserialize, Serialize};
const RATE: u32 = 11025;
const WINDOW: usize = 8192;
const MAX_WINDOWS: usize = 64;
const MAJOR: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];
const LABELS: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];
const MAJOR_CAMELOT: [&str; 12] = [
    "8B", "3B", "10B", "5B", "12B", "7B", "2B", "9B", "4B", "11B", "6B", "1B",
];
const MINOR_CAMELOT: [&str; 12] = [
    "5A", "12A", "7A", "2A", "9A", "4A", "11A", "6A", "1A", "8A", "3A", "10A",
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeyEstimate {
    pub label: String,
    pub camelot: String,
    pub confidence: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeyCandidate {
    pub label: String,
    pub camelot: String,
    pub score: f32,
}
#[derive(Clone, Debug)]
pub struct KeyAnalysis {
    pub estimate: Option<KeyEstimate>,
    pub candidates: Vec<KeyCandidate>,
}
impl KeyAnalysis {
    fn empty() -> Self {
        Self {
            estimate: None,
            candidates: Vec::new(),
        }
    }
}
/// Backwards-compatible entrypoint. Uncertain results retain candidates through analyze_key.
pub fn estimate_key(samples: &[[f32; 2]], sample_rate: u32) -> Option<KeyEstimate> {
    analyze_key(samples, sample_rate).estimate
}
/// Local spectral peaks with parabolic interpolation and global tuning correction
/// feed octave-folded, compressed pitch-class salience. Flat-spectrum/percussive
/// windows are rejected and major/minor profile agreement is assessed over time.
pub fn analyze_key(samples: &[[f32; 2]], sample_rate: u32) -> KeyAnalysis {
    if !(8000..=384000).contains(&sample_rate) || samples.len() < sample_rate as usize / 2 {
        return KeyAnalysis::empty();
    }
    let source_window = (WINDOW as u64 * sample_rate as u64 / RATE as u64) as usize;
    if source_window > samples.len() {
        return KeyAnalysis::empty();
    }
    let count = (samples.len() / source_window).clamp(1, MAX_WINDOWS);
    let mut spectrum = Spectrum::new(WINDOW);
    let mut windows = Vec::new();
    let mut tuning = [0.0_f64; 50];
    for index in 0..count {
        let start = if count == 1 {
            0
        } else {
            (samples.len() - source_window) * index / (count - 1)
        };
        let signal = downsample(
            samples,
            start,
            (start + source_window).min(samples.len()),
            RATE,
            sample_rate,
        );
        let energy =
            signal.iter().map(|s| *s as f64 * *s as f64).sum::<f64>() / signal.len().max(1) as f64;
        if energy < 1e-8 {
            continue;
        }
        spectrum.process(&signal);
        let magnitudes = &spectrum.magnitudes;
        let min_bin = (55.0 * WINDOW as f64 / RATE as f64) as usize;
        let max_bin = (4500.0 * WINDOW as f64 / RATE as f64) as usize;
        let sum = magnitudes[min_bin..max_bin]
            .iter()
            .map(|m| *m as f64)
            .sum::<f64>();
        let geometric = (magnitudes[min_bin..max_bin]
            .iter()
            .map(|m| (*m as f64 + 1e-12).ln())
            .sum::<f64>()
            / (max_bin - min_bin) as f64)
            .exp();
        let flatness = geometric / (sum / (max_bin - min_bin) as f64).max(1e-12);
        if flatness > 0.78 {
            continue;
        }
        let mut peaks = Vec::new();
        for bin in min_bin.max(1)..max_bin.min(magnitudes.len() - 1) {
            let value = magnitudes[bin];
            if value <= magnitudes[bin - 1] || value < magnitudes[bin + 1] {
                continue;
            }
            let low = bin.saturating_sub(20).max(min_bin);
            let high = (bin + 21).min(max_bin);
            let background =
                magnitudes[low..high].iter().copied().sum::<f32>() / (high - low) as f32;
            if value < background * 2.3 || value < 0.005 {
                continue;
            }
            let a = (magnitudes[bin - 1] as f64 + 1e-12).ln();
            let b = (value as f64 + 1e-12).ln();
            let c = (magnitudes[bin + 1] as f64 + 1e-12).ln();
            let denominator = a - 2.0 * b + c;
            let delta = if denominator.abs() < 1e-9 {
                0.0
            } else {
                (0.5 * (a - c) / denominator).clamp(-0.5, 0.5)
            };
            let frequency = (bin as f64 + delta) * RATE as f64 / WINDOW as f64;
            let midi = 69.0 + 12.0 * (frequency / 440.0).log2();
            let residual = midi - midi.round();
            let salience = (value as f64).sqrt()
                * (value as f64 / (background as f64 + 1e-6)).min(8.0).sqrt()
                / (1.0 + frequency / 2500.0);
            let bucket = (((residual + 0.5) * 50.0).floor() as usize).min(49);
            if frequency > 100.0 {
                tuning[bucket] += salience;
            }
            peaks.push((midi, salience));
        }
        if peaks.len() >= 3 {
            windows.push(peaks);
        }
    }
    if windows.is_empty() {
        return KeyAnalysis::empty();
    }
    let strongest = tuning
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    let total = tuning.iter().sum::<f64>();
    let tuning_offset = if total > 0.0 {
        let mut weighted = 0.0;
        let mut weight = 0.0;
        for offset in -2_isize..=2 {
            let i = (strongest as isize + offset).rem_euclid(50) as usize;
            let residual = (strongest as f64 + offset as f64 + 0.5) / 50.0 - 0.5;
            weighted += residual * tuning[i];
            weight += tuning[i];
        }
        weighted / weight.max(1e-12)
    } else {
        0.0
    };
    let mut chroma = [0.0; 12];
    let mut local_chromas = Vec::new();
    for peaks in windows {
        let mut local = [0.0_f64; 12];
        for (midi, salience) in peaks {
            let tuned = midi - tuning_offset;
            let note = tuned.round() as i32;
            let distance = tuned - note as f64;
            let weight = (-0.5 * (distance / 0.20).powi(2)).exp();
            local[note.rem_euclid(12) as usize] += salience * weight;
        }
        let total = local.iter().sum::<f64>();
        if total <= 0.0 {
            continue;
        }
        for i in 0..12 {
            local[i] /= total;
            chroma[i] += local[i];
        }
        local_chromas.push(local);
    }
    if local_chromas.is_empty() {
        return KeyAnalysis::empty();
    }
    for x in &mut chroma {
        *x /= local_chromas.len() as f64;
    }
    classify(chroma, &local_chromas)
}
fn classify(chroma: [f64; 12], windows: &[[f64; 12]]) -> KeyAnalysis {
    let total = chroma.iter().sum::<f64>();
    if !total.is_finite() || total <= 0.0 {
        return KeyAnalysis::empty();
    }
    let normalized = chroma.map(|x| x / total);
    let entropy = -normalized
        .iter()
        .filter(|x| **x > 0.0)
        .map(|x| x * x.ln())
        .sum::<f64>()
        / 12.0_f64.ln();
    let supported = normalized.iter().filter(|x| **x > 0.065).count();
    if entropy > 0.975 || supported < 3 {
        return KeyAnalysis::empty();
    }
    let mut ranking = Vec::with_capacity(24);
    for minor in [false, true] {
        let profile = if minor { MINOR } else { MAJOR };
        for tonic in 0..12 {
            let rotated = std::array::from_fn(|i| profile[(i + 12 - tonic) % 12]);
            ranking.push((correlation(normalized, rotated), tonic, minor, rotated));
        }
    }
    ranking.sort_by(|a, b| b.0.total_cmp(&a.0));
    let best = &ranking[0];
    let margin = best.0 - ranking[1].0;
    let candidates = ranking
        .iter()
        .take(3)
        .filter(|r| r.0 > 0.35)
        .map(|r| candidate(r.1, r.2, r.0))
        .collect::<Vec<_>>();
    let agreement = if windows.is_empty() {
        1.0
    } else {
        windows
            .iter()
            .filter(|local| correlation(**local, best.3) >= 0.2)
            .count() as f64
            / windows.len() as f64
    };
    let confidence = (((best.0 - 0.35) / 0.6).clamp(0.0, 1.0) * (margin / 0.14).clamp(0.0, 1.0))
        .sqrt()
        * (0.6 + 0.4 * agreement);
    let estimate = if best.0 >= 0.48 && margin >= 0.018 && confidence >= 0.22 {
        let c = candidate(best.1, best.2, best.0);
        Some(KeyEstimate {
            label: c.label,
            camelot: c.camelot,
            confidence: confidence as f32,
        })
    } else {
        None
    };
    KeyAnalysis {
        estimate,
        candidates,
    }
}
fn candidate(tonic: usize, minor: bool, score: f64) -> KeyCandidate {
    KeyCandidate {
        label: format!(
            "{} {}",
            LABELS[tonic],
            if minor { "minor" } else { "major" }
        ),
        camelot: if minor {
            MINOR_CAMELOT[tonic]
        } else {
            MAJOR_CAMELOT[tonic]
        }
        .into(),
        score: score.clamp(0.0, 1.0) as f32,
    }
}
fn correlation(a: [f64; 12], b: [f64; 12]) -> f64 {
    let ma = a.iter().sum::<f64>() / 12.0;
    let mb = b.iter().sum::<f64>() / 12.0;
    let mut cross = 0.0;
    let mut aa = 0.0;
    let mut bb = 0.0;
    for i in 0..12 {
        let x = a[i] - ma;
        let y = b[i] - mb;
        cross += x * y;
        aa += x * x;
        bb += y * y;
    }
    cross / (aa * bb).sqrt().max(1e-12)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn chord(notes: &[u32], cents: f64, rate: u32, seconds: f64, harmonics: bool) -> Vec<[f32; 2]> {
        (0..(seconds * rate as f64) as usize)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let mut value = 0.0;
                for n in notes {
                    let phase = 2.0
                        * std::f64::consts::PI
                        * 440.0
                        * 2.0_f64.powf((*n as f64 - 69.0 + cents / 100.0) / 12.0)
                        * t;
                    value += phase.sin()
                        + if harmonics {
                            0.3 * (phase * 2.0).sin() + 0.15 * (phase * 3.0).sin()
                        } else {
                            0.0
                        };
                }
                let value = (value * 0.5 / notes.len() as f64) as f32;
                [value, -value]
            })
            .collect()
    }
    #[test]
    fn detuned_harmonic_major_minor_chords_survive_stereo_cancellation() {
        for (notes, label, camelot, cents, rate) in [
            ([60, 64, 67], "C major", "8B", 23.0, 44100),
            ([57, 60, 64], "A minor", "8A", -31.0, 48000),
            ([62, 65, 69], "D minor", "7A", 0.0, 8000),
        ] {
            let result = estimate_key(&chord(&notes, cents, rate, 2.0, true), rate).unwrap();
            assert_eq!(result.label, label);
            assert_eq!(result.camelot, camelot);
            assert!(result.confidence >= 0.3, "{result:?}");
        }
    }
    #[test]
    fn chromatic_single_tone_silence_and_noise_do_not_fabricate_keys() {
        for pcm in [
            vec![[0.0; 2]; 11025],
            chord(&[69], 0.0, 11025, 2.0, false),
            chord(&(60..72).collect::<Vec<_>>(), 0.0, 11025, 2.0, false),
        ] {
            assert!(estimate_key(&pcm, 11025).is_none());
        }
        let mut seed = 43_u32;
        let noise = (0..22050)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                [seed as f32 / u32::MAX as f32 * 0.2; 2]
            })
            .collect::<Vec<_>>();
        assert!(estimate_key(&noise, 11025).is_none());
    }
    #[test]
    fn progression_and_changing_timbre_retain_tonal_center() {
        let rate = 11025;
        let mut pcm = Vec::new();
        for _ in 0..3 {
            for notes in [[60, 64, 67], [60, 65, 69], [59, 62, 67], [60, 64, 67]] {
                pcm.extend(chord(&notes, 12.0, rate, 1.2, true));
            }
        }
        let key = analyze_key(&pcm, rate);
        assert_eq!(key.estimate.unwrap().label, "C major");
        assert!(key.candidates.len() >= 2);
    }
}

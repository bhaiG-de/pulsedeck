//! Beat-synchronous recurrence evidence; model activations are uncalibrated.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BarEvidence {
    #[serde(default)]
    pub pulse_repairs: Vec<PulseRepair>,
    pub beats_per_bar: usize,
    #[serde(default)]
    pub half_phase_corrected: bool,
    pub recurrence: f64,
    pub strong_windows: usize,
    pub windows: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PulseRepair {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub inserted_beats: usize,
    pub original_bpm: f64,
    pub corrected_bpm: f64,
    #[serde(default)]
    pub slow_bar_recurrence: f64,
    #[serde(default)]
    pub doubled_bar_recurrence: f64,
    #[serde(default)]
    pub bracketed_by_dominant_pulse: bool,
}
pub fn recurrence(mel: &[f32], beats: &[f64]) -> Vec<BarEvidence> {
    let mut evidence: Vec<_> = (2..=8)
        .map(|m| BarEvidence {
            beats_per_bar: m,
            half_phase_corrected: false,
            recurrence: 0.0,
            strong_windows: 0,
            windows: 0,
            pulse_repairs: Vec::new(),
        })
        .collect();
    if !mel.len().is_multiple_of(128) || beats.len() < 33 {
        return evidence;
    }
    let frames = mel.len() / 128;
    // One reusable beat window, independent of full source length.
    for start in (0..beats.len() - 1).step_by(64) {
        let end = (start + 64).min(beats.len() - 1);
        if end - start < 32 {
            continue;
        }
        let features: Vec<Vec<f64>> = beats[start..=end]
            .windows(2)
            .map(|w| {
                let first = ((w[0] + (w[1] - w[0]) * 0.15) * 50.0).round().max(0.0) as usize;
                let end = ((w[0] + (w[1] - w[0]) * 0.85) * 50.0).round().max(0.0) as usize;
                let mut feature = vec![0.0; 128];
                if first < end && end <= frames {
                    for frame in first..end {
                        for (k, v) in feature.iter_mut().enumerate() {
                            *v += f64::from(mel[frame * 128 + k]);
                        }
                    }
                    for v in &mut feature {
                        *v /= (end - first) as f64;
                    }
                }
                feature
            })
            .collect();
        let window = &features;
        let mut means = vec![0.0; 128];
        for feature in window {
            for (k, v) in feature.iter().enumerate() {
                means[k] += v / window.len() as f64;
            }
        }
        let scores: Vec<f64> = (2..=8)
            .map(|lag| {
                let (mut dot, mut norm_a, mut norm_b) = (0.0, 0.0, 0.0);
                for i in 0..window.len() - lag {
                    for (k, mean) in means.iter().enumerate().skip(12) {
                        let a = window[i][k] - mean;
                        let b = window[i + lag][k] - mean;
                        dot += a * b;
                        norm_a += a * a;
                        norm_b += b * b;
                    }
                }
                if norm_a * norm_b > 1e-12 {
                    dot / (norm_a * norm_b).sqrt()
                } else {
                    0.0
                }
            })
            .collect();
        for (i, score) in scores.iter().enumerate() {
            evidence[i].recurrence += score;
            evidence[i].windows += 1;
            let competitor = scores
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, x)| *x)
                .fold(f64::NEG_INFINITY, f64::max);
            if *score > 0.4 && *score - competitor > 0.1 {
                evidence[i].strong_windows += 1;
            }
        }
    }
    for e in &mut evidence {
        if e.windows > 0 {
            e.recurrence /= e.windows as f64;
        }
    }
    evidence
}
pub fn neural_bar_phase(beats: &[f64], downbeat: &[f32], meter: usize) -> (usize, f64) {
    let probability: Vec<f64> = beats
        .iter()
        .map(|t| {
            let i = (t * 50.0).round() as usize;
            let value = downbeat
                [i.saturating_sub(1).min(downbeat.len())..i.saturating_add(2).min(downbeat.len())]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            (1.0 / (1.0 + (-f64::from(value)).exp())).clamp(1e-5, 1.0 - 1e-5)
        })
        .collect();
    (0..meter)
        .map(|phase| {
            let logp = probability
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i % meter == phase {
                        p.ln()
                    } else {
                        (1.0 - p).ln()
                    }
                })
                .sum::<f64>();
            (phase, logp)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0))
}

fn flux(mel: &[f32], time: f64, bins: std::ops::Range<usize>) -> f64 {
    let frame = (time * 50.0).round().max(0.0) as usize;
    let frames = mel.len() / 128;
    (frame.saturating_sub(1).max(1)..=(frame + 1).min(frames.saturating_sub(1)))
        .map(|f| {
            bins.clone()
                .map(|bin| {
                    (f64::from(mel[f * 128 + bin]) - f64::from(mel[(f - 1) * 128 + bin])).max(0.0)
                })
                .sum::<f64>()
        })
        .fold(0.0, f64::max)
}
/// Only an extremely regular repeating source with independently agreeing
/// bass/mid transients can replace a neural half-beat phase choice.
pub fn correct_half_phase(mel: &[f32], beats: &[f64]) -> Option<Vec<f64>> {
    if beats.len() < 33 || mel.len() < 256 {
        return None;
    }
    let mut periods: Vec<f64> = beats.windows(9).map(|w| (w[8] - w[0]) / 8.0).collect();
    periods.sort_by(f64::total_cmp);
    let median = periods[periods.len() / 2];
    if periods[(periods.len() - 1) * 95 / 100] / periods[(periods.len() - 1) * 5 / 100] > 1.15 {
        return None;
    }
    let mut support = 0;
    for &t in beats {
        if t < median * 0.5 {
            continue;
        }
        let at = flux(mel, t, 0..24);
        let alternate = flux(mel, t - median * 0.5, 0..24);
        let mid = flux(mel, t, 24..64);
        let other = flux(mel, t - median * 0.5, 24..64);
        if alternate > at * 1.8 + 1e-5 && other > mid * 1.3 + 1e-5 {
            support += 1;
        }
    }
    if support * 4 < beats.len() * 3 {
        return None;
    }
    Some(
        beats
            .iter()
            .map(|t| t - median * 0.5)
            .filter(|t| *t >= 0.0)
            .collect(),
    )
}
pub fn accented_bar_phase(mel: &[f32], beats: &[f64], meter: usize) -> Option<usize> {
    let mut sums = vec![0.0; meter];
    let mut counts = vec![0usize; meter];
    for (i, &t) in beats.iter().enumerate() {
        sums[i % meter] += flux(mel, t, 0..24);
        counts[i % meter] += 1;
    }
    let mut scores: Vec<_> = sums
        .into_iter()
        .enumerate()
        .map(|(i, s)| (i, s / counts[i].max(1) as f64))
        .collect();
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    if scores.len() > 1 && scores[0].1 > scores[1].1 * 1.03 + 1e-5 {
        Some(scores[0].0)
    } else {
        None
    }
}

/// Repair a half-pulse passage only when the globally supported pulse and the
/// acoustic odd-bar pattern agree. New events are midpoint reconstructions;
/// this is not a claim of fresh neural detections or sample-level precision.
pub fn repair_odd_pulse(mel: &[f32], beats: &[f64], meter: usize) -> (Vec<f64>, Vec<PulseRepair>) {
    if beats.len() < 65 {
        return (beats.to_vec(), vec![]);
    }
    let mut periods: Vec<f64> = beats.windows(33).map(|w| (w[32] - w[0]) / 32.0).collect();
    periods.sort_by(f64::total_cmp);
    let target = periods[periods.len() / 2];
    let dominant_intervals = beats
        .windows(2)
        .filter(|w| (0.8..1.4).contains(&((w[1] - w[0]) / target)))
        .count();
    let blocks: Vec<_> = (0..beats.len() - 1)
        .step_by(32)
        .map(|start| {
            let end = (start + 32).min(beats.len() - 1);
            (
                start,
                end,
                (beats[end] - beats[start]) / (end - start) as f64,
            )
        })
        .collect();
    let mut result = Vec::with_capacity(beats.len());
    let mut repairs = Vec::new();
    for (block, &(start, end, local)) in blocks.iter().enumerate() {
        let doubled: Vec<f64> = beats[start..=end]
            .windows(2)
            .flat_map(|w| [w[0], (w[0] + w[1]) * 0.5])
            .chain([beats[end]])
            .collect();
        let score = if local / target > 1.6 && local / target < 2.4 && end - start >= 16 {
            recurrence(mel, &doubled)
                .into_iter()
                .find(|e| e.beats_per_bar == meter)
                .map(|e| e.recurrence)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        // A weak half-pulse run bracketed by the same dominant pulse can be
        // reconstructed, but a strong acoustic slow-bar pattern preserves a
        // real slowdown. All inferred events remain explicitly declared.
        let before = blocks[..block]
            .iter()
            .rev()
            .find(|(_, _, p)| *p < target * 1.4);
        let after = blocks[block + 1..]
            .iter()
            .find(|(_, _, p)| *p < target * 1.4);
        let slow_pattern = recurrence(mel, &beats[start..=end])
            .into_iter()
            .find(|e| e.beats_per_bar == meter)
            .map(|e| e.recurrence)
            .unwrap_or(0.0);
        let bracketed = local / target > 1.6
            && local / target < 2.4
            && end - start >= 16
            && dominant_intervals * 5 >= (beats.len() - 1) * 3
            && before.is_some_and(|(_, _, p)| (0.85..1.15).contains(&(*p / target)))
            && after.is_some_and(|(_, _, p)| (0.85..1.15).contains(&(*p / target)))
            && slow_pattern < 0.5;
        let reconstruct = score > 0.35 || bracketed;
        let inserted_beats = if reconstruct {
            beats[start..=end]
                .windows(2)
                .filter(|w| (1.6..2.4).contains(&((w[1] - w[0]) / target)))
                .count()
        } else {
            0
        };
        if inserted_beats > 0 {
            repairs.push(PulseRepair {
                start_seconds: beats[start],
                end_seconds: beats[end],
                inserted_beats,
                original_bpm: 60.0 / local,
                corrected_bpm: 60.0 * (end - start + inserted_beats) as f64
                    / (beats[end] - beats[start]),
                slow_bar_recurrence: slow_pattern,
                doubled_bar_recurrence: score,
                bracketed_by_dominant_pulse: bracketed,
            });
        }
        for w in beats[start..=end].windows(2) {
            result.push(w[0]);
            // A boundary block may contain both normal and missing pulses.
            // Split only its long intervals, preserving already detected beats.
            if reconstruct && (1.6..2.4).contains(&((w[1] - w[0]) / target)) {
                result.push((w[0] + w[1]) * 0.5);
            }
        }
    }
    result.push(*beats.last().unwrap());
    (result, repairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_replacement_requires_both_bands_and_preserves_correct_phase() {
        let truth: Vec<f64> = (0..160).map(|i| 0.2 + i as f64 * 0.5).collect();
        let offbeat: Vec<f64> = truth.iter().map(|t| t + 0.25).collect();
        let mut mel = vec![0.0; 4100 * 128];
        for &t in &truth {
            let f = (t * 50.0).round() as usize;
            mel[f * 128 + 10] = 4.0;
        }
        assert!(correct_half_phase(&mel, &offbeat).is_none());
        for &t in &truth {
            let f = (t * 50.0).round() as usize;
            mel[f * 128 + 40] = 4.0;
        }
        let corrected = correct_half_phase(&mel, &offbeat).unwrap();
        assert!(corrected
            .iter()
            .zip(&truth)
            .all(|(a, b)| (a - b).abs() < 1e-9));
        assert!(correct_half_phase(&mel, &truth).is_none());
    }
    #[test]
    fn missing_half_pulse_is_repaired_but_real_tempo_halving_is_preserved() {
        for real_slowdown in [false, true] {
            let truth: Vec<f64> = (0..=320)
                .map(|i| {
                    if real_slowdown && i > 192 {
                        96.0 + (i - 192) as f64
                    } else {
                        i as f64 * 0.5
                    }
                })
                .collect();
            let observed: Vec<f64> = truth
                .iter()
                .enumerate()
                .filter(|(i, _)| real_slowdown || *i < 192 || i % 2 == 0)
                .map(|(_, t)| *t)
                .collect();
            let frames = (truth[320] * 50.0).round() as usize + 1;
            let mut mel = vec![0.0_f32; frames * 128];
            for i in 0..320 {
                for frame in
                    (truth[i] * 50.0).round() as usize..(truth[i + 1] * 50.0).round() as usize
                {
                    mel[frame * 128 + 32 + i % 5] = 4.0;
                }
            }
            let (actual, repairs) = repair_odd_pulse(&mel, &observed, 5);
            assert_eq!(actual, truth);
            assert_eq!(
                repairs.iter().map(|r| r.inserted_beats).sum::<usize>(),
                if real_slowdown { 0 } else { 64 }
            );
        }
    }
    #[test]
    fn returning_intentional_slowdown_is_preserved_despite_matching_flanks() {
        let truth: Vec<f64> = (0..=320)
            .map(|i| {
                if i <= 192 {
                    i as f64 * 0.5
                } else if i <= 256 {
                    96.0 + (i - 192) as f64
                } else {
                    160.0 + (i - 256) as f64 * 0.5
                }
            })
            .collect();
        let frames = (truth[320] * 50.0) as usize + 1;
        let mut mel = vec![0.0; frames * 128];
        for i in 0..320 {
            for f in (truth[i] * 50.0) as usize..(truth[i + 1] * 50.0) as usize {
                mel[f * 128 + 32 + i % 5] = 4.0;
            }
        }
        let (actual, repairs) = repair_odd_pulse(&mel, &truth, 5);
        assert_eq!(actual, truth);
        assert!(repairs.is_empty());
    }
    #[test]
    fn odd_recurrence_is_distinct_from_duple_repetition_and_silence() {
        for meter in [4, 5, 7] {
            let beats: Vec<f64> = (0..321).map(|i| i as f64 * 0.5).collect();
            let mut mel = vec![0.0_f32; 8001 * 128];
            for frame in 0..8001 {
                let phase = (frame / 25) % meter;
                mel[frame * 128 + 32 + phase] = 4.0;
            }
            let evidence = recurrence(&mel, &beats);
            if meter != 4 {
                assert!(
                    evidence
                        .iter()
                        .find(|e| e.beats_per_bar == meter)
                        .unwrap()
                        .strong_windows
                        >= 2
                );
            }
            assert!(evidence
                .iter()
                .filter(|e| e.beats_per_bar != meter)
                .all(|e| e.strong_windows == 0));
        }
        assert!(recurrence(
            &vec![0.0; 8001 * 128],
            &(0..321).map(|i| i as f64 * 0.5).collect::<Vec<_>>()
        )
        .iter()
        .all(|e| e.strong_windows == 0));
    }
}

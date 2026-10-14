//! Beat This! logits adapted to a BSD madmom-style temporal/meter decoder.
//! Only decoder code is vendored; its original BLSTM models are never used.
//! This competes whole-track 3/4/5/7 hypotheses, not arbitrary changing meter.
mod bars;
mod decoder;
pub use bars::{recurrence as bar_recurrence_for_validation, BarEvidence, PulseRepair};

// The vendored implementation's narrow support surface. Deliberately omit all
// model loading, feature extraction and provider/file analysis entry points.
mod types {
    pub(super) struct ActivationOutput {
        pub beat: Vec<f32>,
        pub downbeat: Vec<f32>,
    }
    pub(super) struct BeatEvent {
        pub time_sec: f32,
        pub beat_in_bar: usize,
        pub confidence: f32,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum ProgressStage {
        Dbn,
    }
    #[derive(Clone, Copy, Debug)]
    pub(super) struct ProgressEvent {
        pub stage: ProgressStage,
        pub progress: f32,
    }
    pub(super) trait ProgressSink {
        fn on_progress(&mut self, event: ProgressEvent);
    }
    impl<F: FnMut(ProgressEvent)> ProgressSink for F {
        fn on_progress(&mut self, event: ProgressEvent) {
            self(event);
        }
    }
}
struct FeatureConfig {
    fps: f32,
}
struct DbnConfig {
    beats_per_bar: Vec<usize>,
    min_bpm: f32,
    max_bpm: f32,
    num_tempi: usize,
    transition_lambda: f32,
    observation_lambda: usize,
    threshold: f32,
    correct: bool,
}
struct CoreConfig {
    feature: FeatureConfig,
    dbn: DbnConfig,
}
impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            feature: FeatureConfig { fps: 50.0 },
            dbn: DbnConfig {
                beats_per_bar: vec![3, 4],
                min_bpm: 55.0,
                max_bpm: 300.0,
                num_tempi: 60,
                transition_lambda: 100.0,
                observation_lambda: 16,
                threshold: 0.05,
                correct: true,
            },
        }
    }
}
#[derive(Debug)]
enum RhythmError {
    InvalidInput(String),
    Model(String),
}
impl std::fmt::Display for RhythmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) | Self::Model(message) => f.write_str(message),
        }
    }
}

/// Clamp-free upstream Beat This! madmom adapter: epsilon avoids zero densities;
/// ordinary beat excludes downbeat mass. No madmom model inference is involved.
pub(crate) fn decode_logits(
    beat: &[f32],
    downbeat: &[f32],
) -> Result<(Vec<f64>, Vec<f64>), String> {
    let (beats, downbeats) =
        decode_neural_logits_with_meters_for_validation(beat, downbeat, 300.0, &[3, 4, 5, 7])?;
    Ok(refine_event_times(beats, downbeats, beat))
}

/// Independent beat clock for testing whether downbeat errors corrupt tempo.
#[doc(hidden)]
pub fn decode_independent_beats_for_validation(beat: &[f32]) -> Result<Vec<f64>, String> {
    if beat.is_empty() || beat.iter().any(|v| !v.is_finite()) {
        return Err("invalid beat logits".into());
    }
    let activation = types::ActivationOutput {
        beat: vec![0.0; beat.len()],
        downbeat: beat
            .iter()
            .map(|x| (1.0 / (1.0 + (-f64::from(*x)).exp())).clamp(1e-5, 1.0 - 1e-5) as f32)
            .collect(),
    };
    let mut config = CoreConfig::default();
    config.dbn.beats_per_bar = vec![1];
    let mut progress = |_event: types::ProgressEvent| {};
    let events = decoder::decode(&activation, &config, &mut progress).map_err(|e| e.to_string())?;
    let beats = events
        .into_iter()
        .map(|e| f64::from(e.time_sec))
        .collect::<Vec<_>>();
    Ok(refine_event_times(beats, vec![], beat).0)
}

#[doc(hidden)]
pub fn decode_separated_for_validation(
    beat: &[f32],
    downbeat: &[f32],
) -> Result<(Vec<f64>, Vec<f64>), String> {
    let beats = decode_independent_beats_for_validation(beat)?;
    let (meter, phase, _) = [3, 4, 5, 7]
        .into_iter()
        .map(|m| {
            let (p, s) = bars::neural_bar_phase(&beats, downbeat, m);
            (m, p, s)
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .unwrap();
    let down = beats
        .iter()
        .enumerate()
        .filter(|(i, _)| i % meter == phase)
        .map(|(_, t)| *t)
        .collect();
    Ok((beats, down))
}

#[doc(hidden)]
pub type AcousticMeterResult = (Vec<f64>, Vec<f64>, Vec<BarEvidence>);

/// Independent acoustic recurrence can identify an odd bar period despite a
/// conflicting neural downbeat head. Weak/repeating duple evidence is ignored.
#[doc(hidden)]
pub fn decode_with_acoustic_meter_for_validation(
    beat: &[f32],
    downbeat: &[f32],
    mel: &[f32],
) -> Result<AcousticMeterResult, String> {
    let mut independent = decode_independent_beats_for_validation(beat)?;
    let mut phase_corrected = false;
    let mut evidence = bars::recurrence(mel, &independent);
    if evidence
        .iter()
        .any(|e| [3, 4, 5, 7].contains(&e.beats_per_bar) && e.recurrence > 0.75 && e.windows >= 2)
    {
        if let Some(corrected) = bars::correct_half_phase(mel, &independent) {
            independent = corrected;
            phase_corrected = true;
            evidence = bars::recurrence(mel, &independent);
        }
    }
    let strong = evidence.iter().map(|e| e.strong_windows).sum::<usize>();
    let selected = evidence
        .iter()
        .filter(|e| {
            let other = evidence
                .iter()
                .filter(|x| {
                    x.beats_per_bar != e.beats_per_bar && x.beats_per_bar % e.beats_per_bar != 0
                })
                .map(|x| x.recurrence)
                .fold(f64::NEG_INFINITY, f64::max);
            ([3, 4, 5, 7].contains(&e.beats_per_bar)
                && e.windows >= 2
                && e.recurrence > 0.75
                && e.recurrence - other > 0.12)
                || ([5, 7].contains(&e.beats_per_bar)
                    && ((e.strong_windows >= 2 && e.strong_windows * 4 >= strong * 3)
                        || (e.windows >= 4
                            && e.strong_windows >= 1
                            && e.recurrence > 0.25
                            && e.recurrence - other > 0.075)))
        })
        .max_by(|a, b| a.recurrence.total_cmp(&b.recurrence));
    if let Some(e) = selected {
        let meter = e.beats_per_bar;
        let (independent, repairs) = bars::repair_odd_pulse(mel, &independent, meter);
        evidence
            .iter_mut()
            .find(|e| e.beats_per_bar == meter)
            .unwrap()
            .pulse_repairs = repairs;
        let phase = bars::accented_bar_phase(mel, &independent, meter)
            .unwrap_or_else(|| bars::neural_bar_phase(&independent, downbeat, meter).0);
        evidence
            .iter_mut()
            .find(|e| e.beats_per_bar == meter)
            .unwrap()
            .half_phase_corrected = phase_corrected;
        let down = independent
            .iter()
            .enumerate()
            .filter(|(i, _)| i % meter == phase)
            .map(|(_, t)| *t)
            .collect();
        Ok((independent, down, evidence))
    } else {
        let (beats, down) = decode_logits(beat, downbeat)?;
        Ok((beats, down, evidence))
    }
}

/// Run the current app decoder on independently labeled evaluation activations.
#[doc(hidden)]
pub fn decode_neural_logits_for_app_validation(
    beat: &[f32],
    downbeat: &[f32],
) -> Result<(Vec<f64>, Vec<f64>), String> {
    let (beats, downbeats) = decode_logits(beat, downbeat)?;
    // PCM analysis stores event times as f32 in BeatAnalysis before publication.
    Ok((
        beats.into_iter().map(|t| f64::from(t as f32)).collect(),
        downbeats.into_iter().map(|t| f64::from(t as f32)).collect(),
    ))
}

/// Decode exact saved model activations for reproducible temporal-model
/// comparison without rerunning neural inference. No provider audio is read.
#[doc(hidden)]
pub fn decode_neural_logits_for_validation(
    beat: &[f32],
    downbeat: &[f32],
    max_bpm: f32,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    decode_neural_logits_with_meters_for_validation(beat, downbeat, max_bpm, &[3, 4])
}

/// Evaluation-only bounded meter hypothesis comparison.
#[doc(hidden)]
pub fn decode_neural_logits_with_meters_for_validation(
    beat: &[f32],
    downbeat: &[f32],
    max_bpm: f32,
    meters: &[usize],
) -> Result<(Vec<f64>, Vec<f64>), String> {
    if meters.is_empty() || meters.len() > 6 || meters.iter().any(|m| !(2..=7).contains(m)) {
        return Err("DBN needs one to six meter candidates in 2..=7".into());
    }
    if !max_bpm.is_finite() || !(56.0..=600.0).contains(&max_bpm) {
        return Err("DBN maximum tempo must be finite and between 56 and 600 BPM".into());
    }
    if beat.is_empty()
        || beat.len() != downbeat.len()
        || beat.iter().chain(downbeat).any(|v| !v.is_finite())
    {
        return Err("DBN rejected empty, mismatched or non-finite logits".into());
    }
    const EPSILON: f64 = 1.0e-5;
    // Beat This!'s Python adapter converts logits to double before sigmoid.
    // Narrow only after the identical activation formula, at the port boundary.
    let probability =
        |logit: f32| 1.0 / (1.0 + (-f64::from(logit)).exp()) * (1.0 - EPSILON) + EPSILON / 2.0;
    let mut ordinary = Vec::with_capacity(beat.len());
    let mut down = Vec::with_capacity(beat.len());
    for (&beat, &downbeat) in beat.iter().zip(downbeat) {
        let b = probability(beat);
        let d = probability(downbeat);
        ordinary.push((b - d).max(EPSILON / 2.0) as f32);
        down.push(d as f32);
    }
    let activations = types::ActivationOutput {
        beat: ordinary,
        downbeat: down,
    };
    let mut no_progress = |event: types::ProgressEvent| {
        let _ = (event.stage, event.progress);
    };
    let mut config = CoreConfig::default();
    config.dbn.max_bpm = max_bpm;
    config.dbn.beats_per_bar = meters.to_vec();
    let events = decoder::decode(&activations, &config, &mut no_progress)
        .map_err(|error| format!("DBN beat decoding failed: {error}"))?;
    let mut beats = Vec::with_capacity(events.len());
    let mut downbeats = Vec::new();
    for event in events {
        let time = f64::from(event.time_sec);
        let _ = event.confidence; // Preserve Beat This! separate uncalibrated scores instead.
        beats.push(time);
        if event.beat_in_bar == 1 {
            downbeats.push(time);
        }
    }
    Ok((beats, downbeats))
}

/// Fit the beat-logit maximum over its adjacent frames. Shift at most half a
/// frame (10 ms), only for a concave local maximum. Downbeats reuse the exact
/// same refined beat, preserving subset membership and event ordering.
fn refine_event_times(
    beats: Vec<f64>,
    downbeats: Vec<f64>,
    logits: &[f32],
) -> (Vec<f64>, Vec<f64>) {
    let refined: Vec<f64> = beats
        .iter()
        .map(|&time| {
            let frame = (time * 50.0).round() as usize;
            if frame == 0 || frame + 1 >= logits.len() {
                return time;
            }
            let (left, center, right) = (
                f64::from(logits[frame - 1]),
                f64::from(logits[frame]),
                f64::from(logits[frame + 1]),
            );
            let curvature = left - 2.0 * center + right;
            let shift = if curvature < 0.0 && center >= left && center >= right {
                (0.5 * (left - right) / curvature).clamp(-0.5, 0.5)
            } else {
                0.0
            };
            (frame as f64 + shift) / 50.0
        })
        .collect();
    let refined_downbeats = downbeats
        .iter()
        .filter_map(|time| {
            beats
                .binary_search_by(|beat| beat.total_cmp(time))
                .ok()
                .map(|index| refined[index])
        })
        .collect();
    (refined, refined_downbeats)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pulses(
        meter: usize,
        extra_downbeats: bool,
        variable: bool,
    ) -> (Vec<f32>, Vec<f32>, Vec<f64>) {
        let mut beat = vec![-12.0; 2500];
        let mut downbeat = vec![-12.0; 2500];
        let mut times = Vec::new();
        let mut frame = 10;
        let mut index = 0;
        while frame < 2490 {
            beat[frame] = 8.0;
            if index % meter == 0 {
                downbeat[frame] = 8.0;
            } else if extra_downbeats {
                downbeat[frame] = 0.3;
            }
            times.push(frame as f64 / 50.0);
            frame += if variable && index > 40 { 23 } else { 25 };
            index += 1;
        }
        (beat, downbeat, times)
    }
    #[test]
    fn regular_and_odd_meters_retain_activation_times() {
        for meter in [3, 4, 5, 7] {
            let (beat, downbeat, expected) = pulses(meter, false, false);
            let (actual, down) = decode_logits(&beat, &downbeat).unwrap();
            assert_eq!(actual.len(), expected.len());
            assert!(actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (*a - b).abs() < 1e-5));
            let indices: Vec<_> = down
                .iter()
                .map(|t| actual.iter().position(|b| b == t).unwrap())
                .collect();
            assert!(indices.windows(2).all(|w| w[1] - w[0] == meter));
        }
    }
    #[test]
    fn temporal_meter_model_rejects_excess_weak_downbeat_peaks() {
        let (beat, downbeat, _) = pulses(4, true, false);
        let (actual, down) = decode_logits(&beat, &downbeat).unwrap();
        let indices: Vec<_> = down
            .iter()
            .map(|t| actual.iter().position(|b| b == t).unwrap())
            .collect();
        assert!(indices.windows(2).all(|w| w[1] - w[0] == 4));
        assert!(down.len() < actual.len() / 2);
    }
    #[test]
    fn tempo_change_preserves_local_timing_instead_of_uniformizing() {
        let (beat, downbeat, expected) = pulses(4, false, true);
        let (actual, _) = decode_logits(&beat, &downbeat).unwrap();
        assert_eq!(actual.len(), expected.len());
        assert!(actual
            .iter()
            .zip(expected)
            .all(|(a, b)| (*a - b).abs() < 1e-5));
        assert!((actual[20] - actual[19] - 0.5).abs() < 1e-5);
        assert!((actual[60] - actual[59] - 0.46).abs() < 1e-5);
    }
    #[test]
    fn subframe_refinement_is_bounded_and_downbeats_share_the_refined_event() {
        let logits = [0.0, 1.0, 4.0, 3.0, 0.0];
        let (beats, down) = refine_event_times(vec![0.04], vec![0.04], &logits);
        assert!((beats[0] - 0.045).abs() < 1e-12);
        assert_eq!(beats, down);
        let (beats, _) = refine_event_times(vec![0.0, 0.02, 0.08], vec![], &logits);
        assert_eq!(beats, vec![0.0, 0.02, 0.08]);
    }
    #[test]
    fn silence_and_invalid_logits_do_not_invent_events() {
        assert_eq!(
            decode_logits(&[-30.0; 100], &[-30.0; 100]).unwrap(),
            (vec![], vec![])
        );
        assert!(decode_logits(&[], &[]).is_err());
        assert!(decode_logits(&[0.0], &[0.0, 0.0]).is_err());
        assert!(decode_logits(&[f32::NAN], &[0.0]).is_err());
    }
    #[test]
    fn dj_tempo_range_retains_fast_regular_music() {
        let mut beat = vec![-12.0; 1650];
        let mut downbeat = vec![-12.0; 1650];
        for index in 0..120 {
            let frame = 10 + (index as f64 * 50.0 * 60.0 / 228.0).round() as usize;
            beat[frame] = 8.0;
            if index % 4 == 0 {
                downbeat[frame] = 8.0;
            }
        }
        let (actual, down) = decode_logits(&beat, &downbeat).unwrap();
        assert_eq!(actual.len(), 120);
        assert_eq!(down.len(), 30);
        assert!((60.0 * 32.0 / (actual[40] - actual[8]) - 228.0).abs() < 0.5);
        assert!(decode_neural_logits_for_validation(&beat, &downbeat, f32::NAN).is_err());
    }
}

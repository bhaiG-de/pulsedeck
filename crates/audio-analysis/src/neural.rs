//! Beat This! full final0 inference on PCM, always outside the audio callback.
//! Model output is 50 Hz (20 ms frames), with bounded half-frame logit refinement. Scores
//! summarize model activations and are not calibrated accuracy probabilities.
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use anyhow::{anyhow, Context};
use beat_this::{BeatAnalysis, BeatThis, Model, Runtime, Tensor};
use rten::{NodeId, RunOptions, ThreadPool, ValueView};
use rten_tensor::Layout;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MODEL_ID: &str =
    "Beat This! final0 FP32 + DBN 3/4/5/7 55-300 BPM + acoustic bar/pulse/phase evidence + bounded logit timing / beat-this 1.1.0 / RTen 0.24.0";
const MINIMAL_MODEL_ID: &str = "Beat This! final0 FP32 + minimal / beat-this 1.1.0 / RTen 0.24.0";
const BEAT_SHA256: &str = "5f810debe53459b559127fb55bbad40035bb47cc567b20e501670f968c770f02";
const MEL_SHA256: &str = "fdd59e65c515331308e4c8841edf99972deca646bdf6197744c2a5b7755e3de9";
const FRAMES_PER_SECOND: f64 = 50.0;
const MAX_DURATION_SECONDS: f64 = 7200.0;
type Tracker = BeatThis<BoundedModel>;
static TRACKER: OnceLock<Mutex<Option<Tracker>>> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NeuralRhythm {
    /// Predicted nonuniform source times, including explicitly declared acoustic
    /// phase adjustments and midpoint reconstructions in bar_evidence.
    pub beats_seconds: Vec<f64>,
    /// A sorted subset of beats. Default DBN selects a whole-track 3/4/5/7 meter
    /// hypothesis; minimal benchmark mode imposes no fixed meter.
    pub downbeats_seconds: Vec<f64>,
    /// Median local tempo across up to 32-beat windows to reduce 20 ms frame
    /// quantization bias. The individual timestamp vectors stay authoritative.
    pub bpm: f64,
    /// Mean sigmoid activation near selected beats; uncalibrated.
    pub beat_confidence: f32,
    /// Mean sigmoid activation near selected downbeats; uncalibrated.
    pub downbeat_confidence: f32,
    pub model: String,
    /// Elapsed analysis wall time, including waiting for other analyses.
    pub analysis_seconds: f64,
    /// Independent beat-synchronous acoustic recurrence; not calibrated probabilities.
    #[serde(default)]
    pub bar_evidence: Vec<crate::BarEvidence>,
    #[serde(default)]
    pub beats_per_bar: Option<usize>,
}

/// Optionally warm up verified model loading on a background thread. Successful
/// loading is retained for the process; missing/invalid assets are retryable.
pub fn initialize_neural_rhythm() -> Result<(), String> {
    let mut tracker = TRACKER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "neural beat analysis lock is poisoned".to_string())?;
    if tracker.is_none() {
        *tracker = Some(load_tracker()?);
    }
    Ok(())
}

/// Analyze decoded stereo PCM in memory. No source audio is written to disk or
/// decoded a second time. Call only on a background worker: the model mutex
/// serializes complete analyses and its private pool is limited to two threads
/// by default. A missing model returns an error rather than a spectral result.
pub fn analyze_neural_rhythm(
    samples: &[[f32; 2]],
    sample_rate: u32,
) -> Result<NeuralRhythm, String> {
    analyze_neural_rhythm_with_logit_observer(samples, sample_rate, |_, _| Ok(()))
}

/// Validation instrumentation: observe the exact neural logits before temporal
/// decoding. The callback receives derived activations only, never source PCM,
/// and runs on the same background analysis worker. Normal app analysis does
/// not export anything. Useful for same-activation Rust/Python decoder parity.
#[doc(hidden)]
pub fn analyze_neural_rhythm_with_logit_observer(
    samples: &[[f32; 2]],
    sample_rate: u32,
    observe: impl FnOnce(&[f32], &[f32]) -> Result<(), String>,
) -> Result<NeuralRhythm, String> {
    let started = Instant::now();
    let duration = validate_audio(samples, sample_rate)?;
    let mut guard = TRACKER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "neural beat analysis lock is poisoned".to_string())?;
    if guard.is_none() {
        *guard = Some(load_tracker()?);
    }
    let tracker = guard.as_mut().expect("initialized above");
    let mut stream = tracker
        .stream(sample_rate)
        .map_err(|e| format!("neural beat stream: {e:#}"))?;
    // One reusable mono scratch chunk, never a second track-sized PCM buffer.
    let mut mono = Vec::with_capacity(8192);
    for chunk in samples.chunks(8192) {
        mono.clear();
        mono.extend(chunk.iter().map(|s| 0.5 * s[0] + 0.5 * s[1]));
        stream
            .push(&mono)
            .map_err(|e| format!("neural beat input: {e:#}"))?;
    }
    let mut analysis = stream
        .finish()
        .map_err(|e| format!("neural beat inference: {e:#}"))?;
    observe(&analysis.beat_logits, &analysis.downbeat_logits)?;
    let minimal = match std::env::var("PULSEDECK_BEAT_POSTPROCESS") {
        Ok(mode) if mode == "minimal" => true,
        Ok(mode) if mode == "dbn" => false,
        Err(std::env::VarError::NotPresent) => false,
        _ => {
            return Err("PULSEDECK_BEAT_POSTPROCESS must be dbn or minimal (benchmark only)".into())
        }
    };
    let mut bar_evidence = Vec::new();
    if !minimal {
        let (beats, downbeats, evidence) = crate::dbn::decode_with_acoustic_meter_for_validation(
            &analysis.beat_logits,
            &analysis.downbeat_logits,
            &analysis.mel.data,
        )?;
        bar_evidence = evidence;
        analysis.beats = beats.into_iter().map(|time| time as f32).collect();
        analysis.downbeats = downbeats.into_iter().map(|time| time as f32).collect();
    }
    let mut result = finish_analysis(analysis, duration, started.elapsed().as_secs_f64())?;
    result.bar_evidence = bar_evidence;
    let mut meter_counts = std::collections::BTreeMap::new();
    for pair in result.downbeats_seconds.windows(2) {
        let a = result
            .beats_seconds
            .binary_search_by(|b| b.total_cmp(&pair[0]))
            .unwrap();
        let b = result
            .beats_seconds
            .binary_search_by(|b| b.total_cmp(&pair[1]))
            .unwrap();
        *meter_counts.entry(b - a).or_insert(0usize) += 1;
    }
    result.beats_per_bar = meter_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(meter, _)| meter);
    if minimal {
        result.model = MINIMAL_MODEL_ID.into();
    }
    result.analysis_seconds = started.elapsed().as_secs_f64();
    Ok(result)
}

/// Evaluation-only hook for the authors' published mel spectrogram benchmark.
/// Uses the same verified model, bounded runtime and chunk/overlap code as PCM
/// inference; does not test decoding/resampling/mel extraction from audio.
#[doc(hidden)]
pub fn neural_logits_from_spectrogram_for_validation(
    frames: Vec<f32>,
) -> Result<(Vec<f32>, Vec<f32>), String> {
    if frames.is_empty()
        || !frames.len().is_multiple_of(128)
        || frames.iter().any(|v| !v.is_finite())
        || frames.len() / 128 > (MAX_DURATION_SECONDS * FRAMES_PER_SECOND) as usize
    {
        return Err("invalid validation mel spectrogram".into());
    }
    initialize_neural_rhythm()?;
    let mut guard = TRACKER
        .get()
        .expect("initialized above")
        .lock()
        .map_err(|_| "neural beat analysis lock is poisoned".to_string())?;
    let analysis = beat_this::__probe::predict_decode(
        guard.as_mut().expect("initialized above"),
        Tensor {
            shape: vec![1, frames.len() / 128, 128],
            data: frames,
        },
    )
    .map_err(|e| format!("validation spectrogram inference: {e:#}"))?;
    Ok((analysis.beat_logits, analysis.downbeat_logits))
}

fn validate_audio(samples: &[[f32; 2]], rate: u32) -> Result<f64, String> {
    if rate == 0 || samples.is_empty() {
        return Err("neural beat analysis needs non-empty PCM and a non-zero sample rate".into());
    }
    let duration = samples.len() as f64 / f64::from(rate);
    if !(3.0..=MAX_DURATION_SECONDS).contains(&duration) {
        return Err(
            "neural beat analysis supports source durations from 3 seconds to 2 hours".into(),
        );
    }
    let mut energy = 0.0_f64;
    let mut peak = 0.0_f32;
    for sample in samples {
        if sample.iter().any(|v| !v.is_finite()) {
            return Err("neural beat analysis rejected non-finite PCM".into());
        }
        let mono = 0.5 * sample[0] + 0.5 * sample[1];
        energy += f64::from(mono).powi(2);
        peak = peak.max(mono.abs());
    }
    if peak <= 1.0e-5 || (energy / samples.len() as f64).sqrt() <= 1.0e-7 {
        return Err("neural beat analysis abstained: silent mono audio".into());
    }
    Ok(duration)
}

fn finish_analysis(
    analysis: BeatAnalysis,
    duration: f64,
    analysis_seconds: f64,
) -> Result<NeuralRhythm, String> {
    let mut beats: Vec<f64> = analysis.beats.iter().map(|&t| f64::from(t)).collect();
    let mut downbeats: Vec<f64> = analysis.downbeats.iter().map(|&t| f64::from(t)).collect();
    // Centered STFT produces 1 + samples/hop frames: a source ending exactly
    // at a hop may produce an event at EOF, where no source sample exists.
    // Validate the raw timeline with one frame's rounding allowance, then drop
    // only end-padding events. No in-source event is moved or regularized.
    validate_events(
        &beats,
        &downbeats,
        duration + 1.0 / FRAMES_PER_SECOND + 1.0e-4,
    )?;
    beats.retain(|&time| time < duration);
    downbeats.retain(|&time| time < duration);
    validate_events(&beats, &downbeats, duration)?;
    if analysis.beat_logits.is_empty()
        || analysis.beat_logits.len() != analysis.downbeat_logits.len()
        || analysis
            .beat_logits
            .iter()
            .chain(&analysis.downbeat_logits)
            .any(|v| !v.is_finite())
    {
        return Err("neural beat model returned invalid activations".into());
    }
    let bpm = summarize_bpm(&beats);
    if !bpm.is_finite() || !(20.0..=600.0).contains(&bpm) {
        return Err("neural beat analysis abstained: implausible median tempo".into());
    }
    Ok(NeuralRhythm {
        beat_confidence: peak_confidence(&beats, &analysis.beat_logits),
        downbeat_confidence: peak_confidence(&downbeats, &analysis.downbeat_logits),
        beats_seconds: beats,
        downbeats_seconds: downbeats,
        bpm,
        model: MODEL_ID.into(),
        analysis_seconds,
        bar_evidence: Vec::new(),
        beats_per_bar: None,
    })
}

fn summarize_bpm(beats: &[f64]) -> f64 {
    // A single interval toggles between 20 ms bins and can bias DJ rate by
    // several percent (174 BPM may display as 176.47). Multi-beat slopes reduce
    // that quantization without regularizing any predicted beat event. Sliding
    // windows keep this a robust local summary when a track changes tempo.
    let span = [32, 16, 8, 4, 1]
        .into_iter()
        .find(|&n| beats.len() > n)
        .unwrap_or(1);
    let mut intervals: Vec<f64> = beats
        .windows(span + 1)
        .map(|window| (window[span] - window[0]) / span as f64)
        .collect();
    intervals.sort_by(f64::total_cmp);
    let middle = intervals.len() / 2;
    let median = if intervals.len().is_multiple_of(2) {
        (intervals[middle - 1] + intervals[middle]) * 0.5
    } else {
        intervals[middle]
    };
    60.0 / median
}

fn validate_events(beats: &[f64], downbeats: &[f64], duration: f64) -> Result<(), String> {
    if beats.len() < 4 {
        return Err("neural beat analysis abstained: fewer than four beat events".into());
    }
    for (name, events) in [("beat", beats), ("downbeat", downbeats)] {
        if let Some((index, time)) = events
            .iter()
            .enumerate()
            .find(|(_, t)| !t.is_finite() || **t < 0.0 || **t >= duration)
        {
            return Err(format!("neural model returned invalid {name} timestamp {time} at index {index} for {duration}s source"));
        }
        if events.windows(2).any(|w| w[1] <= w[0]) {
            return Err(format!(
                "neural model returned non-monotonic {name} timestamps"
            ));
        }
    }
    if downbeats
        .iter()
        .any(|time| beats.binary_search_by(|beat| beat.total_cmp(time)).is_err())
    {
        return Err("neural model downbeats are not a subset of beat events".into());
    }
    Ok(())
}

fn peak_confidence(times: &[f64], logits: &[f32]) -> f32 {
    if times.is_empty() {
        return 0.0;
    }
    // ±3 frames follows the upstream peak-picking window. It also accounts for
    // plateau averaging and downbeats snapped onto a nearby beat event.
    times
        .iter()
        .map(|&t| {
            let frame = (t * FRAMES_PER_SECOND).round() as usize;
            let start = frame.saturating_sub(3).min(logits.len());
            let end = frame.saturating_add(4).min(logits.len());
            let logit = logits[start..end]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            1.0 / (1.0 + (-logit).exp())
        })
        .sum::<f32>()
        / times.len() as f32
}

fn load_tracker() -> Result<Tracker, String> {
    let directory = std::env::var_os("PULSEDECK_BEAT_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/beat-models")
        });
    let mel_path = directory.join("mel_spectrogram.onnx");
    let beat_path = directory.join("beat_this.onnx");
    verify_model(&mel_path, 270_742, MEL_SHA256)?;
    verify_model(&beat_path, 83_162_650, BEAT_SHA256)?;
    let thread_count = match std::env::var("PULSEDECK_BEAT_THREADS") {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=8).contains(n))
            .ok_or_else(|| "PULSEDECK_BEAT_THREADS must be an integer from 1 to 8".to_string())?,
        Err(_) => 2,
    };
    let runtime = BoundedRuntime {
        pool: Arc::new(ThreadPool::with_num_threads(thread_count)),
    };
    BeatThis::new(&runtime, &mel_path, &beat_path)
        .map_err(|e| format!("neural beat model unavailable: {e:#}"))
}

fn verify_model(path: &Path, size: u64, expected: &str) -> Result<(), String> {
    let checked = || -> anyhow::Result<()> {
        let mut file = File::open(path)?;
        if file.metadata()?.len() != size {
            return Err(anyhow!("model size mismatch; expected {size} bytes"));
        }
        let mut hash = Sha256::new();
        let mut chunk = [0_u8; 65_536];
        loop {
            let len = file.read(&mut chunk)?;
            if len == 0 {
                break;
            }
            hash.update(&chunk[..len]);
        }
        if format!("{:x}", hash.finalize()) != expected {
            return Err(anyhow!("model SHA-256 mismatch"));
        }
        Ok(())
    };
    checked().map_err(|e| {
        format!(
            "neural beat model unavailable ({}): {e:#}; run python3 tools/ensure-beat-models.py",
            path.display()
        )
    })
}

// Per-model RTen adapter derived from beat-this-rs (MIT; notice shipped alongside
// weights), with a private bounded pool instead of process-global thread settings.
struct BoundedRuntime {
    pool: Arc<ThreadPool>,
}
struct BoundedModel {
    graph: rten::Model,
    inputs: HashMap<String, NodeId>,
    outputs: Vec<(NodeId, String)>,
    options: RunOptions,
}

impl Runtime for BoundedRuntime {
    type Model = BoundedModel;
    fn load_model(&self, path: &Path) -> anyhow::Result<Self::Model> {
        let graph = rten::Model::load_file(path)?;
        let inputs = graph
            .input_ids()
            .iter()
            .map(|&id| {
                let name = graph
                    .node_info(id)
                    .and_then(|info| info.name())
                    .ok_or_else(|| anyhow!("neural model input has no name"))?;
                Ok((name.to_string(), id))
            })
            .collect::<anyhow::Result<_>>()?;
        let outputs = graph
            .output_ids()
            .iter()
            .map(|&id| {
                let name = graph
                    .node_info(id)
                    .and_then(|info| info.name())
                    .ok_or_else(|| anyhow!("neural model output has no name"))?;
                Ok((id, name.to_string()))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(BoundedModel {
            graph,
            inputs,
            outputs,
            options: RunOptions::default().with_thread_pool(Some(Arc::clone(&self.pool))),
        })
    }
}

impl Model for BoundedModel {
    fn run(&mut self, inputs: &[(&str, &Tensor)]) -> anyhow::Result<HashMap<String, Tensor>> {
        let inputs = inputs
            .iter()
            .map(|(name, tensor)| {
                let id = *self
                    .inputs
                    .get(*name)
                    .ok_or_else(|| anyhow!("unknown neural model input {name}"))?;
                let view = ValueView::from_shape(tensor.shape.as_slice(), tensor.data.as_slice())
                    .with_context(|| format!("invalid neural model tensor {name}"))?;
                Ok((id, view.into()))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let output_ids: Vec<_> = self.outputs.iter().map(|(id, _)| *id).collect();
        let values = self
            .graph
            .run(inputs, &output_ids, Some(self.options.clone()))?;
        self.outputs
            .iter()
            .zip(values)
            .map(|((_, name), value)| {
                let tensor = value
                    .into_tensor::<f32>()
                    .ok_or_else(|| anyhow!("non-f32 neural model output {name}"))?;
                Ok((
                    name.clone(),
                    Tensor {
                        shape: tensor.shape().to_vec(),
                        data: tensor.into_data(),
                    },
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_and_invalid_pcm_abstain_before_model_loading() {
        let mut samples = vec![[0.0; 2]; 3 * 8000];
        assert!(analyze_neural_rhythm(&samples, 8000)
            .unwrap_err()
            .contains("silent"));
        samples[99][0] = f32::NAN;
        assert!(analyze_neural_rhythm(&samples, 8000)
            .unwrap_err()
            .contains("non-finite"));
        assert!(analyze_neural_rhythm(&samples, 0).is_err());
    }

    #[test]
    fn validation_rejects_invalid_order_bounds_and_downbeat_membership() {
        let beats = [0.1, 0.6, 1.1, 1.6, 2.1, 2.6];
        assert!(validate_events(&beats, &[0.1, 1.6], 3.0).is_ok()); // 3/4 is preserved
        assert!(validate_events(&beats, &[0.1, 1.1], 3.0).is_ok()); // no fixed meter
        for bad in [
            vec![0.1, 0.6, 0.6, 1.6],
            vec![0.1, f64::NAN, 1.1, 1.6],
            vec![0.1, 0.6, 1.1, 3.0],
        ] {
            assert!(validate_events(&bad, &[], 3.0).is_err());
        }
        assert!(validate_events(&beats, &[0.1, 1.2], 3.0).is_err());
        assert!(validate_events(&beats, &[1.6, 0.1], 3.0).is_err());
        assert!(validate_events(&beats[..3], &[], 3.0).is_err());
    }

    #[test]
    fn confidence_is_separate_and_uncalibrated() {
        assert!((peak_confidence(&[0.1], &[2.0; 12]) - 0.880797).abs() < 1.0e-5);
        assert!((peak_confidence(&[0.1], &[-2.0; 12]) - 0.119203).abs() < 1.0e-5);
        assert_eq!(peak_confidence(&[], &[2.0; 12]), 0.0);
    }

    #[test]
    fn result_preserves_nonuniform_timing_and_irregular_bars() {
        let beats = vec![0.12, 0.59, 1.08, 1.60, 2.15, 2.72, 3.30];
        let downbeats = vec![0.12, 1.60, 3.30];
        let analysis = BeatAnalysis {
            beats: beats.clone(),
            downbeats: downbeats.clone(),
            mel: Tensor {
                shape: vec![1, 200, 128],
                data: vec![],
            },
            beat_logits: vec![3.0; 200],
            downbeat_logits: vec![1.0; 200],
        };
        let result = finish_analysis(analysis, 4.0, 0.25).unwrap();
        assert_eq!(
            result.beats_seconds,
            beats.into_iter().map(f64::from).collect::<Vec<_>>()
        );
        assert_eq!(
            result.downbeats_seconds,
            downbeats.into_iter().map(f64::from).collect::<Vec<_>>()
        );
        assert!(result.beat_confidence > result.downbeat_confidence);
        assert_eq!(result.analysis_seconds, 0.25);
    }

    #[test]
    fn centered_stft_end_frame_is_removed_without_moving_source_events() {
        let analysis = BeatAnalysis {
            beats: vec![0.0, 0.5, 1.0, 1.5, 2.0],
            downbeats: vec![0.0, 2.0],
            mel: Tensor {
                shape: vec![1, 101, 128],
                data: vec![],
            },
            beat_logits: vec![2.0; 101],
            downbeat_logits: vec![1.0; 101],
        };
        let result = finish_analysis(analysis, 2.0, 0.1).unwrap();
        assert_eq!(result.beats_seconds, vec![0.0, 0.5, 1.0, 1.5]);
        assert_eq!(result.downbeats_seconds, vec![0.0]);
    }

    #[test]
    fn model_and_stream_are_send_for_background_analysis() {
        fn assert_send<T: Send>() {}
        assert_send::<Tracker>();
        assert_send::<beat_this::BeatStream<'static, BoundedModel>>();
        assert_send::<Mutex<Option<Tracker>>>();
    }

    #[test]
    fn multi_beat_slopes_remove_frame_quantization_bpm_bias() {
        for bpm in [126.0, 129.0, 133.0, 174.0] {
            let beats: Vec<f64> = (0..160)
                .map(|i| (i as f64 * 60.0 / bpm * FRAMES_PER_SECOND).round() / FRAMES_PER_SECOND)
                .collect();
            assert!((summarize_bpm(&beats) - bpm).abs() < 0.12);
        }
        assert_eq!(summarize_bpm(&[0.0, 0.5, 1.0, 1.5]), 120.0);
    }

    #[test]
    #[ignore = "requires installed full final0 model; synthetic smoke test, not an accuracy benchmark"]
    fn full_model_inference_smoke() {
        let rate = 22050;
        let samples: Vec<[f32; 2]> = (0..rate * 12)
            .map(|i| {
                let t = f64::from(i) / f64::from(rate);
                let age = t % 0.5;
                let value =
                    ((-age * 35.0).exp() * (t * 2.0 * std::f64::consts::PI * 80.0).sin()) as f32;
                [value, value]
            })
            .collect();
        let result = analyze_neural_rhythm(&samples, rate).unwrap();
        assert!(result.model.contains("final0"));
        validate_events(&result.beats_seconds, &result.downbeats_seconds, 12.0).unwrap();
        assert!((20.0..=600.0).contains(&result.bpm));
        assert!((0.0..=1.0).contains(&result.beat_confidence));
        assert!((0.0..=1.0).contains(&result.downbeat_confidence));
    }
}

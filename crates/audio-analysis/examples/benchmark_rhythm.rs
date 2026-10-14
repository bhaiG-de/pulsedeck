//! Opt-in paired model/timing benchmark. ffmpeg is needed only by this example.
//! cargo run -p audio-analysis --release --example benchmark_rhythm -- AUDIO
//!   [--golden PYTHON_GOLDEN.json] [--tempo BPM] [--sample-rate 44100]
use audio_analysis::{analyze_neural_rhythm_with_logit_observer, analyze_rhythm};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let audio = args.first().ok_or(
        "usage: benchmark_rhythm AUDIO [--golden JSON] [--tempo BPM] [--sample-rate RATE]",
    )?;
    let mut golden_path = None;
    let mut annotated_tempo = None;
    let mut activation_path = None;
    let mut rate = 44100_u32;
    let mut index = 1;
    while index < args.len() {
        let value = args.get(index + 1).ok_or("missing option value")?;
        match args[index].as_str() {
            "--golden" => golden_path = Some(value.as_str()),
            "--tempo" => annotated_tempo = Some(value.parse::<f64>()?),
            "--activations" => activation_path = Some(value.as_str()),
            "--sample-rate" => rate = value.parse()?,
            _ => return Err(format!("unknown option {}", args[index]).into()),
        }
        index += 2;
    }
    if !(8000..=192000).contains(&rate) {
        return Err("sample rate outside 8000–192000 Hz".into());
    }
    if annotated_tempo.is_some_and(|v| !v.is_finite() || v <= 0.0) {
        return Err("annotated tempo must be finite and positive".into());
    }
    let decoded = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            audio,
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ac",
            "2",
            "-ar",
            &rate.to_string(),
            "pipe:1",
        ])
        .output()?;
    if !decoded.status.success() {
        return Err(format!(
            "ffmpeg decode failed: {}",
            String::from_utf8_lossy(&decoded.stderr)
        )
        .into());
    }
    if decoded.stdout.len() % 8 != 0 {
        return Err("incomplete stereo f32 sample".into());
    }
    let samples: Vec<[f32; 2]> = decoded
        .stdout
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| {
            [
                f32::from_le_bytes(b[..4].try_into().unwrap()),
                f32::from_le_bytes(b[4..].try_into().unwrap()),
            ]
        })
        .collect();
    drop(decoded);
    let duration = samples.len() as f64 / f64::from(rate);
    let start = Instant::now();
    let (legacy, phase) = analyze_rhythm(&samples, rate);
    let legacy_seconds = start.elapsed().as_secs_f64();
    let neural = analyze_neural_rhythm_with_logit_observer(&samples, rate, |beats, downbeats| {
        if let Some(path) = activation_path {
            let data = json!({"fps": 50, "beat_logits": beats, "downbeat_logits": downbeats,
                "provenance": "Exact Beat This! logits observed before PulseDeck decoding; no PCM"});
            fs::write(path, serde_json::to_vec(&data).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    let golden = golden_path.map(|path| -> Result<Value, Box<dyn std::error::Error>> {
        let source: Value = serde_json::from_slice(&fs::read(path)?)?;
        let references = |name: &str| -> Result<Vec<f64>, Box<dyn std::error::Error>> {
            let values: Vec<f64> = serde_json::from_value(source.get(name).ok_or("missing golden event vector")?.clone())?;
            if values.iter().any(|t| !t.is_finite() || *t < 0.0)
                || values.windows(2).any(|w| w[1] <= w[0]) {
                return Err("golden events must be finite, sorted, unique and non-negative".into());
            }
            Ok(values)
        };
        let beat_ref = references("beats")?;
        let downbeat_ref = references("downbeats")?;
        Ok(json!({
            "path": path, "sha256": digest(Path::new(path))?,
            "provenance": source.get("provenance"),
            "comparison_kind": source.get("reference_kind").cloned().unwrap_or_else(|| json!("same-model Python-reference parity; not human beat ground truth")),
            "beats": ([70.0, 20.0, 10.0].map(|ms| score(&beat_ref, &neural.beats_seconds, ms))),
            "downbeats": ([70.0, 20.0, 10.0].map(|ms| score(&downbeat_ref, &neural.downbeats_seconds, ms))),
            "beats_f32_rounding_aware": ([70.0, 20.0, 10.0].map(|ms| score_with_allowance(&beat_ref, &neural.beats_seconds, ms, 0.01))),
            "downbeats_f32_rounding_aware": ([70.0, 20.0, 10.0].map(|ms| score_with_allowance(&downbeat_ref, &neural.downbeats_seconds, ms, 0.01))),
        }))
    }).transpose()?;
    let tempo_error = |bpm: Option<f64>| {
        bpm.zip(annotated_tempo).map(|(predicted, reference)| json!({
        "absolute_bpm": (predicted-reference).abs(),
        "relative_percent": 100.0 * (predicted-reference).abs()/reference,
        "within_4_percent_exact_tempo": (predicted-reference).abs()/reference <= 0.04,
        "within_4_percent_allowing_half_double": ([0.5, 1.0, 2.0].iter().any(|factor| (predicted * factor-reference).abs()/reference <= 0.04)),
    }))
    };
    let report = json!({
        "audio": {"filename": Path::new(audio).file_name().unwrap_or_default().to_string_lossy(),
                  "sha256": digest(Path::new(audio))?, "duration_seconds": duration, "decoded_sample_rate": rate,
                  "decoder": "ffmpeg stereo f32le to memory; no source PCM export"},
        "model_manifest": serde_json::from_slice::<Value>(&fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/beat-models/manifest.json"))?)?,
        "neural": neural,
        "neural_real_time_factor": neural.analysis_seconds / duration,
        "legacy": {"tempo": legacy, "phase": phase, "analysis_seconds": legacy_seconds},
        "annotated_tempo_bpm": annotated_tempo,
        "tempo_only_reference_note": "A BPM label does not establish beat placement, downbeat accuracy or bar/phrase alignment.",
        "neural_tempo_error": tempo_error(Some(neural.bpm)),
        "legacy_tempo_error": tempo_error(legacy.bpm),
        "python_reference": golden,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn digest(path: &Path) -> Result<String, std::io::Error> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

/// Ordered one-to-one event matching. A reference and prediction can each match
/// at most once. Primary scores use strict tolerance; a separately labeled
/// diagnostic allows 10 µs for source-f32 to golden-f64 conversion rounding.
fn score(reference: &[f64], predicted: &[f64], tolerance_ms: f64) -> Value {
    score_with_allowance(reference, predicted, tolerance_ms, 0.0)
}

fn score_with_allowance(
    reference: &[f64],
    predicted: &[f64],
    tolerance_ms: f64,
    allowance_ms: f64,
) -> Value {
    let tolerance = (tolerance_ms + allowance_ms) / 1000.0;
    let (mut i, mut j) = (0, 0);
    let mut errors = Vec::new();
    while i < reference.len() && j < predicted.len() {
        let delta = predicted[j] - reference[i];
        if delta.abs() <= tolerance {
            errors.push(delta.abs() * 1000.0);
            i += 1;
            j += 1;
        } else if delta < 0.0 {
            j += 1;
        } else {
            i += 1;
        }
    }
    errors.sort_by(f64::total_cmp);
    let count = errors.len();
    let precision = if predicted.is_empty() {
        0.0
    } else {
        count as f64 / predicted.len() as f64
    };
    let recall = if reference.is_empty() {
        0.0
    } else {
        count as f64 / reference.len() as f64
    };
    json!({
        "tolerance_ms": tolerance_ms,
        "matched": count, "predicted_count": predicted.len(), "reference_count": reference.len(),
        "exact_count_equal": predicted.len() == reference.len(),
        "precision": precision, "recall": recall,
        "f_measure": if precision + recall == 0.0 { 0.0 } else { 2.0 * precision * recall / (precision + recall) },
        "matched_error_mean_ms": if count == 0 { None } else { Some(errors.iter().sum::<f64>() / count as f64) },
        "matched_error_p95_ms": if count == 0 { None } else { Some(errors[((count as f64 * 0.95).ceil() as usize).saturating_sub(1)]) },
        "matched_error_max_ms": errors.last(),
        "matching": "ordered one-to-one",
        "float_conversion_allowance_ms": allowance_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_never_reuses_an_event_and_handles_float_bin_boundaries() {
        let result = score(&[0.1, 0.5, 1.0], &[0.09, 0.11, 0.51, 1.1], 20.0);
        assert_eq!(result["matched"], 2);
        assert_eq!(result["predicted_count"], 4);
        assert_eq!(result["reference_count"], 3);
        assert_eq!(score(&[], &[], 10.0)["matched"], 0);
        assert_eq!(score(&[0.1], &[f64::from(0.12_f32)], 20.0)["matched"], 1);
        assert_eq!(
            score(&[150.0], &[f64::from(150.02_f32)], 20.0)["matched"],
            0
        );
        assert_eq!(
            score_with_allowance(&[150.0], &[f64::from(150.02_f32)], 20.0, 0.01)["matched"],
            1
        );
    }
}

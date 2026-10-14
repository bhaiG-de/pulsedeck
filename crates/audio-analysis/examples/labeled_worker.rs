//! Persistent benchmark worker; no provider PCM or annotations are read here.
//! NDJSON input: {"id":"...","mel_path":"..."} or {"id":"...","logits_path":"..."}.
use audio_analysis::{
    decode_independent_beats_for_validation, decode_neural_logits_for_app_validation,
    decode_neural_logits_with_meters_for_validation, decode_separated_for_validation,
    decode_with_acoustic_meter_for_validation, neural_logits_from_spectrogram_for_validation,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, BufRead},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let started = Instant::now();
        let (beat, downbeat) = if let Some(p) = request["mel_path"].as_str() {
            let bytes = fs::read(p)?;
            if !bytes.len().is_multiple_of(4) {
                return Err("invalid f32 file".into());
            }
            let frames = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            neural_logits_from_spectrogram_for_validation(frames)?
        } else {
            let data: Value = serde_json::from_slice(&fs::read(
                request["logits_path"]
                    .as_str()
                    .ok_or("missing logits_path")?,
            )?)?;
            (
                serde_json::from_value(data["beat_logits"].clone())?,
                serde_json::from_value(data["downbeat_logits"].clone())?,
            )
        };
        let inference_seconds = started.elapsed().as_secs_f64();
        let meters: Vec<usize> = if request["meters"].is_array() {
            serde_json::from_value(request["meters"].clone())?
        } else {
            vec![3, 4]
        };
        let (beats, downbeats) = if request["policy"] == "separated" {
            decode_separated_for_validation(&beat, &downbeat)?
        } else if request["policy"] == "beat-only" {
            (decode_independent_beats_for_validation(&beat)?, vec![])
        } else if request["policy"] == "production" && request["evidence_mel_path"].is_string() {
            let bytes = fs::read(request["evidence_mel_path"].as_str().unwrap())?;
            if !bytes.len().is_multiple_of(4) {
                return Err("invalid evidence f32 file".into());
            }
            let mel: Vec<f32> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            let (beats, downbeats, _) =
                decode_with_acoustic_meter_for_validation(&beat, &downbeat, &mel)?;
            (
                beats.into_iter().map(|t| f64::from(t as f32)).collect(),
                downbeats.into_iter().map(|t| f64::from(t as f32)).collect(),
            )
        } else if request["policy"] == "production" {
            decode_neural_logits_for_app_validation(&beat, &downbeat)?
        } else {
            decode_neural_logits_with_meters_for_validation(&beat, &downbeat, 300.0, &meters)?
        };
        println!(
            "{}",
            json!({"id":request["id"],"decoder_analysis_version":audio_analysis::ANALYSIS_VERSION,"beat_logits":beat,"downbeat_logits":downbeat,"beats":beats,"downbeats":downbeats,"inference_seconds":inference_seconds,"total_seconds":started.elapsed().as_secs_f64(),"meters":meters})
        );
    }
    Ok(())
}

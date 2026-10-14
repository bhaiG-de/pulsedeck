//! cargo run -p audio-analysis --release --example compare_dbn -- LOGITS_JSON MAX_BPM DURATION
use audio_analysis::decode_neural_logits_for_validation;
use serde_json::{json, Value};
use std::{fs, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: compare_dbn LOGITS_JSON MAX_BPM DURATION_SECONDS".into());
    }
    let source: Value = serde_json::from_slice(&fs::read(&args[0])?)?;
    let beat: Vec<f32> = serde_json::from_value(source["beat_logits"].clone())?;
    let downbeat: Vec<f32> = serde_json::from_value(source["downbeat_logits"].clone())?;
    let max_bpm: f32 = args[1].parse()?;
    let duration: f64 = args[2].parse()?;
    if !duration.is_finite() || duration <= 0.0 {
        return Err("invalid duration".into());
    }
    let started = Instant::now();
    let (mut beats, mut downbeats) =
        decode_neural_logits_for_validation(&beat, &downbeat, max_bpm)?;
    beats.retain(|time| *time < duration);
    downbeats.retain(|time| *time < duration);
    let span = 32.min(beats.len().saturating_sub(1));
    let mut intervals: Vec<f64> = if span == 0 {
        vec![]
    } else {
        beats
            .windows(span + 1)
            .map(|w| (w[span] - w[0]) / span as f64)
            .collect()
    };
    intervals.sort_by(f64::total_cmp);
    let midpoint = intervals.len() / 2;
    let median = if intervals.is_empty() {
        None
    } else if intervals.len().is_multiple_of(2) {
        Some((intervals[midpoint - 1] + intervals[midpoint]) * 0.5)
    } else {
        Some(intervals[midpoint])
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"beats":beats,"downbeats":downbeats,"bpm":median.map(|value|60.0/value),"max_bpm":max_bpm,"duration_seconds":duration,"decode_seconds":started.elapsed().as_secs_f64(),"activation_file":args[0]})
        )?
    );
    Ok(())
}

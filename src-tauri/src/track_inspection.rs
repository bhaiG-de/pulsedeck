//! Bounded views of existing analysis bins; no new audio analysis or DJ decisions.
use control_protocol::ControlError;
use serde_json::{json, Value};

pub fn view(
    mut metadata: Value,
    start: f64,
    end: Option<f64>,
    points: usize,
) -> Result<Value, ControlError> {
    let duration = metadata["duration_seconds"].as_f64().unwrap_or(0.0);
    let end = end.unwrap_or(duration);
    if !duration.is_finite() || duration <= 0.0 {
        return Err(ControlError::new(
            "analysis_unavailable",
            "Track has no usable duration",
        ));
    }
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.0
        || end > duration
        || end <= start
        || !(1..=512).contains(&points)
    {
        return Err(ControlError::new(
            "invalid_arguments",
            "Use 0 <= start_seconds < end_seconds <= duration_seconds and points 1..512",
        )
        .with_detail("duration_seconds", duration));
    }
    if metadata.get("beatgrid_uniform").is_none() && metadata["bpm_source"] == "manual" {
        metadata["beatgrid_uniform"] = json!(true);
    }
    if let Some(stage) = metadata.as_object_mut().unwrap().remove("beat_model") {
        metadata["beat_model"] = json!({"version":stage["version"],"beat_count":stage["beats_seconds"].as_array().map(Vec::len),"downbeat_count":stage["downbeats_seconds"].as_array().map(Vec::len),"grid":stage["grid"],"compatibility_stage":true});
    }
    let rhythm = metadata
        .as_object_mut()
        .unwrap()
        .remove("neural_rhythm")
        .unwrap_or(Value::Null);
    let meter = rhythm["beats_per_bar"]
        .as_u64()
        .filter(|m| (2..=12).contains(m))
        .unwrap_or(4) as i64;
    let mut beat_events = Vec::new();
    let mut total_events = 0usize;
    let mut tempo_times = Vec::new();
    if let Some(times) = rhythm["beats_seconds"].as_array() {
        let downbeats: Vec<f64> = rhythm["downbeats_seconds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect();
        let anchor = metadata["beatgrid_anchor_index"]
            .as_u64()
            .or_else(|| metadata["manual_grid_anchor_index"].as_u64())
            .map(|i| i as usize)
            .unwrap_or_else(|| {
                times
                    .iter()
                    .position(|t| {
                        t.as_f64().is_some_and(|t| {
                            downbeats.first().is_some_and(|v| (t - v).abs() < 1e-5)
                        })
                    })
                    .unwrap_or(0)
            })
            .min(times.len().saturating_sub(1));
        let origin = times.get(anchor).and_then(Value::as_f64).unwrap_or(0.0);
        let offset = metadata["beatgrid_offset_seconds"]
            .as_f64()
            .unwrap_or(origin);
        let model_bpm = rhythm["bpm"].as_f64().unwrap_or(120.0);
        let manual = metadata["beatgrid_manual"]
            .as_bool()
            .unwrap_or(metadata["beatgrid_source"] == "manual");
        let scaled =
            metadata.get("beatgrid_anchor_index").is_some() || metadata["bpm_source"] == "manual";
        let scale = if scaled {
            model_bpm
                / metadata["bpm"]
                    .as_f64()
                    .filter(|b| *b > 0.0)
                    .unwrap_or(model_bpm)
        } else {
            1.0
        };
        for (index, t) in times.iter().filter_map(Value::as_f64).enumerate() {
            if metadata["beatgrid_uniform"] == true {
                break;
            }
            let seconds = offset + (t - origin) * scale;
            if seconds < start || seconds > end {
                continue;
            }
            total_events += 1;
            tempo_times.push(seconds);
            if beat_events.len() < points {
                let downbeat = if manual {
                    (index as i64 - anchor as i64).rem_euclid(meter) == 0
                } else {
                    downbeats.binary_search_by(|d| d.total_cmp(&t)).is_ok()
                };
                beat_events.push(json!({"seconds":seconds,"beat_index":index,"beat_position":index as i64-anchor as i64,"downbeat":downbeat}));
            }
        }
        metadata["neural_rhythm"] = json!({"model":rhythm["model"],"bpm":model_bpm,"beat_confidence":rhythm["beat_confidence"],"downbeat_confidence":rhythm["downbeat_confidence"],"analysis_seconds":rhythm["analysis_seconds"],"beat_count":times.len(),"downbeat_count":downbeats.len(),"scores_calibrated":false,"beats_per_bar":rhythm["beats_per_bar"],"bar_evidence":rhythm["bar_evidence"]});
    }
    if metadata["beatgrid_uniform"] == true {
        let bpm = metadata["bpm"].as_f64().unwrap_or(0.0);
        let offset = metadata["beatgrid_offset_seconds"].as_f64().unwrap_or(0.0);
        if bpm > 0.0 {
            let first = ((start - offset) * bpm / 60.0).ceil() as i64;
            let last = ((end - offset) * bpm / 60.0).floor() as i64;
            total_events = (last - first + 1).max(0) as usize;
            let manual = metadata["beatgrid_manual"] == true;
            for index in first..=last.min(first + points as i64 - 1) {
                beat_events.push(json!({"seconds":offset+index as f64*60.0/bpm,"beat_index":index,"beat_position":index,"downbeat":manual && index.rem_euclid(meter)==0}));
            }
        }
    }
    let inferred_region = metadata["neural_rhythm"]["bar_evidence"]
        .as_array()
        .is_some_and(|items| {
            items.iter().any(|e| {
                e["half_phase_corrected"] == true
                    || e["pulse_repairs"].as_array().is_some_and(|repairs| {
                        repairs.iter().any(|r| {
                            r["start_seconds"].as_f64().unwrap_or(f64::INFINITY) < end
                                && r["end_seconds"].as_f64().unwrap_or(0.0) > start
                        })
                    })
            })
        });
    let local_tempo = if metadata["beatgrid_uniform"] == true {
        json!({"source_bpm":metadata["bpm"],"p05_bpm":metadata["bpm"],"p95_bpm":metadata["bpm"],"uniform":true})
    } else if tempo_times.len() >= 2 {
        let span = [32, 16, 8, 4, 1]
            .into_iter()
            .find(|&n| tempo_times.len() > n)
            .unwrap();
        let mut rates: Vec<f64> = tempo_times
            .windows(span + 1)
            .map(|w| 60.0 * span as f64 / (w[span] - w[0]))
            .collect();
        rates.sort_by(f64::total_cmp);
        let source_bpm = rates[rates.len() / 2];
        let p05 = rates[(rates.len() - 1) * 5 / 100];
        let p95 = rates[(rates.len() - 1) * 95 / 100];
        let ratio = metadata["bpm"]
            .as_f64()
            .filter(|b| *b > 0.0)
            .map(|b| source_bpm / b);
        json!({"source_bpm":source_bpm,"p05_bpm":p05,"p95_bpm":p95,"span_beats":span,"relative_to_track_summary":ratio,"inferred_events_in_range":inferred_region,"pulse_review_required":inferred_region || ratio.is_some_and(|r|!(0.7..=1.4).contains(&r)) || p95/p05>1.25,"uniform":false})
    } else {
        Value::Null
    };
    let beat_events = json!({"events":beat_events,"local_tempo":local_tempo,"total_in_range":total_events,"truncated":total_events>points,"timing_resolution_seconds":0.02,"source":metadata["beatgrid_source"],"manual":metadata["beatgrid_manual"],"uniform":metadata["beatgrid_uniform"]});
    let peaks = metadata
        .as_object_mut()
        .unwrap()
        .remove("waveform")
        .unwrap_or(Value::Null);
    let bands = metadata
        .as_object_mut()
        .unwrap()
        .remove("waveform_bands")
        .unwrap_or(Value::Null);
    let peaks = peaks.as_array().ok_or_else(|| {
        ControlError::new("analysis_unavailable", "Track has no stored peak waveform")
    })?;
    if peaks.is_empty() {
        return Err(ControlError::new(
            "analysis_unavailable",
            "Track has no stored peak waveform",
        ));
    }
    let bin_seconds = duration / peaks.len() as f64;
    let first = (start / bin_seconds).floor() as usize;
    let last = ((end / bin_seconds).ceil() as usize).min(peaks.len());
    let count = points.min(last - first);
    let bands_available = ["low", "mid", "high"].iter().all(|key| {
        bands[key]
            .as_array()
            .is_some_and(|a| a.len() == peaks.len())
    });
    let mut bins = Vec::with_capacity(count);
    for group in 0..count {
        let lo = first + group * (last - first) / count;
        let hi = first + (group + 1) * (last - first) / count;
        let peak = peaks[lo..hi]
            .iter()
            .filter_map(Value::as_f64)
            .fold(0.0, f64::max);
        let rms = |key: &str| -> Option<f64> {
            if !bands_available {
                return None;
            }
            let values = bands[key].as_array().unwrap();
            Some(
                (values[lo..hi]
                    .iter()
                    .filter_map(Value::as_f64)
                    .map(|v| v * v)
                    .sum::<f64>()
                    / (hi - lo) as f64)
                    .sqrt(),
            )
        };
        bins.push(json!({"start_seconds":lo as f64 * bin_seconds,"end_seconds":hi as f64 * bin_seconds,"peak":peak,"low":rms("low"),"mid":rms("mid"),"high":rms("high")}));
    }
    Ok(
        json!({"metadata":metadata,"beat_events":beat_events,"requested_range":{"start_seconds":start,"end_seconds":end},"raw_bin_seconds":bin_seconds,"raw_bins":peaks.len(),"bands_available":bands_available,"aggregation":{"peak":"maximum","bands":"root mean square of stored bin RMS values","resolution":"Boundaries follow stored bins; requesting more points cannot exceed original resolution"},"bins":bins}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_origin_retains_detected_five_beat_bar_length() {
        let beats: Vec<f64> = (0..11).map(|i| i as f64 * 0.5).collect();
        let m = json!({"duration_seconds":6.0,"waveform":vec![0.5;12],"bpm":120.0,"beatgrid_offset_seconds":0.0,"beatgrid_anchor_index":0,"beatgrid_manual":true,"neural_rhythm":{"bpm":120.0,"beats_per_bar":5,"beats_seconds":beats,"downbeats_seconds":[0.0,2.5,5.0]}});
        let v = view(m, 0.0, None, 64).unwrap();
        let indices: Vec<_> = v["beat_events"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["downbeat"] == true)
            .map(|e| e["beat_index"].as_u64().unwrap())
            .collect();
        assert_eq!(indices, vec![0, 5, 10]);
    }
    #[test]
    fn neural_events_are_bounded_and_manual_anchor_transforms_actual_timeline() {
        let m = json!({"duration_seconds":4.0,"waveform":vec![0.5;8],"bpm":120.0,"beatgrid_offset_seconds":1.1,"beatgrid_anchor_index":2,"beatgrid_manual":true,"neural_rhythm":{"bpm":120.0,"beats_seconds":[0.0,0.5,1.0,1.6,2.3,3.0],"downbeats_seconds":[0.0,2.3]}});
        let v = view(m, 1.0, Some(3.5), 2).unwrap();
        assert_eq!(v["beat_events"]["total_in_range"], 4);
        assert_eq!(v["beat_events"]["truncated"], true);
        assert_eq!(v["beat_events"]["events"][0]["seconds"], 1.1);
        assert_eq!(v["beat_events"]["events"][0]["downbeat"], true);
        assert!((v["beat_events"]["events"][1]["seconds"].as_f64().unwrap() - 1.7).abs() < 1e-8);
        assert!(v["metadata"]["neural_rhythm"]
            .get("beats_seconds")
            .is_none());
    }
    #[test]
    fn overview_and_ranges_preserve_bin_times_peaks_and_shared_band_scale() {
        let metadata = json!({"duration_seconds":8.0,"title":"Fixture","waveform":[0.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7],"waveform_bands":{"low":[0.0,0.0,0.5,0.5,0.5,0.5,0.0,0.0],"mid":vec![0.25;8],"high":vec![0.1;8]}});
        let v = view(metadata.clone(), 2.2, Some(5.7), 2).unwrap();
        assert_eq!(v["bins"][0]["start_seconds"], 2.0);
        assert_eq!(v["bins"][1]["end_seconds"], 6.0);
        assert_eq!(v["bins"][1]["peak"], 0.5);
        assert_eq!(v["bins"][0]["low"], 0.5);
        assert!(v["metadata"].get("waveform").is_none());
        assert_eq!(
            view(metadata.clone(), 0.0, None, 512).unwrap()["bins"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert!(view(metadata.clone(), 6.0, Some(5.0), 8).is_err());
        assert!(view(metadata, 0.0, Some(9.0), 8).is_err());
    }
    #[test]
    fn missing_bands_are_explicit_and_output_is_bounded() {
        let v = view(
            json!({"duration_seconds":180.0,"waveform":vec![0.5;4096]}),
            0.0,
            None,
            64,
        )
        .unwrap();
        assert_eq!(v["bands_available"], false);
        assert_eq!(v["bins"][0]["low"], Value::Null);
        assert_eq!(v["bins"].as_array().unwrap().len(), 64);
        assert!(v.to_string().len() < 20000);
    }
}

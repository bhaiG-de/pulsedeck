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
        json!({"metadata":metadata,"requested_range":{"start_seconds":start,"end_seconds":end},"raw_bin_seconds":bin_seconds,"raw_bins":peaks.len(),"bands_available":bands_available,"aggregation":{"peak":"maximum","bands":"root mean square of stored bin RMS values","resolution":"Boundaries follow stored bins; requesting more points cannot exceed original resolution"},"bins":bins}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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

//! Piecewise source-time beat clock. No allocations or inference in render.
/// Interpolate the observed beat sequence, extrapolating only for clock math.
/// UI renders observed events only; extrapolation does not fabricate markers.
pub(crate) fn position(times: &[f64], seconds: f64) -> f64 {
    let next = times.partition_point(|t| *t <= seconds);
    let left = next.saturating_sub(1).min(times.len() - 2);
    left as f64 + (seconds - times[left]) / (times[left + 1] - times[left])
}
pub(crate) fn time(times: &[f64], beat: f64) -> f64 {
    let left = (beat.floor().max(0.0) as usize).min(times.len() - 2);
    times[left] + (beat - left as f64) * (times[left + 1] - times[left])
}
pub(crate) fn nearest_index(times: &[f64], seconds: f64) -> usize {
    let next = times.partition_point(|t| *t < seconds).min(times.len() - 1);
    if next > 0 && (times[next - 1] - seconds).abs() < (times[next] - seconds).abs() {
        next - 1
    } else {
        next
    }
}

impl crate::DeckSnapshot {
    pub fn grid_position(&self, seconds: f64) -> Option<f64> {
        if !self.beatgrid_valid {
            return None;
        }
        if let Some(g) = self
            .track
            .as_ref()
            .and_then(|t| t.neural_rhythm.as_ref())
            .filter(|g| !self.beatgrid_uniform && g.beats_seconds.len() >= 2)
        {
            let anchor = self
                .beatgrid_anchor_index
                .unwrap_or(0)
                .min(g.beats_seconds.len() - 1);
            let scale = g.bpm / self.bpm.unwrap_or(g.bpm);
            Some(
                position(
                    &g.beats_seconds,
                    g.beats_seconds[anchor] + (seconds - self.beatgrid_offset_seconds) / scale,
                ) - anchor as f64,
            )
        } else {
            self.bpm
                .map(|bpm| (seconds - self.beatgrid_offset_seconds) * bpm / 60.0)
        }
    }
    pub fn grid_time(&self, beat: f64) -> Option<f64> {
        if !self.beatgrid_valid {
            return None;
        }
        if let Some(g) = self
            .track
            .as_ref()
            .and_then(|t| t.neural_rhythm.as_ref())
            .filter(|g| !self.beatgrid_uniform && g.beats_seconds.len() >= 2)
        {
            let anchor = self
                .beatgrid_anchor_index
                .unwrap_or(0)
                .min(g.beats_seconds.len() - 1);
            let scale = g.bpm / self.bpm.unwrap_or(g.bpm);
            Some(
                self.beatgrid_offset_seconds
                    + (time(&g.beats_seconds, beat + anchor as f64) - g.beats_seconds[anchor])
                        * scale,
            )
        } else {
            self.bpm
                .map(|bpm| self.beatgrid_offset_seconds + beat * 60.0 / bpm)
        }
    }
    pub fn snapped_grid_time(&self, seconds: f64) -> Option<f64> {
        let mut beat = self.grid_position(seconds)?.round();
        if let Some(g) = self
            .track
            .as_ref()
            .and_then(|t| t.neural_rhythm.as_ref())
            .filter(|g| !self.beatgrid_uniform && g.beats_seconds.len() >= 2)
        {
            let anchor = self
                .beatgrid_anchor_index
                .unwrap_or(0)
                .min(g.beats_seconds.len() - 1);
            let scale = g.bpm / self.bpm.unwrap_or(g.bpm);
            let origin = g.beats_seconds[anchor];
            let duration = self.track.as_ref()?.duration_seconds;
            let first = g
                .beats_seconds
                .partition_point(|t| *t < origin - self.beatgrid_offset_seconds / scale);
            let end = g.beats_seconds.partition_point(|t| {
                *t < origin + (duration - self.beatgrid_offset_seconds) / scale
            });
            if first >= end {
                return None;
            }
            beat = (beat + anchor as f64).clamp(first as f64, (end - 1) as f64) - anchor as f64;
        }
        let at = self.grid_time(beat)?;
        if at < 0.0 {
            self.grid_time(self.grid_position(0.0)?.ceil())
        } else {
            Some(at)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_tempo_grid_is_invertible_and_does_not_drift_to_mean_bpm() {
        let times = [0.173, 0.67, 1.2, 1.71, 2.3, 2.95];
        for i in -4..40 {
            let beat = i as f64 / 4.0;
            assert!((position(&times, time(&times, beat)) - beat).abs() < 1e-10);
        }
        assert_eq!(time(&times, 5.0), 2.95);
        assert!((time(&times, 5.0) - (times[0] + 5.0 * 0.5)).abs() > 0.2);
    }
}

use crate::{
    decode::TrackData,
    model::{ControlTarget, EngineCommand, LoopBoundary},
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(crate) struct Smooth {
    pub target: f32,
    value: f32,
}
impl Smooth {
    fn new(value: f32) -> Self {
        Self {
            target: value,
            value,
        }
    }
    #[inline]
    fn tick(&mut self, coefficient: f32) -> f32 {
        if (self.target - self.value).abs() < 1e-6 {
            self.value = self.target;
        } else {
            self.value += (self.target - self.value) * coefficient;
        }
        self.value
    }
}

pub(crate) struct Deck {
    pub track: Option<Arc<TrackData>>,
    pub grid: Option<Arc<audio_analysis::NeuralRhythm>>,
    pub prepared: Option<Arc<TrackData>>,
    pub preparation_generation: u64,
    pub prepared_generation: u64,
    pub prepared_rate: f64,
    pub pitch: f64,
    pub keylock: bool,
    pub loop_in: Option<f64>,
    pub loop_edit: Option<LoopBoundary>,
    pub fx_beats: f64,
    pub jog_touch: bool,
    pub memory_cues: [Option<f64>; 10],
    pub peak: [f32; 2],
    pub playing: bool,
    pub position: f64,
    pub position_revision: u64,
    pub tempo: f64,
    pub cue: f64,
    pub hot_cues: [Option<f64>; 8],
    pub loop_region: Option<(f64, f64)>,
    last_loop_region: Option<(f64, f64)>,
    pub gain: Smooth,
    pub channel_fader: Smooth,
    pub sync_enabled: bool,
    cue_preview: bool,
    pub eq: [Smooth; 3],
    pub filter: Smooth,
    pub cue_enabled: bool,
    pub echo: Smooth,
    pub reverb: Smooth,
    pub bpm: Option<f64>,
    pub grid_offset: f64,
    pub grid_valid: bool,
    pub grid_downbeat: bool,
    pub grid_manual: bool,
    pub grid_uniform: bool,
    pub grid_anchor_index: usize,
    pub grid_scale: f64,
    band_low: [f32; 2],
    band_high: [f32; 2],
    filter_low: [[f32; 2]; 2],
    filter_high: [[f32; 2]; 2],
    echo_buffer: Vec<[f32; 2]>,
    echo_cursor: usize,
    reverb_buffer: Vec<[f32; 2]>,
    reverb_cursor: usize,
    bend: f64,
    scratch_speed: f64,
    scratch_remaining: u32,
    cursor_target: Option<f64>,
    cursor_release_pending: bool,
    attack: f32,
    output_rate: u32,
    smoothing: f32,
    low_coefficient: f32,
    high_coefficient: f32,
    filter_coefficient: f32,
    filter_counter: u32,
    sync_correction: f64,
    echo_epoch: Vec<u32>,
    reverb_epoch: Vec<u32>,
    epoch: u32,
}
impl Deck {
    fn new() -> Self {
        Self {
            track: None,
            grid: None,
            prepared: None,
            preparation_generation: 0,
            prepared_generation: 0,
            prepared_rate: 1.0,
            pitch: 0.0,
            keylock: false,
            loop_in: None,
            loop_edit: None,
            fx_beats: 0.5,
            jog_touch: false,
            memory_cues: [None; 10],
            peak: [0.0; 2],
            playing: false,
            position: 0.0,
            position_revision: 0,
            tempo: 0.0,
            cue: 0.0,
            hot_cues: [None; 8],
            loop_region: None,
            last_loop_region: None,
            gain: Smooth::new(1.0),
            channel_fader: Smooth::new(1.0),
            sync_enabled: false,
            cue_preview: false,
            eq: [Smooth::new(1.0); 3],
            filter: Smooth::new(0.0),
            cue_enabled: false,
            echo: Smooth::new(0.0),
            reverb: Smooth::new(0.0),
            bpm: None,
            grid_offset: 0.0,
            grid_valid: false,
            grid_downbeat: false,
            grid_manual: false,
            grid_uniform: false,
            grid_anchor_index: 0,
            grid_scale: 1.0,
            band_low: [0.0; 2],
            band_high: [0.0; 2],
            filter_low: [[0.0; 2]; 2],
            filter_high: [[0.0; 2]; 2],
            echo_buffer: vec![[0.0; 2]; 192_000],
            echo_cursor: 0,
            reverb_buffer: vec![[0.0; 2]; 48_000],
            reverb_cursor: 0,
            bend: 0.0,
            scratch_speed: 0.0,
            scratch_remaining: 0,
            cursor_target: None,
            cursor_release_pending: false,
            attack: 0.0,
            output_rate: 0,
            smoothing: 0.0,
            low_coefficient: 0.0,
            high_coefficient: 0.0,
            filter_coefficient: 0.0,
            filter_counter: 0,
            sync_correction: 0.0,
            echo_epoch: vec![0; 192000],
            reverb_epoch: vec![0; 48000],
            epoch: 1,
        }
    }
    pub fn sample_rate(&self) -> f64 {
        self.track
            .as_ref()
            .map_or(48_000.0, |t| t.sample_rate as f64)
    }
    pub fn duration_frames(&self) -> f64 {
        self.track.as_ref().map_or(0.0, |t| t.samples.len() as f64)
    }
    fn seek(&mut self, seconds: f64) {
        self.position = (seconds * self.sample_rate()).clamp(0.0, self.duration_frames());
        self.position_revision = self.position_revision.wrapping_add(1);
        self.scratch_remaining = 0;
        self.cursor_target = None;
        self.cursor_release_pending = false;
        self.attack = 0.0;
    }
    pub fn seconds(&self) -> f64 {
        self.position / self.sample_rate()
    }
    pub fn rate(&self) -> f64 {
        1.0 + self.tempo / 100.0
    }
    fn neural_grid(&self) -> Option<&audio_analysis::NeuralRhythm> {
        if self.grid_uniform {
            return None;
        }
        self.grid
            .as_deref()
            .or_else(|| self.track.as_ref()?.analysis.neural_rhythm.as_ref())
            .filter(|g| g.beats_seconds.len() >= 2)
    }
    pub fn grid_position(&self, seconds: f64) -> f64 {
        if let Some(g) = self.neural_grid() {
            let anchor = self.grid_anchor_index.min(g.beats_seconds.len() - 1);
            let original = g.beats_seconds[anchor] + (seconds - self.grid_offset) / self.grid_scale;
            crate::grid::position(&g.beats_seconds, original) - anchor as f64
        } else {
            (seconds - self.grid_offset) * self.bpm.unwrap_or(120.0) / 60.0
        }
    }
    pub fn grid_time(&self, beat: f64) -> f64 {
        if let Some(g) = self.neural_grid() {
            let anchor = self.grid_anchor_index.min(g.beats_seconds.len() - 1);
            self.grid_offset
                + (crate::grid::time(&g.beats_seconds, beat + anchor as f64)
                    - g.beats_seconds[anchor])
                    * self.grid_scale
        } else {
            self.grid_offset + beat * 60.0 / self.bpm.unwrap_or(120.0)
        }
    }
    pub fn grid_period(&self) -> f64 {
        if let Some(g) = self.neural_grid() {
            let origin = g.beats_seconds[self.grid_anchor_index];
            let original = origin + (self.seconds() - self.grid_offset) / self.grid_scale;
            let next = g.beats_seconds.partition_point(|t| *t <= original);
            let left = next.saturating_sub(1).min(g.beats_seconds.len() - 2);
            (g.beats_seconds[left + 1] - g.beats_seconds[left]) * self.grid_scale
        } else {
            60.0 / self.bpm.unwrap_or(120.0)
        }
    }
    pub fn grid_bpm(&self) -> Option<f64> {
        self.neural_grid()
            .map(|g| g.bpm / self.grid_scale)
            .or(self.bpm)
    }
    pub fn snapped_grid_time(&self, seconds: f64) -> f64 {
        let mut beat = self.grid_position(seconds).round();
        if let Some(g) = self.neural_grid() {
            let origin = g.beats_seconds[self.grid_anchor_index];
            let raw_start = origin - self.grid_offset / self.grid_scale;
            let raw_end = origin
                + (self.duration_frames() / self.sample_rate() - self.grid_offset)
                    / self.grid_scale;
            let first = g.beats_seconds.partition_point(|t| *t < raw_start);
            let end = g.beats_seconds.partition_point(|t| *t < raw_end);
            if first >= end {
                return f64::NAN;
            }
            beat = (beat + self.grid_anchor_index as f64).clamp(first as f64, (end - 1) as f64)
                - self.grid_anchor_index as f64;
        }
        let time = self.grid_time(beat);
        if time < 0.0 {
            self.grid_time(self.grid_position(0.0).ceil())
        } else {
            time
        }
    }
    pub fn preparation_pending(&self) -> bool {
        (self.keylock || self.pitch.abs() > 0.0001)
            && (self.prepared_generation != self.preparation_generation
                || (self.prepared_rate - self.rate()).abs() > 1e-6)
    }
    pub fn fade_in(&mut self) {
        self.attack = 0.0;
    }
    #[inline]
    fn render(&mut self, output_rate: u32) -> [f32; 2] {
        if self.output_rate != output_rate {
            self.output_rate = output_rate;
            self.smoothing = 1.0 - (-1.0 / (output_rate as f32 * 0.008)).exp();
            self.low_coefficient =
                1.0 - (-2.0 * std::f32::consts::PI * 250.0 / output_rate as f32).exp();
            self.high_coefficient =
                1.0 - (-2.0 * std::f32::consts::PI * 2500.0 / output_rate as f32).exp();
            self.filter_counter = 0;
        }
        let smoothing = self.smoothing;
        let mut source = [0.0; 2];
        if let Some(original) = self.track.as_ref() {
            let use_prepared = self.scratch_remaining == 0
                && self.prepared_generation == self.preparation_generation
                && (self.prepared_rate - self.rate()).abs() < 1e-6
                && (self.keylock || self.pitch.abs() > 0.0001)
                && self.prepared.is_some();
            let track = if use_prepared {
                self.prepared.as_ref().unwrap()
            } else {
                original
            };
            if (self.playing && !self.jog_touch) || self.scratch_remaining > 0 {
                if let Some((start, end)) =
                    self.loop_region.filter(|_| self.cursor_target.is_none())
                {
                    let size = end - start;
                    if size >= 1.0 && self.position >= end {
                        self.position = start + (self.position - start).rem_euclid(size);
                        self.position_revision = self.position_revision.wrapping_add(1);
                    }
                    if size >= 1.0 && self.position < start && self.scratch_remaining > 0 {
                        self.position = end - (start - self.position).rem_euclid(size);
                        self.position_revision = self.position_revision.wrapping_add(1);
                    }
                }
                let length = original.samples.len();
                let read_position = if use_prepared {
                    self.position / self.prepared_rate
                } else {
                    self.position
                };
                if self.position >= 0.0
                    && self.position < (length.saturating_sub(1)) as f64
                    && read_position < (track.samples.len().saturating_sub(1)) as f64
                {
                    let index = read_position as usize;
                    let fractional = (read_position - index as f64) as f32;
                    for (channel, sample) in source.iter_mut().enumerate() {
                        *sample = track.samples[index][channel] * (1.0 - fractional)
                            + track.samples[index + 1][channel] * fractional;
                    }
                    self.attack = (self.attack + 1.0 / (output_rate as f32 * 0.003)).min(1.0);
                    // A brief tail stops a stationary cursor without a hard audio edge.
                    let tail = if self.cursor_target.is_some() {
                        (self.scratch_remaining as f32 / (output_rate as f32 * 0.002)).min(1.0)
                    } else {
                        1.0
                    };
                    source[0] *= self.attack * tail;
                    source[1] *= self.attack * tail;
                    let speed = if self.scratch_remaining > 0 {
                        self.scratch_speed
                    } else {
                        self.rate() + self.bend + self.sync_correction
                    };
                    self.position += speed * track.sample_rate as f64 / output_rate as f64;
                    if self.scratch_remaining > 0 {
                        self.scratch_remaining -= 1;
                        if self.scratch_remaining == 0 {
                            if let Some(target) = self.cursor_target.take() {
                                self.position = target;
                                if self.cursor_release_pending {
                                    self.cursor_release_pending = false;
                                    self.jog_touch = false;
                                    self.position_revision = self.position_revision.wrapping_add(1);
                                    self.attack = 0.0;
                                }
                            }
                        }
                    }
                    self.bend *= 1.0 - 1.0 / (output_rate as f64 * 0.15);
                } else {
                    if self.scratch_remaining == 0 {
                        self.playing = false;
                    }
                    self.scratch_remaining = 0;
                    self.cursor_target = None;
                    if self.cursor_release_pending {
                        self.jog_touch = false;
                    }
                    self.cursor_release_pending = false;
                    self.position_revision = self.position_revision.wrapping_add(1);
                    self.position = if self.position < 0.0 {
                        0.0
                    } else {
                        length as f64
                    };
                }
            }
        }
        let gain = self.gain.tick(smoothing);
        let eq = [
            self.eq[0].tick(smoothing),
            self.eq[1].tick(smoothing),
            self.eq[2].tick(smoothing),
        ];
        let amount = self.filter.tick(smoothing);
        let low_coefficient = self.low_coefficient;
        let high_coefficient = self.high_coefficient;
        if self.filter_counter == 0 {
            let cutoff = if amount < 0.0 {
                20000.0 * (0.001_f32).powf(-amount)
            } else {
                20.0 * (1000.0_f32).powf(amount)
            };
            self.filter_coefficient = 1.0
                - (-2.0 * std::f32::consts::PI * cutoff.min(output_rate as f32 * 0.45)
                    / output_rate as f32)
                    .exp();
        }
        self.filter_counter = (self.filter_counter + 1) % 32;
        let filter_coefficient = self.filter_coefficient;
        let mut output = [0.0; 2];
        for channel in 0..2 {
            self.band_low[channel] += low_coefficient * (source[channel] - self.band_low[channel]);
            self.band_high[channel] +=
                high_coefficient * (source[channel] - self.band_high[channel]);
            let low = self.band_low[channel];
            let mid = self.band_high[channel] - low;
            let high = source[channel] - self.band_high[channel];
            let equalized = low * eq[0] + mid * eq[1] + high * eq[2];
            self.filter_low[0][channel] +=
                filter_coefficient * (equalized - self.filter_low[0][channel]);
            self.filter_low[1][channel] +=
                filter_coefficient * (self.filter_low[0][channel] - self.filter_low[1][channel]);
            self.filter_high[0][channel] +=
                filter_coefficient * (equalized - self.filter_high[0][channel]);
            let hp1 = equalized - self.filter_high[0][channel];
            self.filter_high[1][channel] +=
                filter_coefficient * (hp1 - self.filter_high[1][channel]);
            let filtered = if amount < -0.001 {
                self.filter_low[1][channel]
            } else if amount > 0.001 {
                hp1 - self.filter_high[1][channel]
            } else {
                equalized
            };
            output[channel] = filtered * gain;
        }
        let echo = self.echo.tick(smoothing);
        let reverb = self.reverb.tick(smoothing);
        let delay_seconds = 60.0 / self.bpm.unwrap_or(120.0) / self.rate() * self.fx_beats;
        let delay_frames = (delay_seconds * output_rate as f64)
            .round()
            .clamp(1.0, (self.echo_buffer.len() - 1) as f64) as usize;
        let read =
            (self.echo_cursor + self.echo_buffer.len() - delay_frames) % self.echo_buffer.len();
        let wet = if self.echo_epoch[read] == self.epoch {
            self.echo_buffer[read]
        } else {
            [0.0; 2]
        };
        let tap1 = (self.reverb_cursor + self.reverb_buffer.len()
            - (output_rate as usize * 73 / 1000).min(self.reverb_buffer.len() - 1))
            % self.reverb_buffer.len();
        let tap2 = (self.reverb_cursor + self.reverb_buffer.len()
            - (output_rate as usize * 109 / 1000).min(self.reverb_buffer.len() - 1))
            % self.reverb_buffer.len();
        let r1 = if self.reverb_epoch[tap1] == self.epoch {
            self.reverb_buffer[tap1]
        } else {
            [0.0; 2]
        };
        let r2 = if self.reverb_epoch[tap2] == self.epoch {
            self.reverb_buffer[tap2]
        } else {
            [0.0; 2]
        };
        let reverberated = [r1[0] * 0.58 + r2[1] * 0.34, r1[1] * 0.58 + r2[0] * 0.34];
        for channel in 0..2 {
            // Gate writes when bypassed, so enabling FX doesn't reveal an old song.
            self.echo_buffer[self.echo_cursor][channel] =
                output[channel] * echo + wet[channel] * 0.42 * echo;
            self.reverb_buffer[self.reverb_cursor][channel] =
                output[channel] * reverb + reverberated[channel] * 0.56 * reverb;
            output[channel] += wet[channel] * echo + reverberated[channel] * reverb;
        }
        self.echo_epoch[self.echo_cursor] = self.epoch;
        self.reverb_epoch[self.reverb_cursor] = self.epoch;
        self.echo_cursor = (self.echo_cursor + 1) % self.echo_buffer.len();
        self.reverb_cursor = (self.reverb_cursor + 1) % self.reverb_buffer.len();
        self.peak = output;
        output
    }
}

#[derive(Clone, Copy)]
struct ControlRamp {
    target: ControlTarget,
    from: f32,
    to: f32,
    start: u64,
    end: u64,
    clock: Option<usize>,
    source_revision: Option<u64>,
    target_revision: Option<u64>,
    position_revision: Option<u64>,
}
#[derive(Clone, Copy)]
pub(crate) struct ScheduledLaunch {
    clock: usize,
    pub at_seconds: f64,
    revisions: [u64; 2],
    positions: [u64; 2],
    rates: [f64; 2],
    bpms: [Option<f64>; 2],
    grids: [f64; 2],
}
pub(crate) struct Mixer {
    pub applied_fence: u64,
    pub load_revisions: [u64; 2],
    ramps: [Option<ControlRamp>; 17],
    pub launches: [Option<ScheduledLaunch>; 2],
    pub decks: [Deck; 2],
    pub sample_rate: u32,
    pub master_gain: Smooth,
    pub crossfader: Smooth,
    pub cue_gain: Smooth,
    pub cue_mix: Smooth,
    pub rendered_frames: u64,
    pub master_deck: usize,
    pub sampler_gain: Smooth,
    pub samples: [Arc<TrackData>; 16],
    pub sample_positions: [Option<f64>; 16],
    smoothing: f32,
    sync_smoothing: f64,
    smoothing_rate: u32,
    fade_value: f32,
    fade_gains: [f32; 2],
}
impl Mixer {
    pub fn new() -> Self {
        Self {
            applied_fence: 0,
            load_revisions: [0; 2],
            ramps: [None; 17],
            launches: [None; 2],
            decks: [Deck::new(), Deck::new()],
            sample_rate: 48_000,
            master_gain: Smooth::new(0.8),
            crossfader: Smooth::new(0.0),
            cue_gain: Smooth::new(0.8),
            cue_mix: Smooth::new(0.0),
            rendered_frames: 0,
            master_deck: 0,
            sampler_gain: Smooth::new(0.7),
            samples: std::array::from_fn(crate::decode::sample_track),
            sample_positions: [None; 16],
            smoothing: 0.0,
            sync_smoothing: 0.0,
            smoothing_rate: 0,
            fade_value: f32::NAN,
            fade_gains: [0.70710677; 2],
        }
    }
    pub fn load(&mut self, deck: usize, track: Arc<TrackData>) -> Option<Arc<TrackData>> {
        self.launches.fill(None);
        let d = &mut self.decks[deck];
        if d.track
            .as_ref()
            .is_some_and(|t| !t.analysis.recording_allowed)
        {
            d.echo_buffer.fill([0.; 2]);
            d.reverb_buffer.fill([0.; 2]);
        }
        let previous = d.track.replace(track);
        d.position = 0.0;
        d.position_revision = d.position_revision.wrapping_add(1);
        d.playing = false;
        d.cue = 0.0;
        d.hot_cues = [None; 8];
        d.loop_region = None;
        d.last_loop_region = None;
        d.loop_edit = None;
        d.tempo = 0.0;
        d.bend = 0.0;
        d.scratch_remaining = 0;
        d.cursor_target = None;
        d.cursor_release_pending = false;
        d.attack = 0.0;
        d.preparation_generation = 0;
        d.prepared_generation = 0;
        d.pitch = 0.0;
        d.keylock = false;
        d.loop_in = None;
        d.jog_touch = false;
        d.memory_cues = [None; 10];
        d.epoch = d.epoch.wrapping_add(1).max(1);
        d.band_low = [0.0; 2];
        d.band_high = [0.0; 2];
        d.filter_low = [[0.0; 2]; 2];
        d.filter_high = [[0.0; 2]; 2];
        d.bpm = d.track.as_ref().and_then(|t| t.analysis.bpm);
        d.grid_offset = d
            .track
            .as_ref()
            .and_then(|t| t.analysis.beatgrid_offset_seconds)
            .filter(|x| x.is_finite())
            .unwrap_or(0.0);
        d.grid_valid = d.track.as_ref().is_some_and(|t| {
            t.analysis.rhythm_error.is_none()
                && t.analysis
                    .beatgrid_offset_seconds
                    .is_some_and(f64::is_finite)
        });
        d.grid_downbeat = d.grid_valid
            && d.track
                .as_ref()
                .is_some_and(|t| t.analysis.beatgrid_source.as_deref() == Some("manual"));
        d.grid_manual = d.grid_downbeat;
        d.grid_uniform = d
            .track
            .as_ref()
            .is_some_and(|t| t.analysis.bpm_source.as_deref() == Some("manual"));
        d.grid_scale = 1.0;
        d.grid_anchor_index = 0;
        if let Some(track) = d.track.as_ref() {
            if let Some(g) = track
                .analysis
                .neural_rhythm
                .as_ref()
                .filter(|g| g.beats_seconds.len() >= 2)
            {
                let first = g
                    .downbeats_seconds
                    .first()
                    .copied()
                    .unwrap_or(g.beats_seconds[0]);
                d.grid_anchor_index = track
                    .analysis
                    .manual_grid_anchor_index
                    .unwrap_or_else(|| {
                        if d.grid_manual {
                            crate::grid::nearest_index(&g.beats_seconds, d.grid_offset)
                        } else {
                            g.beats_seconds.partition_point(|t| *t < first)
                        }
                    })
                    .min(g.beats_seconds.len() - 1);
                if !d.grid_manual {
                    d.grid_offset = g.beats_seconds[d.grid_anchor_index];
                }
                if track.analysis.bpm_source.as_deref() == Some("manual") {
                    d.grid_scale = 1.0;
                }
                d.grid_valid = track.analysis.rhythm_error.is_none() || d.grid_manual;
                d.grid_downbeat =
                    d.grid_manual || (!d.grid_uniform && !g.downbeats_seconds.is_empty());
            }
        }
        previous
    }
    pub fn update_grid(
        &mut self,
        deck: usize,
        grid: Arc<audio_analysis::NeuralRhythm>,
    ) -> Option<Arc<audio_analysis::NeuralRhythm>> {
        self.launches.fill(None);
        self.ramps.fill(None);
        let d = &mut self.decks[deck];
        let previous_bpm = d.grid_bpm();
        let uniform = d.grid_uniform;
        let previous_phase_corrected = d
            .neural_grid()
            .is_some_and(|old| old.bar_evidence.iter().any(|e| e.half_phase_corrected));
        let previous_anchor = d
            .neural_grid()
            .and_then(|g| g.beats_seconds.get(d.grid_anchor_index))
            .copied()
            .unwrap_or(d.grid_offset);
        let previous = d.grid.replace(grid);
        let g = d.grid.as_ref().unwrap();
        if d.grid_manual {
            let phase_changed =
                previous_phase_corrected != g.bar_evidence.iter().any(|e| e.half_phase_corrected);
            // A human already compensated the old phase: bind to their source
            // time when a new detector explicitly replaces that phase.
            let anchor = if phase_changed {
                d.grid_offset
            } else {
                previous_anchor
            };
            d.grid_anchor_index = crate::grid::nearest_index(&g.beats_seconds, anchor);
        } else {
            let first = g
                .downbeats_seconds
                .first()
                .copied()
                .unwrap_or(g.beats_seconds[0]);
            d.grid_anchor_index = g
                .beats_seconds
                .partition_point(|t| *t < first)
                .min(g.beats_seconds.len() - 1);
            d.grid_offset = g.beats_seconds[d.grid_anchor_index];
        }
        // Preserve an explicitly retimed grid; otherwise adopt the new detector tempo.
        d.grid_scale = if (d.grid_scale - 1.0).abs() > 1e-8 {
            g.bpm / previous_bpm.unwrap_or(g.bpm)
        } else {
            1.0
        };
        d.bpm = if uniform {
            previous_bpm
        } else {
            Some(g.bpm / d.grid_scale)
        };
        d.grid_valid = true;
        d.grid_downbeat = d.grid_manual || (!uniform && !g.downbeats_seconds.is_empty());
        previous
    }
    /// Copy the source transport at this exact render boundary, sharing immutable PCM.
    pub fn instant_double(
        &mut self,
        deck: usize,
        track: Arc<TrackData>,
        generation: u64,
    ) -> [Option<Arc<TrackData>>; 2] {
        let source = &self.decks[1 - deck];
        let grid = source.grid.clone();
        let uniform = source.grid_uniform;
        let prepared = if source.prepared_generation == source.preparation_generation
            && (source.prepared_rate - source.rate()).abs() < 1e-6
        {
            source.prepared.clone()
        } else {
            None
        };
        let state = (
            source.position,
            source.playing,
            source.tempo,
            source.cue,
            source.hot_cues,
            source.memory_cues,
            source.loop_region,
            source.loop_in,
            source.pitch,
            source.keylock,
            source.bpm,
            source.grid_offset,
            source.prepared_rate,
            source.last_loop_region,
            source.grid_valid,
            source.grid_downbeat,
            source.grid_manual,
            source.grid_anchor_index,
            source.grid_scale,
        );
        let old_prepared = self.decks[deck].prepared.take();
        let old_track = self.load(deck, track);
        let d = &mut self.decks[deck];
        d.position = state.0;
        d.playing = state.1;
        d.tempo = state.2;
        d.cue = state.3;
        d.hot_cues = state.4;
        d.memory_cues = state.5;
        d.loop_region = state.6;
        d.loop_in = state.7;
        d.pitch = state.8;
        d.keylock = state.9;
        d.bpm = state.10;
        d.grid_offset = state.11;
        d.prepared_rate = state.12;
        d.last_loop_region = state.13;
        d.grid_valid = state.14;
        d.grid_downbeat = state.15;
        d.grid_manual = state.16;
        d.grid_anchor_index = state.17;
        d.grid_scale = state.18;
        d.grid = grid;
        d.grid_uniform = uniform;
        d.preparation_generation = generation;
        d.prepared_generation = if prepared.is_some() { generation } else { 0 };
        d.prepared = prepared;
        d.sync_enabled = false;
        [old_track, old_prepared]
    }
    pub fn unload(&mut self, deck: usize) -> [Option<Arc<TrackData>>; 2] {
        self.launches.fill(None);
        let d = &mut self.decks[deck];
        d.playing = false;
        d.grid_valid = false;
        d.grid_downbeat = false;
        d.cue_preview = false;
        d.position = 0.;
        d.position_revision = d.position_revision.wrapping_add(1);
        d.jog_touch = false;
        d.bpm = None;
        d.hot_cues = [None; 8];
        d.memory_cues = [None; 10];
        d.loop_region = None;
        d.last_loop_region = None;
        d.loop_edit = None;
        d.scratch_remaining = 0;
        d.cursor_target = None;
        d.cursor_release_pending = false;
        d.echo_buffer.fill([0.; 2]);
        d.reverb_buffer.fill([0.; 2]);
        [d.track.take(), d.prepared.take()]
    }
    pub fn command(&mut self, command: EngineCommand) {
        // Manual changes supersede a queued launch even within one buffer.
        if matches!(
            &command,
            EngineCommand::Play { .. }
                | EngineCommand::Pause { .. }
                | EngineCommand::Cue { .. }
                | EngineCommand::CuePress { .. }
                | EngineCommand::Seek { .. }
                | EngineCommand::TriggerHotCue { .. }
                | EngineCommand::BeatJump { .. }
                | EngineCommand::SetTempo { .. }
                | EngineCommand::SetBpm { .. }
                | EngineCommand::SetBeatgrid { .. }
                | EngineCommand::SetSync { .. }
                | EngineCommand::Sync { .. }
                | EngineCommand::Jog { .. }
                | EngineCommand::JogTouch { .. }
                | EngineCommand::ScrubTo { .. }
                | EngineCommand::SetLoop { .. }
                | EngineCommand::SetPitch { .. }
                | EngineCommand::SetKeylock { .. }
        ) {
            if let Some(deck) = command.deck() {
                for (target, pending) in self.launches.iter_mut().enumerate() {
                    if target == deck || pending.is_some_and(|p| p.clock == deck) {
                        *pending = None;
                    }
                }
            }
        }
        use EngineCommand::*;
        match &command {
            SetCrossfader { .. } => self.ramps[0] = None,
            SetMasterGain { .. } => self.ramps[1] = None,
            SetCueGain { .. } => self.ramps[2] = None,
            SetCueMix { .. } => self.ramps[3] = None,
            SetSamplerGain { .. } => self.ramps[4] = None,
            SetChannelFader { deck, .. } => {
                self.ramps[ControlTarget::ChannelFader { deck: *deck }.index()] = None
            }
            SetDeckGain { deck, .. } => {
                self.ramps[ControlTarget::DeckGain { deck: *deck }.index()] = None
            }
            SetFilter { deck, .. } => {
                self.ramps[ControlTarget::Filter { deck: *deck }.index()] = None
            }
            SetEq { deck, .. } => {
                for t in [
                    ControlTarget::EqLow { deck: *deck },
                    ControlTarget::EqMid { deck: *deck },
                    ControlTarget::EqHigh { deck: *deck },
                ] {
                    self.ramps[t.index()] = None;
                }
            }
            _ => {}
        }
        match command {
            Ramp {
                target,
                value,
                seconds,
                clock,
                beats,
                quantize,
            } => {
                let mut duration = seconds;
                let mut delay = 0.0;
                if let Some(deck) = clock {
                    let d = &self.decks[deck];
                    let position = d.grid_position(d.seconds());
                    let start = quantize.map_or(position, |q| (position / q).ceil() * q);
                    delay = ((d.grid_time(start) - d.seconds()) / d.rate()).max(0.0);
                    if let Some(beats) = beats {
                        duration = (d.grid_time(start + beats) - d.grid_time(start)) / d.rate();
                    }
                }
                let from = self.target_value(target);
                let start = self.rendered_frames + (delay * self.sample_rate as f64).round() as u64;
                let frames = (duration * self.sample_rate as f64).round().max(1.0) as u64;
                let guard = clock.or(target.deck());
                self.ramps[target.index()] = Some(ControlRamp {
                    target,
                    from,
                    to: value,
                    start,
                    end: start + frames,
                    clock,
                    source_revision: guard.map(|d| self.load_revisions[d]),
                    target_revision: target.deck().map(|d| self.load_revisions[d]),
                    position_revision: clock.map(|d| self.decks[d].position_revision),
                });
            }
            CancelRamps => self.ramps.fill(None),
            Play { deck } => {
                self.launches[deck] = None;
                let d = &mut self.decks[deck];
                if d.track.is_some() {
                    if d.position >= d.duration_frames() {
                        d.seek(d.cue);
                    }
                    d.playing = true;
                }
            }
            PlayQuantized {
                deck,
                clock,
                clock_revision,
                at_seconds,
                quantize,
            } => {
                let a = &self.decks[clock];
                let b = &self.decks[deck];
                if self.load_revisions[clock] != clock_revision
                    || !a.playing
                    || b.playing
                    || !a.grid_valid
                    || !b.grid_valid
                    || a.loop_region.is_some()
                    || b.loop_region.is_some()
                    || a.jog_touch
                    || b.jog_touch
                    || b.track.is_none()
                    || b.preparation_pending()
                {
                    return;
                }
                let (Some(a_bpm), Some(b_bpm)) = (a.grid_bpm(), b.grid_bpm()) else {
                    return;
                };
                if !(b.sync_enabled && clock == self.master_deck)
                    && (a_bpm * a.rate() - b_bpm * b.rate()).abs() > 0.01
                {
                    return;
                }
                let beat = a.grid_position(a.seconds());
                let at = at_seconds
                    .unwrap_or(a.grid_time((beat / quantize).floor().mul_add(quantize, quantize)));
                if !at.is_finite()
                    || at < a.seconds()
                    || at >= a.duration_frames() / a.sample_rate()
                    || (a.snapped_grid_time(at) - at).abs() > 0.001
                {
                    return;
                }
                let entry = b.snapped_grid_time(b.seconds());
                if !entry.is_finite()
                    || entry < 0.0
                    || entry >= b.duration_frames() / b.sample_rate()
                {
                    return;
                }
                self.decks[deck].seek(entry);
                self.decks[deck].sync_correction = 0.0;
                self.launches[deck] = Some(ScheduledLaunch {
                    clock,
                    at_seconds: at,
                    revisions: self.load_revisions,
                    positions: self.decks.each_ref().map(|d| d.position_revision),
                    rates: self.decks.each_ref().map(Deck::rate),
                    bpms: self.decks.each_ref().map(|d| d.bpm),
                    grids: self.decks.each_ref().map(|d| d.grid_offset),
                });
            }
            CancelLaunch { deck } => self.launches[deck] = None,
            Pause { deck } => {
                self.launches[deck] = None;
                self.decks[deck].playing = false;
            }
            TogglePlay { deck } => {
                let playing = self.decks[deck].playing || self.launches[deck].is_some();
                self.command(if playing {
                    Pause { deck }
                } else {
                    Play { deck }
                })
            }
            Cue { deck } => {
                let d = &mut self.decks[deck];
                d.playing = false;
                d.seek(d.cue);
            }
            SetCue { deck } => {
                let d = &mut self.decks[deck];
                d.cue = d.seconds();
            }
            SetCuePoint { deck, seconds } => {
                let d = &mut self.decks[deck];
                d.cue = seconds.clamp(0.0, d.duration_frames() / d.sample_rate());
            }
            RestoreHotCues { deck, cues } => self.decks[deck].hot_cues = cues,
            SetHotCuePoint {
                deck,
                index,
                seconds,
            } => {
                let d = &mut self.decks[deck];
                d.hot_cues[index] = Some(seconds.clamp(0.0, d.duration_frames() / d.sample_rate()));
            }
            CuePress { deck, pressed } => {
                let d = &mut self.decks[deck];
                if pressed {
                    if d.playing {
                        d.playing = false;
                        d.seek(d.cue);
                        d.cue_preview = false;
                    } else {
                        d.seek(d.cue);
                        d.playing = true;
                        d.cue_preview = true;
                    }
                } else if d.cue_preview {
                    d.playing = false;
                    d.seek(d.cue);
                    d.cue_preview = false;
                }
            }
            Seek { deck, seconds } => self.decks[deck].seek(seconds),
            ScrubTo {
                deck,
                seconds,
                duration_ms,
            } => {
                let d = &mut self.decks[deck];
                if d.jog_touch && d.track.is_some() {
                    let target = (seconds * d.sample_rate()).clamp(0.0, d.duration_frames());
                    let distance = target - d.position;
                    if distance.abs() > 1e-6 {
                        if d.scratch_remaining == 0 {
                            d.attack = 0.0;
                        }
                        let frames = (self.sample_rate as f64 * duration_ms.clamp(8.0, 100.0)
                            / 1000.0)
                            .round()
                            .max(1.0) as u32;
                        // Retarget the actual audio cursor, rather than overwriting a fixed
                        // delta grain. Frequent pointer updates cannot lose displacement.
                        d.scratch_speed =
                            distance / frames as f64 * self.sample_rate as f64 / d.sample_rate();
                        d.scratch_remaining = frames;
                        d.cursor_target = Some(target);
                    }
                }
            }
            ScrubEnd { deck, seconds } => {
                let d = &mut self.decks[deck];
                let target = (seconds * d.sample_rate()).clamp(0.0, d.duration_frames());
                if d.cursor_target.is_some()
                    && d.scratch_remaining > 0
                    && (target - d.position).abs() > 1e-6
                {
                    // A pointer-up may share an audio command batch with the last
                    // movement. Preserve that audible traversal rather than seeking
                    // over it before the callback has rendered even one sample.
                    d.scratch_speed = (target - d.position) / d.scratch_remaining as f64
                        * self.sample_rate as f64
                        / d.sample_rate();
                    d.cursor_target = Some(target);
                    d.cursor_release_pending = true;
                } else {
                    d.seek(seconds);
                    d.jog_touch = false;
                }
            }
            SetHotCue { deck, index } => {
                let d = &mut self.decks[deck];
                d.hot_cues[index] = Some(d.seconds());
            }
            TriggerHotCue { deck, index } => {
                let d = &mut self.decks[deck];
                if let Some(time) = d.hot_cues[index] {
                    d.seek(time);
                    d.playing = true;
                }
            }
            ToggleActiveLoop { deck } => {
                let d = &mut self.decks[deck];
                d.loop_edit = None;
                if let Some(region) = d.loop_region.take() {
                    d.last_loop_region = Some(region);
                } else {
                    d.loop_region = d.last_loop_region.filter(|(start, end)| {
                        *start >= 0.0 && *end <= d.duration_frames() && *end >= *start + 1.0
                    });
                }
            }
            SetLoop { deck, beats } => {
                let d = &mut self.decks[deck];
                d.loop_edit = None;
                if let Some(region) = d.loop_region {
                    d.last_loop_region = Some(region);
                }
                d.loop_region = beats.map(|beats| {
                    let beat = d.grid_position(d.seconds()).floor();
                    let start = d.grid_time(beat).max(0.0) * d.sample_rate();
                    let end =
                        (d.grid_time(beat + beats) * d.sample_rate()).min(d.duration_frames());
                    (start, end.max(start + 1.0))
                });
                if let Some(region) = d.loop_region {
                    d.last_loop_region = Some(region);
                }
            }
            SetLoopEdit { deck, boundary } => {
                let d = &mut self.decks[deck];
                d.loop_edit = if d.loop_region.is_some() {
                    boundary
                } else {
                    None
                };
            }
            SetFxBeat { deck, beats } => self.decks[deck].fx_beats = beats,
            JogTouch { deck, touched } => {
                let d = &mut self.decks[deck];
                d.jog_touch = touched;
                if touched || d.cursor_target.is_some() {
                    d.scratch_remaining = 0;
                    d.cursor_target = None;
                    d.cursor_release_pending = false;
                    d.bend = 0.0;
                }
                d.attack = 0.0;
            }
            RestoreMemoryCues { deck, mut cues } => {
                cues.sort_unstable_by(|a, b| {
                    a.unwrap_or(f64::INFINITY)
                        .total_cmp(&b.unwrap_or(f64::INFINITY))
                });
                self.decks[deck].memory_cues = cues;
            }
            MemoryCueSave { deck } => {
                let d = &mut self.decks[deck];
                let time = d.seconds();
                if !d
                    .memory_cues
                    .iter()
                    .flatten()
                    .any(|x| (x - time).abs() < 0.02)
                {
                    if let Some(index) = d.memory_cues.iter().position(Option::is_none) {
                        d.memory_cues[index] = Some(time);
                        d.memory_cues.sort_unstable_by(|a, b| {
                            a.unwrap_or(f64::INFINITY)
                                .total_cmp(&b.unwrap_or(f64::INFINITY))
                        });
                    }
                }
            }
            MemoryCueDelete { deck } => {
                let d = &mut self.decks[deck];
                let time = d.seconds();
                if let Some((index, _)) = d
                    .memory_cues
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.map(|x| (i, (x - time).abs())))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                {
                    for i in index..9 {
                        d.memory_cues[i] = d.memory_cues[i + 1];
                    }
                    d.memory_cues[9] = None;
                }
            }
            MemoryCueNext { deck } => {
                let d = &mut self.decks[deck];
                if let Some(time) = d
                    .memory_cues
                    .iter()
                    .flatten()
                    .copied()
                    .find(|x| *x > d.seconds() + 0.03)
                {
                    d.seek(time);
                }
            }
            MemoryCuePrevious { deck } => {
                let d = &mut self.decks[deck];
                if let Some(time) = d
                    .memory_cues
                    .iter()
                    .flatten()
                    .copied()
                    .rev()
                    .find(|x| *x < d.seconds() - 0.03)
                {
                    d.seek(time);
                }
            }
            TriggerSample { slot } => self.sample_positions[slot] = Some(0.0),
            StopSample { slot } => self.sample_positions[slot] = None,
            SetSamplerGain { gain } => self.sampler_gain.target = gain,
            ClearHotCue { deck, index } => self.decks[deck].hot_cues[index] = None,
            LoopIn { deck } => {
                let d = &mut self.decks[deck];
                d.loop_in = Some(d.position);
                if let Some(region) = d.loop_region {
                    d.last_loop_region = Some(region);
                }
                d.loop_region = None;
                d.loop_edit = None;
            }
            LoopOut { deck } => {
                let d = &mut self.decks[deck];
                if let Some(start) = d.loop_in {
                    if d.position > start + 1.0 {
                        d.loop_region = Some((start, d.position));
                        d.last_loop_region = d.loop_region;
                        d.position = start;
                        d.position_revision = d.position_revision.wrapping_add(1);
                        d.attack = 0.0;
                    }
                }
            }
            SetPitch { deck, semitones } => self.decks[deck].pitch = semitones,
            SetKeylock { deck, enabled } => self.decks[deck].keylock = enabled,
            BeatJump { deck, beats } => {
                let d = &mut self.decks[deck];
                let time = d.grid_time(d.grid_position(d.seconds()) + beats);
                d.seek(time);
            }
            SetTempo { deck, percent } => self.decks[deck].tempo = percent,
            Jog {
                deck,
                delta_seconds,
                scratch,
            } => {
                let d = &mut self.decks[deck];
                if let Some(boundary) = d.loop_edit {
                    if let Some((start, end)) = d.loop_region {
                        let delta = delta_seconds * d.sample_rate();
                        let length = d.duration_frames();
                        d.loop_region = Some(match boundary {
                            LoopBoundary::In => {
                                ((start + delta).clamp(0.0, (end - 1.0).max(0.0)), end)
                            }
                            LoopBoundary::Out => (
                                start,
                                (end + delta).clamp(start + 1.0, length.max(start + 1.0)),
                            ),
                        });
                        d.loop_in = d.loop_region.map(|(start, _)| start);
                        d.last_loop_region = d.loop_region;
                    }
                } else if scratch {
                    d.cursor_target = None;
                    d.cursor_release_pending = false;
                    d.scratch_speed = (delta_seconds / 0.035).clamp(-16.0, 16.0);
                    d.scratch_remaining = (self.sample_rate as f64 * 0.035) as u32;
                } else if !d.playing {
                    // Paused CDJ/rim jogging auditions the track instead of
                    // applying a speed bend to a stopped transport. Accumulate
                    // queued packets from the previous target, not the stale
                    // callback position; keep the final audible grain on release.
                    let target = (d.cursor_target.unwrap_or(d.position) / d.sample_rate()
                        + delta_seconds)
                        .max(0.0);
                    d.jog_touch = true;
                    self.command(EngineCommand::ScrubTo {
                        deck,
                        seconds: target,
                        duration_ms: 16.0,
                    });
                    self.command(EngineCommand::ScrubEnd {
                        deck,
                        seconds: target,
                    });
                } else {
                    d.bend = (delta_seconds * 5.0).clamp(-0.12, 0.12);
                }
            }
            Sync { deck } => {
                let other = if deck == self.master_deck {
                    1 - deck
                } else {
                    self.master_deck
                };
                let leader = &self.decks[other];
                let leader_period = leader.grid_period();
                let leader_rate = leader.rate();
                let leader_phase = leader.grid_position(leader.seconds()).rem_euclid(1.0);
                let d = &mut self.decks[deck];
                if d.grid_bpm().is_some() {
                    d.tempo = ((leader_rate * d.grid_period() / leader_period - 1.0) * 100.0)
                        .clamp(-90.0, 100.0);
                    if d.playing {
                        let beat = d.grid_position(d.seconds()).floor();
                        d.seek(d.grid_time(beat + leader_phase));
                    }
                }
            }
            SetSync { deck, enabled } => {
                if enabled {
                    self.command(Sync { deck });
                }
                self.decks[deck].sync_enabled = enabled;
                if !enabled {
                    self.decks[deck].sync_correction = 0.0;
                }
            }
            SetMaster { deck } => {
                self.launches.fill(None);
                self.master_deck = deck;
                self.decks[deck].sync_enabled = false;
            }
            SetChannelFader { deck, value } => self.decks[deck].channel_fader.target = value,
            SetDeckGain { deck, gain } => self.decks[deck].gain.target = gain,
            SetEq {
                deck,
                low,
                mid,
                high,
            } => {
                self.decks[deck].eq[0].target = low;
                self.decks[deck].eq[1].target = mid;
                self.decks[deck].eq[2].target = high;
            }
            SetFilter { deck, amount } => self.decks[deck].filter.target = amount,
            SetCrossfader { position } => self.crossfader.target = position,
            SetMasterGain { gain } => self.master_gain.target = gain,
            SetCueMix { mix } => self.cue_mix.target = mix,
            SetCueGain { gain } => self.cue_gain.target = gain,
            SetHeadphoneCue { deck, enabled } => self.decks[deck].cue_enabled = enabled,
            SetFx { deck, echo, reverb } => {
                self.decks[deck].echo.target = echo;
                self.decks[deck].reverb.target = reverb;
            }
            SetBpm { deck, bpm } => {
                let d = &mut self.decks[deck];
                if d.grid_bpm()
                    .is_none_or(|previous| (previous - bpm).abs() > 0.01)
                {
                    d.grid_uniform = true;
                    d.grid_downbeat = d.grid_manual;
                    d.grid_scale = 1.0;
                }
                d.bpm = Some(bpm);
            }
            SetBeatgrid {
                deck,
                offset_seconds,
                anchor_index,
            } => {
                let d = &mut self.decks[deck];
                if let Some(g) = d.neural_grid() {
                    let index = (d.grid_position(offset_seconds).round()
                        + d.grid_anchor_index as f64)
                        .clamp(0.0, (g.beats_seconds.len() - 1) as f64)
                        as usize;
                    d.grid_anchor_index =
                        anchor_index.unwrap_or(index).min(g.beats_seconds.len() - 1);
                }
                d.grid_offset = offset_seconds;
                d.grid_valid = true;
                d.grid_downbeat = true;
                d.grid_manual = true;
            }
        }
    }
    fn target_mut(&mut self, target: ControlTarget) -> &mut Smooth {
        match target {
            ControlTarget::Crossfader => &mut self.crossfader,
            ControlTarget::MasterGain => &mut self.master_gain,
            ControlTarget::CueGain => &mut self.cue_gain,
            ControlTarget::CueMix => &mut self.cue_mix,
            ControlTarget::SamplerGain => &mut self.sampler_gain,
            ControlTarget::ChannelFader { deck } => &mut self.decks[deck].channel_fader,
            ControlTarget::DeckGain { deck } => &mut self.decks[deck].gain,
            ControlTarget::Filter { deck } => &mut self.decks[deck].filter,
            ControlTarget::EqLow { deck } => &mut self.decks[deck].eq[0],
            ControlTarget::EqMid { deck } => &mut self.decks[deck].eq[1],
            ControlTarget::EqHigh { deck } => &mut self.decks[deck].eq[2],
        }
    }
    fn target_value(&mut self, target: ControlTarget) -> f32 {
        self.target_mut(target).target
    }
    pub fn active_ramps(&self) -> [bool; 17] {
        self.ramps.map(|r| r.is_some())
    }
    fn advance_ramps(&mut self) {
        for i in 0..self.ramps.len() {
            let Some(ramp) = self.ramps[i] else {
                continue;
            };
            let guard = ramp.clock.or(ramp.target.deck());
            if guard.is_some_and(|d| Some(self.load_revisions[d]) != ramp.source_revision)
                || ramp
                    .target
                    .deck()
                    .is_some_and(|d| Some(self.load_revisions[d]) != ramp.target_revision)
                || ramp.clock.is_some_and(|d| {
                    !self.decks[d].playing
                        || Some(self.decks[d].position_revision) != ramp.position_revision
                })
            {
                self.ramps[i] = None;
                continue;
            }
            if self.rendered_frames < ramp.start {
                continue;
            }
            let progress = ((self.rendered_frames - ramp.start) as f64
                / (ramp.end - ramp.start) as f64)
                .min(1.0) as f32;
            self.target_mut(ramp.target).target = ramp.from + (ramp.to - ramp.from) * progress;
            if progress >= 1.0 {
                self.ramps[i] = None;
            }
        }
    }
    fn advance_launches(&mut self) {
        for deck in 0..2 {
            let Some(pending) = self.launches[deck] else {
                continue;
            };
            let valid = self.load_revisions == pending.revisions
                && self.decks[pending.clock].playing
                && !self.decks[deck].playing
                && self.decks.iter().enumerate().all(|(i, d)| {
                    d.grid_valid
                        && !d.jog_touch
                        && d.loop_region.is_none()
                        && d.position_revision == pending.positions[i]
                        && (d.rate() - pending.rates[i]).abs() < 1e-9
                        && d.bpm == pending.bpms[i]
                        && d.grid_offset == pending.grids[i]
                })
                && !self.decks[deck].preparation_pending();
            if !valid {
                self.launches[deck] = None;
                continue;
            }
            // Sample-clock comparison, not a wall-clock timer or UI poll.
            if self.decks[pending.clock].seconds() >= pending.at_seconds {
                self.launches[deck] = None;
                self.decks[deck].sync_correction = 0.0;
                self.decks[deck].playing = true;
                self.decks[deck].fade_in();
            }
        }
    }
    #[inline]
    pub fn frame(&mut self) -> ([f32; 2], [f32; 2]) {
        if self.smoothing_rate != self.sample_rate {
            self.smoothing_rate = self.sample_rate;
            self.smoothing = 1.0 - (-1.0 / (self.sample_rate as f32 * 0.008)).exp();
            self.sync_smoothing = 1.0 - (-1.0 / (self.sample_rate as f64 * 0.030)).exp();
        }
        self.advance_launches();
        self.advance_ramps();
        let leader_phase =
            self.decks[self.master_deck].grid_position(self.decks[self.master_deck].seconds());
        let leader_period = self.decks[self.master_deck].grid_period();
        let leader_rate = self.decks[self.master_deck].rate();
        let leader_playing =
            self.decks[self.master_deck].playing && !self.decks[self.master_deck].jog_touch;
        let sync_smoothing = self.sync_smoothing;
        let leader_bpm = self.decks[self.master_deck]
            .grid_bpm()
            .map(|bpm| bpm * self.decks[self.master_deck].rate());
        for deck in 0..2 {
            if deck != self.master_deck
                && self.decks[deck].sync_enabled
                && self.decks[deck].grid_valid
                && self.decks[self.master_deck].grid_valid
            {
                if !self.decks[deck].playing && self.launches[deck].is_some() {
                    continue;
                }
                if leader_bpm.is_some() && self.decks[deck].grid_bpm().is_some() {
                    let local_rate = leader_rate * self.decks[deck].grid_period() / leader_period;
                    let desired_tempo = ((local_rate - 1.0) * 100.0).clamp(-90.0, 100.0);
                    if leader_playing && self.decks[deck].playing {
                        self.decks[deck].tempo +=
                            (desired_tempo - self.decks[deck].tempo) * sync_smoothing;
                    } else {
                        self.decks[deck].tempo = desired_tempo;
                    }
                    let follower = self.decks[deck].grid_position(self.decks[deck].seconds());
                    let difference = (leader_phase - follower + 0.5).rem_euclid(1.0) - 0.5;
                    let d = &mut self.decks[deck];
                    if leader_playing && d.playing && !d.jog_touch && d.cursor_target.is_none() {
                        // Feed forward the source-time slope of both observed grids.
                        // Mean BPM alone cannot track locally varying beat intervals.
                        // Bound and smooth varispeed correction; never seek live audio.
                        let target = (difference * 0.5).clamp(-0.08, 0.08);
                        d.sync_correction += (target - d.sync_correction) * sync_smoothing;
                    } else {
                        d.sync_correction = 0.0;
                    }
                }
            }
        }
        let left = self.decks[0].render(self.sample_rate);
        let right = self.decks[1].render(self.sample_rate);
        let smoothing = self.smoothing;
        let channel_left = self.decks[0].channel_fader.tick(smoothing);
        let channel_right = self.decks[1].channel_fader.tick(smoothing);
        let fader = self.crossfader.tick(smoothing);
        // Constant-power crossfade: center has -3 dB on each deck.
        if fader != self.fade_value {
            self.fade_value = fader;
            let angle = (fader + 1.0) * std::f32::consts::FRAC_PI_4;
            self.fade_gains = [angle.cos(), angle.sin()];
        }
        let a = self.fade_gains[0];
        let b = self.fade_gains[1];
        let master_gain = self.master_gain.tick(smoothing);
        let cue_gain = self.cue_gain.tick(smoothing);
        let cue_mix = self.cue_mix.tick(smoothing);
        let mut master = [0.0; 2];
        let mut cue = [0.0; 2];
        for channel in 0..2 {
            master[channel] = (left[channel] * a * channel_left
                + right[channel] * b * channel_right)
                * master_gain;
            let selected = if self.decks[0].cue_enabled {
                left[channel]
            } else {
                0.0
            } + if self.decks[1].cue_enabled {
                right[channel]
            } else {
                0.0
            };
            cue[channel] = (selected * (1.0 - cue_mix) + master[channel] * cue_mix) * cue_gain;
        }
        let sampler_gain = self.sampler_gain.tick(smoothing);
        let mut sampler = [0.0_f32; 2];
        for slot in 0..16 {
            if let Some(position) = self.sample_positions[slot] {
                let track = &self.samples[slot];
                let index = position as usize;
                if index + 1 < track.samples.len() {
                    let fractional = (position - index as f64) as f32;
                    for (channel, sample) in sampler.iter_mut().enumerate() {
                        *sample += (track.samples[index][channel] * (1.0 - fractional)
                            + track.samples[index + 1][channel] * fractional)
                            * sampler_gain;
                    }
                    self.sample_positions[slot] =
                        Some(position + track.sample_rate as f64 / self.sample_rate as f64);
                } else {
                    self.sample_positions[slot] = None;
                }
            }
        }
        for channel in 0..2 {
            master[channel] += sampler[channel] * master_gain;
            cue[channel] += sampler[channel] * master_gain * cue_mix * cue_gain;
        }
        self.rendered_frames += 1;
        (master, cue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn constant(value: f32, seconds: f64) -> Arc<TrackData> {
        let mut t = crate::decode::demo_track(0);
        let t_mut = Arc::get_mut(&mut t).unwrap();
        t_mut.samples = vec![[value; 2]; (seconds * 48000.0) as usize];
        t_mut.analysis.duration_seconds = seconds;
        t
    }
    #[test]
    fn transport_clock_cue_loop_and_eof() {
        let mut m = Mixer::new();
        m.load(0, constant(0.3, 2.0));
        m.command(EngineCommand::Play { deck: 0 });
        for _ in 0..24000 {
            m.frame();
        }
        assert!((m.decks[0].seconds() - 0.5).abs() < 1e-9);
        m.command(EngineCommand::SetCue { deck: 0 });
        m.command(EngineCommand::SetLoop {
            deck: 0,
            beats: Some(1.0),
        });
        for _ in 0..72000 {
            m.frame();
        }
        assert!(m.decks[0].seconds() >= 0.5 && m.decks[0].seconds() <= 1.0);
        m.command(EngineCommand::Cue { deck: 0 });
        assert!(!m.decks[0].playing);
        assert!((m.decks[0].seconds() - 0.5).abs() < 1e-9);
        m.command(EngineCommand::SetLoop {
            deck: 0,
            beats: None,
        });
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: 1.9,
        });
        m.command(EngineCommand::Play { deck: 0 });
        for _ in 0..9600 {
            m.frame();
        }
        assert!(!m.decks[0].playing);
        assert_eq!(m.decks[0].seconds(), 2.0);
    }
    #[test]
    fn cue_is_independent_of_crossfader() {
        let mut m = Mixer::new();
        m.load(0, constant(0.25, 2.0));
        m.load(1, constant(0.5, 2.0));
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::Play { deck: 1 });
        m.command(EngineCommand::SetHeadphoneCue {
            deck: 1,
            enabled: true,
        });
        m.command(EngineCommand::SetCrossfader { position: -1.0 });
        let mut last = ([0.0; 2], [0.0; 2]);
        for _ in 0..12000 {
            last = m.frame();
        }
        assert!((last.0[0] - 0.2).abs() < 0.0001);
        assert!((last.1[0] - 0.4).abs() < 0.0001);
    }
    #[test]
    fn rate_and_sync_match_effective_bpm() {
        let mut m = Mixer::new();
        m.load(0, constant(0.1, 8.0));
        m.load(1, constant(0.1, 8.0));
        m.command(EngineCommand::SetBpm {
            deck: 1,
            bpm: 100.0,
        });
        m.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 10.0,
        });
        m.command(EngineCommand::Sync { deck: 1 });
        assert!((m.decks[1].rate() * 100.0 - 132.0).abs() < 1e-9);
        m.command(EngineCommand::Play { deck: 1 });
        for _ in 0..48000 {
            m.frame();
        }
        assert!((m.decks[1].seconds() - 1.32).abs() < 1e-8);
    }
}

#[cfg(test)]
mod analyzed_phase_load_tests {
    use super::*;
    #[test]
    fn initial_deck_grid_uses_track_analysis_phase() {
        let mut track = crate::decode::demo_track(0);
        let data = Arc::get_mut(&mut track).unwrap();
        data.analysis.beatgrid_offset_seconds = Some(0.173);
        data.analysis.beatgrid_source = Some("analysis".into());
        let mut mixer = Mixer::new();
        mixer.load(0, track);
        assert_eq!(mixer.decks[0].grid_offset, 0.173);
        mixer.command(EngineCommand::SetBeatgrid {
            anchor_index: None,
            deck: 0,
            offset_seconds: 0.287,
        });
        assert_eq!(mixer.decks[0].grid_offset, 0.287);
    }
}

#[cfg(test)]
mod cursor_hold_tests {
    use super::*;
    #[test]
    fn held_cursor_absolute_seeks_are_exact_and_playback_resumes_on_release() {
        let mut mixer = Mixer::new();
        mixer.load(0, crate::decode::demo_track(0));
        let loaded = mixer.decks[0].position_revision;
        mixer.command(EngineCommand::Play { deck: 0 });
        for _ in 0..4800 {
            mixer.frame();
        }
        assert_eq!(mixer.decks[0].position_revision, loaded);
        // A previously queued physical scratch must not leak into held seek.
        mixer.command(EngineCommand::Jog {
            deck: 0,
            delta_seconds: 0.08,
            scratch: true,
        });
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: true,
        });
        let held = mixer.decks[0].seconds();
        for _ in 0..3200 {
            mixer.frame();
        }
        assert_eq!(mixer.decks[0].seconds(), held);
        assert!(mixer.decks[0].playing);
        for target in [2.0, 8.125, 3.75, 17.03125] {
            let revision = mixer.decks[0].position_revision;
            mixer.command(EngineCommand::Seek {
                deck: 0,
                seconds: target,
            });
            for _ in 0..800 {
                mixer.frame();
            }
            assert_eq!(mixer.decks[0].seconds(), target);
            assert_eq!(mixer.decks[0].position_revision, revision + 1);
            assert!(mixer.decks[0].playing && mixer.decks[0].jog_touch);
        }
        let released = mixer.decks[0].seconds();
        let revision = mixer.decks[0].position_revision;
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: false,
        });
        for _ in 0..4800 {
            mixer.frame();
        }
        assert!((mixer.decks[0].seconds() - released - 0.1).abs() < 1e-9);
        assert_eq!(mixer.decks[0].position_revision, revision);
        assert!(mixer.decks[0].playing);
    }
    #[test]
    fn paused_cursor_remains_paused_after_held_seek() {
        let mut mixer = Mixer::new();
        mixer.load(0, crate::decode::demo_track(0));
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: true,
        });
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 4.125,
        });
        for _ in 0..800 {
            mixer.frame();
        }
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: false,
        });
        for _ in 0..800 {
            mixer.frame();
        }
        assert_eq!(mixer.decks[0].seconds(), 4.125);
        assert!(!mixer.decks[0].playing);
    }
    #[test]
    fn loop_wrap_and_unload_are_position_discontinuities() {
        let mut mixer = Mixer::new();
        mixer.load(0, crate::decode::demo_track(0));
        let loaded = mixer.decks[0].position_revision;
        mixer.command(EngineCommand::SetLoop {
            deck: 0,
            beats: Some(1.0),
        });
        mixer.command(EngineCommand::Play { deck: 0 });
        for _ in 0..24001 {
            mixer.frame();
        }
        assert_eq!(mixer.decks[0].position_revision, loaded + 1);
        mixer.unload(0);
        assert_eq!(mixer.decks[0].position_revision, loaded + 2);
        assert_eq!(mixer.decks[0].position, 0.0);
        assert!(!mixer.decks[0].jog_touch);
    }
}

#[cfg(test)]
mod synced_cursor_hold_tests {
    use super::*;
    #[test]
    fn sync_maintenance_does_not_move_a_held_absolute_seek() {
        let mut mixer = Mixer::new();
        mixer.load(0, crate::decode::demo_track(0));
        mixer.load(1, crate::decode::demo_track(1));
        mixer.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 10.0,
        });
        mixer.command(EngineCommand::Play { deck: 0 });
        mixer.command(EngineCommand::Play { deck: 1 });
        mixer.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        mixer.command(EngineCommand::JogTouch {
            deck: 1,
            touched: true,
        });
        for target in [4.25, 9.125, 1.0625] {
            mixer.command(EngineCommand::Seek {
                deck: 1,
                seconds: target,
            });
            let revision = mixer.decks[1].position_revision;
            for _ in 0..2400 {
                mixer.frame();
            }
            assert_eq!(mixer.decks[1].seconds(), target);
            assert_eq!(mixer.decks[1].position_revision, revision);
            assert!(mixer.decks[1].playing && mixer.decks[1].sync_enabled);
            assert!((mixer.decks[1].rate() * 124.0 - 132.0).abs() < 1e-9);
        }
        let held = mixer.decks[1].seconds();
        let revision = mixer.decks[1].position_revision;
        mixer.command(EngineCommand::JogTouch {
            deck: 1,
            touched: false,
        });
        for _ in 0..2400 {
            mixer.frame();
        }
        assert!(mixer.decks[1].seconds() > held + 0.045 && mixer.decks[1].seconds() < held + 0.07);
        assert_eq!(mixer.decks[1].position_revision, revision);
        assert!(mixer.decks[1].playing && mixer.decks[1].sync_enabled);
    }
}

#[cfg(test)]
mod audible_cursor_tests {
    use super::*;
    fn ramp() -> Arc<TrackData> {
        Arc::new(TrackData {
            samples: (0..96_000)
                .map(|i| {
                    let v = 0.1 + i as f32 / 120_000.0;
                    [v, v]
                })
                .collect(),
            sample_rate: 48_000,
            analysis: crate::decode::demo_track(0).analysis.clone(),
        })
    }
    #[test]
    fn paused_drag_reads_pcm_in_both_directions_and_stops_when_stationary() {
        let mut mixer = Mixer::new();
        mixer.load(0, ramp());
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 0.5,
        });
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: true,
        });
        for target in [0.75, 0.25] {
            let from = mixer.decks[0].seconds();
            mixer.command(EngineCommand::ScrubTo {
                deck: 0,
                seconds: target,
                duration_ms: 32.0,
            });
            let frames = 1536;
            let mut previous = from;
            let mut early = 0.0;
            let mut later = 0.0;
            for i in 0..frames {
                let output = mixer.decks[0].render(48_000)[0];
                if i == 300 {
                    early = output;
                }
                if i == 900 {
                    later = output;
                }
                let current = mixer.decks[0].seconds();
                assert!((current - previous) * (target - from) >= -1e-10);
                previous = current;
            }
            assert!((mixer.decks[0].seconds() - target).abs() < 1e-10);
            assert!(
                early > 0.05 && later > 0.05,
                "moving a paused platter must emit source PCM"
            );
            assert!(
                (later - early) as f64 * (target - from) > 0.0,
                "source sample order follows the drag direction"
            );
            assert!((0..100).all(|_| mixer.decks[0].render(48_000)[0].abs() < 1e-6));
            assert!(!mixer.decks[0].playing);
        }
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: false,
        });
        assert!(!mixer.decks[0].playing);
    }
    #[test]
    fn retargeted_scrubbing_keeps_other_deck_running_and_release_resumes_from_final_target() {
        let mut mixer = Mixer::new();
        mixer.load(0, ramp());
        mixer.load(1, ramp());
        mixer.command(EngineCommand::Play { deck: 0 });
        mixer.command(EngineCommand::Play { deck: 1 });
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 0.5,
        });
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: true,
        });
        for target in [0.6, 0.7, 0.8, 0.65, 0.4] {
            mixer.command(EngineCommand::ScrubTo {
                deck: 0,
                seconds: target,
                duration_ms: 24.0,
            });
            let energy: f32 = (0..768).map(|_| mixer.frame().0[0].abs()).sum();
            assert!(energy > 10.0);
            assert!(mixer.decks[0].seconds() > 0.0 && mixer.decks[0].seconds() < 1.0);
        }
        assert!((mixer.decks[1].seconds() - 0.08).abs() < 1e-9);
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 0.4,
        });
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: false,
        });
        for _ in 0..480 {
            mixer.frame();
        }
        assert!((mixer.decks[0].seconds() - 0.41).abs() < 1e-9);
        assert!(mixer.decks[0].playing);
        // A stale movement arriving after release cannot restart scrubbing.
        mixer.command(EngineCommand::ScrubTo {
            deck: 0,
            seconds: 1.0,
            duration_ms: 24.0,
        });
        assert_eq!(mixer.decks[0].scratch_remaining, 0);
    }
    #[test]
    fn scrubbing_uses_original_pcm_despite_keylock_and_bounds_invalid_requests() {
        let mut mixer = Mixer::new();
        mixer.load(0, ramp());
        let mut alternate = ramp();
        Arc::get_mut(&mut alternate)
            .unwrap()
            .samples
            .fill([-0.5; 2]);
        let d = &mut mixer.decks[0];
        d.prepared = Some(alternate);
        d.keylock = true;
        d.prepared_generation = d.preparation_generation;
        d.prepared_rate = 1.0;
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 0.5,
        });
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: true,
        });
        mixer.command(EngineCommand::ScrubTo {
            deck: 0,
            seconds: 0.75,
            duration_ms: 32.0,
        });
        let output = (0..600)
            .map(|_| mixer.decks[0].render(48_000)[0])
            .last()
            .unwrap();
        assert!(
            output > 0.1,
            "keylocked forward asset must not replace cursor PCM"
        );
        for target in [-100.0, 100.0] {
            mixer.command(EngineCommand::ScrubTo {
                deck: 0,
                seconds: target,
                duration_ms: 8.0,
            });
            for _ in 0..500 {
                mixer.frame();
            }
            assert!((0.0..=2.0).contains(&mixer.decks[0].seconds()));
        }
        assert!(EngineCommand::ScrubTo {
            deck: 0,
            seconds: f64::NAN,
            duration_ms: 16.0
        }
        .validate()
        .is_err());
        assert!(EngineCommand::ScrubTo {
            deck: 0,
            seconds: 0.5,
            duration_ms: f64::INFINITY
        }
        .validate()
        .is_err());
        assert!(EngineCommand::ScrubTo {
            deck: 2,
            seconds: 0.5,
            duration_ms: 16.0
        }
        .validate()
        .is_err());
    }
}

#[cfg(test)]
mod short_cursor_release_tests {
    use super::*;
    #[test]
    fn paused_rim_jog_accumulates_packets_audibly_and_does_not_start_playback() {
        let mut mixer = Mixer::new();
        mixer.load(0, crate::decode::demo_track(0));
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 2.0,
        });
        for _ in 0..20 {
            mixer.command(EngineCommand::Jog {
                deck: 0,
                delta_seconds: 0.01,
                scratch: false,
            });
        }
        let energy: f32 = (0..1000).map(|_| mixer.frame().0[0].abs()).sum();
        assert!(energy > 1.0);
        assert!((mixer.decks[0].seconds() - 2.2).abs() < 1e-9);
        assert!(!mixer.decks[0].playing);
        assert!(!mixer.decks[0].jog_touch);
        for _ in 0..10 {
            mixer.command(EngineCommand::Jog {
                deck: 0,
                delta_seconds: -0.01,
                scratch: false,
            });
        }
        let reverse_energy: f32 = (0..1000).map(|_| mixer.frame().0[0].abs()).sum();
        assert!(reverse_energy > 1.0);
        assert!((mixer.decks[0].seconds() - 2.1).abs() < 1e-9);
    }
    #[test]
    fn final_drag_and_release_in_one_audio_batch_still_render_before_play_state_restores() {
        for playing in [false, true] {
            let mut mixer = Mixer::new();
            mixer.load(0, crate::decode::demo_track(0));
            if playing {
                mixer.command(EngineCommand::Play { deck: 0 });
            }
            mixer.command(EngineCommand::JogTouch {
                deck: 0,
                touched: true,
            });
            mixer.command(EngineCommand::Seek {
                deck: 0,
                seconds: 2.0,
            });
            mixer.command(EngineCommand::ScrubTo {
                deck: 0,
                seconds: 2.05,
                duration_ms: 24.0,
            });
            mixer.command(EngineCommand::ScrubEnd {
                deck: 0,
                seconds: 2.05,
            });
            assert!(mixer.decks[0].jog_touch);
            let energy: f32 = (0..1152).map(|_| mixer.frame().0[0].abs()).sum();
            assert!(
                energy > 1.0,
                "release cannot cancel all output from a fast gesture"
            );
            assert!(!mixer.decks[0].jog_touch);
            assert!((mixer.decks[0].seconds() - 2.05).abs() < 1e-9);
            assert_eq!(mixer.decks[0].playing, playing);
            for _ in 0..480 {
                mixer.frame();
            }
            assert!(
                (mixer.decks[0].seconds() - 2.05 - if playing { 0.01 } else { 0.0 }).abs() < 1e-9
            );
        }
    }
}

#[cfg(test)]
mod flx4_controls_tests {
    use super::*;
    #[test]
    fn instant_double_shares_pcm_and_copies_actual_transport_without_reset_frame() {
        let mut mixer = Mixer::new();
        let source = crate::decode::demo_track(0);
        mixer.load(0, source.clone());
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 7.25,
        });
        mixer.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 8.0,
        });
        mixer.command(EngineCommand::SetPitch {
            deck: 0,
            semitones: 2.0,
        });
        mixer.command(EngineCommand::SetKeylock {
            deck: 0,
            enabled: true,
        });
        mixer.command(EngineCommand::SetHotCue { deck: 0, index: 7 });
        mixer.command(EngineCommand::MemoryCueSave { deck: 0 });
        mixer.command(EngineCommand::SetCue { deck: 0 });
        mixer.command(EngineCommand::SetBeatgrid {
            anchor_index: None,
            deck: 0,
            offset_seconds: 0.173,
        });
        mixer.command(EngineCommand::Play { deck: 0 });
        for _ in 0..1000 {
            mixer.frame();
        }
        // Already prepared keylock data can be shared immediately too.
        mixer.decks[0].prepared = Some(source.clone());
        mixer.decks[0].prepared_rate = mixer.decks[0].rate();
        mixer.decks[0].preparation_generation = 9;
        mixer.decks[0].prepared_generation = 9;
        let position = mixer.decks[0].position;
        let retired = mixer.instant_double(1, source.clone(), 42);
        assert!(retired.into_iter().all(|t| t.is_none()));
        let a = &mixer.decks[0];
        let b = &mixer.decks[1];
        assert!(Arc::ptr_eq(
            a.track.as_ref().unwrap(),
            b.track.as_ref().unwrap()
        ));
        assert_eq!(b.position, position);
        assert!(Arc::ptr_eq(
            a.prepared.as_ref().unwrap(),
            b.prepared.as_ref().unwrap()
        ));
        assert_eq!(b.prepared_generation, 42);
        assert_eq!(b.preparation_generation, 42);
        assert_eq!(b.tempo, a.tempo);
        assert_eq!(b.playing, a.playing);
        assert_eq!(b.cue, a.cue);
        assert_eq!(b.hot_cues, a.hot_cues);
        assert_eq!(b.memory_cues, a.memory_cues);
        assert_eq!(
            (b.pitch, b.keylock, b.bpm, b.grid_offset),
            (a.pitch, a.keylock, a.bpm, a.grid_offset)
        );
        mixer.frame();
        assert_eq!(mixer.decks[0].position, mixer.decks[1].position);
    }
    #[test]
    fn loop_jog_edit_changes_only_selected_boundary_and_clamps() {
        let mut mixer = Mixer::new();
        mixer.load(0, crate::decode::sample_track(0));
        mixer.decks[0].position = 12000.0;
        mixer.decks[0].loop_region = Some((4800.0, 24000.0));
        mixer.command(EngineCommand::SetLoopEdit {
            deck: 0,
            boundary: Some(LoopBoundary::In),
        });
        mixer.command(EngineCommand::Jog {
            deck: 0,
            delta_seconds: 0.05,
            scratch: true,
        });
        assert_eq!(mixer.decks[0].loop_region, Some((7200.0, 24000.0)));
        assert_eq!(mixer.decks[0].position, 12000.0);
        assert_eq!(mixer.decks[0].scratch_remaining, 0);
        mixer.command(EngineCommand::Jog {
            deck: 0,
            delta_seconds: 20.0,
            scratch: false,
        });
        assert_eq!(mixer.decks[0].loop_region, Some((23999.0, 24000.0)));
        mixer.command(EngineCommand::SetLoopEdit {
            deck: 0,
            boundary: Some(LoopBoundary::Out),
        });
        mixer.command(EngineCommand::Jog {
            deck: 0,
            delta_seconds: -20.0,
            scratch: true,
        });
        assert_eq!(mixer.decks[0].loop_region, Some((23999.0, 24000.0)));
        mixer.command(EngineCommand::Jog {
            deck: 0,
            delta_seconds: 20.0,
            scratch: true,
        });
        assert_eq!(mixer.decks[0].loop_region, Some((23999.0, 48000.0)));
        mixer.command(EngineCommand::SetLoop {
            deck: 0,
            beats: None,
        });
        assert_eq!(mixer.decks[0].loop_edit, None);
        assert!(!mixer.decks[0].playing);
    }
    #[test]
    fn all_sixteen_sampler_slots_render_and_stop_independently() {
        let mut mixer = Mixer::new();
        for slot in 0..16 {
            let command = EngineCommand::TriggerSample { slot };
            assert!(command.validate().is_ok());
            mixer.command(command);
            let energy: f32 = (0..2048).map(|_| mixer.frame().0[0].abs()).sum();
            assert!(energy > 0.1, "slot {slot}");
            mixer.command(EngineCommand::StopSample { slot });
            assert!(mixer.sample_positions.iter().all(Option::is_none));
        }
        assert!(EngineCommand::TriggerSample { slot: 16 }
            .validate()
            .is_err());
    }
    #[test]
    fn configurable_echo_beat_changes_audible_delay() {
        let mut mixer = Mixer::new();
        let mut data = Arc::try_unwrap(crate::decode::sample_track(0))
            .ok()
            .unwrap();
        data.samples = vec![[0.0; 2]; 96000];
        data.samples[1000] = [0.5; 2];
        mixer.load(0, Arc::new(data));
        mixer.command(EngineCommand::SetBpm {
            deck: 0,
            bpm: 120.0,
        });
        mixer.command(EngineCommand::SetFxBeat {
            deck: 0,
            beats: 1.0,
        });
        mixer.command(EngineCommand::SetFx {
            deck: 0,
            echo: 1.0,
            reverb: 0.0,
        });
        mixer.command(EngineCommand::Play { deck: 0 });
        let mut expected = 0.0f32;
        let mut old_half_beat = 0.0f32;
        for frame in 0..26000 {
            let sample = mixer.frame().0[0].abs();
            if (24999..=25001).contains(&frame) {
                expected = expected.max(sample);
            }
            if (12999..=13001).contains(&frame) {
                old_half_beat = old_half_beat.max(sample);
            }
        }
        assert!(expected > 0.05);
        assert!(old_half_beat < 1e-5);
        assert!(EngineCommand::SetFxBeat {
            deck: 0,
            beats: f64::NAN
        }
        .validate()
        .is_err());
    }
}

#[cfg(test)]
mod active_loop_tests {
    use super::*;
    #[test]
    fn exiting_then_shift_reactivating_restores_bounds_and_load_clears_them() {
        let mut mixer = Mixer::new();
        let source = crate::decode::demo_track(0);
        mixer.load(0, source.clone());
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 4.37,
        });
        mixer.command(EngineCommand::SetLoop {
            deck: 0,
            beats: Some(8.0),
        });
        let bounds = mixer.decks[0].loop_region.unwrap();
        mixer.command(EngineCommand::SetLoop {
            deck: 0,
            beats: None,
        });
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 1.25,
        });
        let position = mixer.decks[0].position;
        mixer.command(EngineCommand::ToggleActiveLoop { deck: 0 });
        assert_eq!(mixer.decks[0].loop_region, Some(bounds));
        assert_eq!(mixer.decks[0].position, position);
        mixer.command(EngineCommand::ToggleActiveLoop { deck: 0 });
        assert!(mixer.decks[0].loop_region.is_none());
        mixer.instant_double(1, source.clone(), 3);
        mixer.command(EngineCommand::ToggleActiveLoop { deck: 1 });
        assert_eq!(mixer.decks[1].loop_region, Some(bounds));
        mixer.load(0, source);
        mixer.command(EngineCommand::ToggleActiveLoop { deck: 0 });
        assert!(mixer.decks[0].loop_region.is_none());
        mixer.unload(1);
        mixer.command(EngineCommand::ToggleActiveLoop { deck: 1 });
        assert!(mixer.decks[1].loop_region.is_none());
    }
    #[test]
    fn manual_loop_completion_and_boundary_edits_update_remembered_bounds() {
        let mut mixer = Mixer::new();
        mixer.load(0, crate::decode::sample_track(0));
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 0.1,
        });
        mixer.command(EngineCommand::LoopIn { deck: 0 });
        mixer.command(EngineCommand::Seek {
            deck: 0,
            seconds: 0.7,
        });
        mixer.command(EngineCommand::LoopOut { deck: 0 });
        mixer.command(EngineCommand::SetLoopEdit {
            deck: 0,
            boundary: Some(LoopBoundary::Out),
        });
        mixer.command(EngineCommand::Jog {
            deck: 0,
            delta_seconds: 0.1,
            scratch: true,
        });
        let bounds = mixer.decks[0].loop_region.unwrap();
        assert!((bounds.0 / 48000.0 - 0.1).abs() < 1e-9);
        assert!((bounds.1 / 48000.0 - 0.8).abs() < 1e-9);
        mixer.command(EngineCommand::ToggleActiveLoop { deck: 0 });
        mixer.command(EngineCommand::ToggleActiveLoop { deck: 0 });
        assert_eq!(mixer.decks[0].loop_region, Some(bounds));
        assert_eq!(mixer.decks[0].loop_edit, None);
    }
}

#[cfg(test)]
mod quantized_launch_tests {
    use super::*;
    fn track(bpm: f64, offset: Option<f64>) -> Arc<TrackData> {
        let mut track = crate::decode::demo_track(0);
        let data = Arc::get_mut(&mut track).unwrap();
        data.samples = vec![[0.2; 2]; 32_000];
        data.sample_rate = 8000;
        data.analysis.duration_seconds = 4.0;
        data.analysis.sample_rate = 8000;
        data.analysis.bpm = Some(bpm);
        data.analysis.beatgrid_offset_seconds = offset;
        data.analysis.beatgrid_source = offset.map(|_| "analysis".into());
        track
    }
    fn mixer() -> Mixer {
        let mut m = Mixer::new();
        m.load(0, track(125.0, Some(0.173)));
        m.load(1, track(128.0, Some(0.211)));
        m.command(EngineCommand::SetTempo {
            deck: 1,
            percent: (125.0 / 128.0 - 1.0) * 100.0,
        });
        m.command(EngineCommand::Play { deck: 0 });
        m
    }
    fn arm(m: &mut Mixer, at_seconds: Option<f64>) {
        m.command(EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: m.load_revisions[0],
            at_seconds,
            quantize: 1.0,
        });
        assert!(m.launches[1].is_some());
    }
    #[test]
    fn launch_uses_sample_clock_with_distinct_offsets_and_tempos() {
        for output_rate in [44_100, 48_000, 96_000] {
            let mut m = mixer();
            m.sample_rate = output_rate;
            let at = 0.173 + 3.0 * 60.0 / 125.0;
            arm(&mut m, Some(at));
            while !m.decks[1].playing {
                assert!(m.decks[1].seconds() == 0.211);
                m.frame();
                assert!(m.decks[0].seconds() < at + 0.001);
            }
            assert!(m.launches[1].is_none());
            let phases = m
                .decks
                .each_ref()
                .map(|d| (d.seconds() - d.grid_offset) * d.bpm.unwrap() / 60.0);
            let error = (phases[1] - phases[0] + 0.5).rem_euclid(1.0) - 0.5;
            assert!(
                error.abs() < 125.0 / 60.0 / output_rate as f64 * 1.01,
                "{error}"
            );
        }
    }
    #[test]
    fn next_beat_is_resolved_natively() {
        let mut m = mixer();
        arm(&mut m, None);
        assert!((m.launches[1].unwrap().at_seconds - 0.173).abs() < 1e-12);
    }
    #[test]
    fn manual_changes_cancel_without_starting_incoming_audio() {
        for command in [
            EngineCommand::Seek {
                deck: 0,
                seconds: 0.3,
            },
            EngineCommand::Pause { deck: 0 },
            EngineCommand::Seek {
                deck: 1,
                seconds: 0.4,
            },
            EngineCommand::SetTempo {
                deck: 0,
                percent: 1.0,
            },
            EngineCommand::SetBeatgrid {
                anchor_index: None,
                deck: 0,
                offset_seconds: 0.2,
            },
            EngineCommand::SetLoop {
                deck: 0,
                beats: Some(4.0),
            },
            EngineCommand::CancelLaunch { deck: 1 },
        ] {
            let mut m = mixer();
            arm(&mut m, Some(1.613));
            m.command(command);
            m.frame();
            assert!(m.launches[1].is_none());
            assert!(!m.decks[1].playing);
        }
    }
    #[test]
    fn source_replacement_cannot_launch_old_or_new_track() {
        for deck in [0, 1] {
            let mut m = mixer();
            arm(&mut m, Some(1.613));
            m.load(deck, track(125.0, Some(0.1)));
            m.load_revisions[deck] += 1;
            m.frame();
            assert!(m.launches[1].is_none());
            assert!(!m.decks[1].playing);
        }
    }
    #[test]
    fn late_or_unknown_grid_launch_does_not_fall_back_to_immediate_play() {
        let mut m = mixer();
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: 2.0,
        });
        m.command(EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: 0,
            at_seconds: Some(1.613),
            quantize: 1.0,
        });
        assert!(m.launches[1].is_none());
        m.load(1, track(125.0, None));
        m.command(EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: 0,
            at_seconds: None,
            quantize: 1.0,
        });
        assert!(m.launches[1].is_none());
        assert!(!m.decks[1].playing);
    }
    #[test]
    fn grid_realign_preserves_transport_and_cues_and_estimate_is_not_bar_one() {
        let mut m = mixer();
        assert!(m.decks[0].grid_valid);
        assert!(!m.decks[0].grid_downbeat);
        m.command(EngineCommand::SetCuePoint {
            deck: 0,
            seconds: 0.5,
        });
        m.command(EngineCommand::SetHotCuePoint {
            deck: 0,
            index: 2,
            seconds: 1.0,
        });
        m.frame();
        let before = (
            m.decks[0].position,
            m.decks[0].position_revision,
            m.decks[0].cue,
            m.decks[0].hot_cues,
        );
        m.command(EngineCommand::SetBeatgrid {
            anchor_index: None,
            deck: 0,
            offset_seconds: 0.297,
        });
        assert_eq!(
            before,
            (
                m.decks[0].position,
                m.decks[0].position_revision,
                m.decks[0].cue,
                m.decks[0].hot_cues
            )
        );
        assert!(m.decks[0].playing && m.decks[0].grid_valid && m.decks[0].grid_downbeat);
        assert_eq!(m.decks[0].grid_offset, 0.297);
    }
}

#[cfg(test)]
mod neural_grid_transport_tests {
    use super::*;
    fn rhythm(shift: f64) -> audio_analysis::NeuralRhythm {
        audio_analysis::NeuralRhythm {
            beats_seconds: vec![0.2, 0.7, 1.3, 1.9, 2.6, 3.2]
                .into_iter()
                .map(|x| x + shift)
                .collect(),
            downbeats_seconds: vec![0.2 + shift, 2.6 + shift],
            bpm: 120.0,
            beat_confidence: 0.8,
            downbeat_confidence: 0.7,
            model: "fixture".into(),
            analysis_seconds: 0.0,
            bar_evidence: Vec::new(),
            beats_per_bar: None,
        }
    }
    fn track(shift: f64) -> Arc<TrackData> {
        let mut t = crate::decode::demo_track(0);
        let d = Arc::get_mut(&mut t).unwrap();
        d.analysis.neural_rhythm = Some(rhythm(shift));
        d.analysis.bpm = Some(120.0);
        d.analysis.beatgrid_source = Some("beat_this".into());
        t
    }
    #[test]
    fn continuous_sync_tracks_local_nonuniform_slopes_without_phase_drift() {
        let mut m = Mixer::new();
        for deck in 0..2 {
            let mut t = track(0.0);
            let data = Arc::get_mut(&mut t).unwrap();
            let mut beats = vec![0.2];
            for i in 0..60 {
                let interval = if deck == 0 {
                    0.5
                } else {
                    [0.48, 0.5, 0.52, 0.48, 0.54, 0.46][i % 6]
                };
                beats.push(beats.last().unwrap() + interval);
            }
            let grid = data.analysis.neural_rhythm.as_mut().unwrap();
            grid.downbeats_seconds = beats.iter().step_by(4).copied().collect();
            grid.beats_seconds = beats;
            m.load(deck, t);
            m.command(EngineCommand::Seek { deck, seconds: 0.2 });
            m.command(EngineCommand::Play { deck });
        }
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        let mut worst = 0.0_f64;
        for _ in 0..12 * 48_000 {
            m.frame();
            let a = m.decks[0].grid_position(m.decks[0].seconds());
            let b = m.decks[1].grid_position(m.decks[1].seconds());
            worst = worst.max(((b - a + 0.5).rem_euclid(1.0) - 0.5).abs());
        }
        assert!(worst < 0.02, "phase drift {worst}");
    }
    #[test]
    fn initial_sync_matches_local_period_even_when_summary_tempos_match() {
        let mut m = Mixer::new();
        for deck in 0..2 {
            let mut t = track(0.0);
            let g = Arc::get_mut(&mut t)
                .unwrap()
                .analysis
                .neural_rhythm
                .as_mut()
                .unwrap();
            g.bpm = 120.0;
            let period = if deck == 0 { 0.5 } else { 0.75 };
            g.beats_seconds = (0..40).map(|i| 0.2 + i as f64 * period).collect();
            g.downbeats_seconds = g.beats_seconds.iter().step_by(4).copied().collect();
            m.load(deck, t);
            m.command(EngineCommand::Seek { deck, seconds: 0.2 });
        }
        m.command(EngineCommand::Sync { deck: 1 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        for _ in 0..128 {
            m.frame();
        }
        assert!((m.decks[1].tempo - 50.0).abs() < 1e-8);
        assert!(
            (m.decks[1].grid_period() / m.decks[1].rate() - m.decks[0].grid_period()).abs() < 1e-8
        );
    }
    #[test]
    fn paused_sync_preserves_cue_and_queued_launch_with_different_local_tempo() {
        let mut m = Mixer::new();
        for deck in 0..2 {
            let mut t = track(0.0);
            let g = Arc::get_mut(&mut t)
                .unwrap()
                .analysis
                .neural_rhythm
                .as_mut()
                .unwrap();
            let period = if deck == 0 { 0.5 } else { 0.75 };
            g.bpm = 120.0;
            g.beats_seconds = (0..40).map(|i| 0.2 + i as f64 * period).collect();
            g.downbeats_seconds = g.beats_seconds.iter().step_by(4).copied().collect();
            m.load(deck, t);
            m.command(EngineCommand::Seek { deck, seconds: 0.2 });
        }
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: 0.45,
        });
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        assert!((m.decks[1].seconds() - 0.2).abs() < 1e-10);
        m.command(EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: 0,
            at_seconds: Some(1.2),
            quantize: 1.0,
        });
        assert!(m.launches[1].is_some());
        for _ in 0..40_000 {
            m.frame();
        }
        assert!(m.decks[1].playing && m.launches[1].is_none());
        assert!((m.decks[1].tempo - 50.0).abs() < 1e-8);
    }
    #[test]
    fn reanalysis_rebinds_manual_anchor_when_decoder_changes_beat_count() {
        let mut m = Mixer::new();
        m.load(0, track(0.0));
        m.command(EngineCommand::SetBeatgrid {
            deck: 0,
            offset_seconds: 1.4,
            anchor_index: Some(2),
        });
        let mut next = rhythm(0.0);
        next.beats_seconds.insert(1, 0.45);
        m.update_grid(0, Arc::new(next));
        assert_eq!(m.decks[0].grid_anchor_index, 3);
        assert_eq!(m.decks[0].grid_offset, 1.4);
        assert!((m.decks[0].grid_time(2.0) - 2.7).abs() < 1e-8);
    }
    #[test]
    fn manual_anchor_survives_explicit_model_half_phase_replacement() {
        let mut m = Mixer::new();
        m.load(0, track(0.0));
        m.command(EngineCommand::SetBeatgrid {
            deck: 0,
            offset_seconds: 0.8,
            anchor_index: Some(2),
        });
        let mut next = rhythm(0.0);
        for t in &mut next.beats_seconds {
            *t -= 0.5;
        }
        for t in &mut next.downbeats_seconds {
            *t -= 0.5;
        }
        next.bar_evidence.push(audio_analysis::BarEvidence {
            beats_per_bar: 4,
            half_phase_corrected: true,
            pulse_repairs: vec![],
            recurrence: 0.9,
            strong_windows: 3,
            windows: 3,
        });
        m.update_grid(0, Arc::new(next));
        assert_eq!(m.decks[0].grid_offset, 0.8);
        assert!((m.decks[0].grid_time(0.0) - 0.8).abs() < 1e-8);
        assert_eq!(m.decks[0].grid_anchor_index, 2);
    }
    #[test]
    fn manual_double_tempo_has_a_complete_uniform_grid_without_truncating_source() {
        let mut m = Mixer::new();
        m.load(0, track(0.0));
        m.command(EngineCommand::SetBpm {
            deck: 0,
            bpm: 240.0,
        });
        assert!(m.decks[0].grid_uniform);
        assert_eq!(m.decks[0].grid_bpm(), Some(240.0));
        assert!(!m.decks[0].grid_downbeat);
        m.command(EngineCommand::SetBeatgrid {
            deck: 0,
            offset_seconds: 0.2,
            anchor_index: None,
        });
        assert!((m.decks[0].grid_time(80.0) - 20.2).abs() < 1e-8);
        assert!((m.decks[0].snapped_grid_time(20.21) - 20.2).abs() < 1e-8);
        m.update_grid(0, Arc::new(rhythm(0.01)));
        assert!(m.decks[0].grid_uniform);
        assert_eq!(m.decks[0].grid_bpm(), Some(240.0));
    }
    #[test]
    fn loop_jump_and_manual_origin_use_actual_nonuniform_events() {
        let mut m = Mixer::new();
        m.load(0, track(0.0));
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: 1.3,
        });
        m.command(EngineCommand::BeatJump {
            deck: 0,
            beats: 2.0,
        });
        assert!((m.decks[0].seconds() - 2.6).abs() < 1e-8);
        m.command(EngineCommand::SetLoop {
            deck: 0,
            beats: Some(1.0),
        });
        let (a, b) = m.decks[0].loop_region.unwrap();
        assert!((a / m.decks[0].sample_rate() - 2.6).abs() < 1e-8);
        assert!((b / m.decks[0].sample_rate() - 3.2).abs() < 1e-8);
        m.command(EngineCommand::SetBeatgrid {
            deck: 0,
            offset_seconds: 1.4,
            anchor_index: Some(2),
        });
        assert!((m.decks[0].grid_time(2.0) - 2.7).abs() < 1e-8);
        let old_pos = m.decks[0].seconds();
        m.update_grid(0, Arc::new(rhythm(0.01)));
        assert!(m.decks[0].grid_manual);
        assert_eq!(m.decks[0].grid_anchor_index, 2);
        assert_eq!(m.decks[0].grid_offset, 1.4);
        assert_eq!(m.decks[0].seconds(), old_pos);
        assert!((m.decks[0].grid_time(2.0) - 2.7).abs() < 1e-8);
    }
    #[test]
    fn scheduled_launch_follows_detected_clock_event_and_snaps_incoming_event() {
        let mut m = Mixer::new();
        m.load(0, track(0.0));
        m.load(1, track(0.1));
        m.command(EngineCommand::Seek {
            deck: 1,
            seconds: 1.35,
        });
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: 0,
            at_seconds: Some(1.3),
            quantize: 1.0,
        });
        assert!(m.launches[1].is_some());
        assert!((m.decks[1].seconds() - 1.4).abs() < 1e-8);
        while !m.decks[1].playing {
            m.frame();
            assert!(m.decks[0].seconds() < 1.301);
        }
        assert!((m.decks[0].grid_position(m.decks[0].seconds()) - 2.0).abs() < 0.0001);
        assert!((m.decks[1].grid_position(m.decks[1].seconds()) - 2.0).abs() < 0.0001);
    }
}

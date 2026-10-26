use crate::{
    decode::TrackData,
    model::{ControlTarget, EngineCommand, LoopBoundary},
};
use std::sync::Arc;

pub(crate) const CONTROL_SMOOTH_SECONDS: f64 = 0.008;
pub(crate) const CONTROL_ENDPOINT_TOLERANCE: f32 = 0.002;

#[derive(Clone, Copy)]
pub(crate) struct Smooth {
    pub target: f32,
    pub(crate) value: f32,
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
    pub(crate) processor: crate::processor::Processor,
    pub track: Option<Arc<TrackData>>,
    pub grid: Option<Arc<audio_analysis::NeuralRhythm>>,
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
    pub normalization_enabled: bool,
    pub normalization_db: f32,
    normalization: Smooth,
    pub channel_fader: Smooth,
    pub sync_enabled: bool,
    pub sync_rate_limits: Option<crate::SyncRateLimits>,
    pub sync_rate_limited: bool,
    cue_preview: bool,
    pub eq: [Smooth; 3],
    pub filter: Smooth,
    pub cue_enabled: bool,
    pub echo: Smooth,
    pub reverb: Smooth,
    pub bpm: Option<f64>,
    pub analysis_pending: bool,
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
    sync_tempo_offset: f64,
    playback_target_bpm: Option<f64>,
}
impl Deck {
    fn new() -> Self {
        Self {
            processor: crate::processor::Processor::new(),
            track: None,
            analysis_pending: false,
            grid: None,
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
            normalization_enabled: true,
            normalization_db: 0.,
            normalization: Smooth::new(1.),
            channel_fader: Smooth::new(1.0),
            sync_enabled: false,
            sync_rate_limits: None,
            sync_rate_limited: false,
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
            sync_tempo_offset: 0.0,
            playback_target_bpm: None,
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
    fn natural_eof(&self) -> bool {
        self.track.is_some() && self.position >= self.duration_frames().max(1.) - 1.
    }
    pub fn seconds(&self) -> f64 {
        self.position / self.sample_rate()
    }
    pub fn transport_rate(&self) -> f64 {
        self.rate() + self.sync_tempo_offset
    }
    pub fn actual_rate(&self) -> f64 {
        self.transport_rate() + self.bend + self.sync_correction
    }
    fn release_sync_preserving_rate(&mut self) {
        self.tempo = ((self.transport_rate() + self.sync_correction - 1.) * 100.).clamp(-90., 100.);
        self.sync_tempo_offset = 0.;
        self.sync_correction = 0.;
        self.sync_enabled = false;
        self.sync_rate_limits = None;
        self.sync_rate_limited = false;
        self.playback_target_bpm = self.local_grid_bpm().map(|bpm| bpm * self.rate());
    }
    fn set_explicit_tempo(&mut self, percent: f64) {
        self.sync_enabled = false;
        self.sync_rate_limits = None;
        self.sync_rate_limited = false;
        self.sync_correction = 0.0;
        self.sync_tempo_offset = 0.;
        self.tempo = percent;
        self.playback_target_bpm = self.local_grid_bpm().map(|bpm| bpm * self.rate());
    }
    fn next_sync_rate(&mut self, desired: f64, smoothing: f64, output_rate: u32) -> f64 {
        let current = self.transport_rate();
        self.sync_rate_limited = self
            .sync_rate_limits
            .is_some_and(|limits| !limits.contains(desired));
        // An implausible local pulse must not retune the audible source.
        if self.sync_rate_limited {
            return current;
        }
        let next = current + (desired - current) * smoothing;
        self.sync_rate_limits.map_or(next, |limits| {
            if !self.playing {
                return next.clamp(limits.min_rate, limits.max_rate);
            }
            let step = limits.max_slew_per_second / output_rate.max(1) as f64;
            next.clamp(current - step, current + step)
                .clamp(limits.min_rate, limits.max_rate)
        })
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
    /// A four-beat fit to already cached events, shared by absolute BPM,
    /// Sync and telemetry. Smoothing belongs to the adaptive transport state.
    fn local_grid_period(&self) -> f64 {
        if let Some(g) = self.neural_grid() {
            let anchor = self.grid_anchor_index.min(g.beats_seconds.len() - 1);
            let original =
                g.beats_seconds[anchor] + (self.seconds() - self.grid_offset) / self.grid_scale;
            let left = crate::grid::nearest_index(&g.beats_seconds, original)
                .min(g.beats_seconds.len() - 2);
            let span = 4.min(g.beats_seconds.len() - 1 - left);
            (g.beats_seconds[left + span] - g.beats_seconds[left]) / span as f64 * self.grid_scale
        } else {
            60. / self.bpm.unwrap_or(120.)
        }
    }
    pub fn local_grid_bpm(&self) -> Option<f64> {
        self.grid_bpm()?;
        let period = self.local_grid_period();
        (period.is_finite() && period > 0.).then_some(60. / period)
    }
    pub fn target_playback_bpm(&self) -> Option<f64> {
        self.playback_target_bpm
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
        // The bounded processor is already configured; RAM pre-roll aligns the
        // first output at starts/seeks. No full-track asynchronous cache exists.
        self.analysis_pending
    }
    pub fn fade_in(&mut self) {
        self.attack = 0.0;
    }
    #[inline]
    fn render(&mut self, output_rate: u32) -> [f32; 2] {
        if self.analysis_pending {
            return [0.0; 2];
        }
        if self.output_rate != output_rate {
            self.processor.invalidate();
            self.output_rate = output_rate;
            self.smoothing =
                1.0 - (-1.0 / (output_rate as f32 * CONTROL_SMOOTH_SECONDS as f32)).exp();
            self.low_coefficient =
                1.0 - (-2.0 * std::f32::consts::PI * 250.0 / output_rate as f32).exp();
            self.high_coefficient =
                1.0 - (-2.0 * std::f32::consts::PI * 2500.0 / output_rate as f32).exp();
            self.filter_counter = 0;
        }
        let smoothing = self.smoothing;
        let mut source = [0.0; 2];
        if let Some(original) = self.track.as_ref() {
            let track = original;
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
                if self.position >= 0.0 && self.position < (length.saturating_sub(1)) as f64 {
                    let speed = if self.scratch_remaining > 0 {
                        self.scratch_speed
                    } else {
                        self.actual_rate()
                    };
                    let scratching = self.scratch_remaining > 0;
                    source = self.processor.render(
                        &track.samples,
                        self.position,
                        track.sample_rate,
                        output_rate,
                        speed,
                        if scratching { 0. } else { self.pitch },
                        !scratching && self.keylock,
                        self.position_revision,
                        self.loop_region,
                    );
                    self.attack = (self.attack + 1.0 / (output_rate as f32 * 0.003)).min(1.0);
                    // A brief tail stops a stationary cursor without a hard audio edge.
                    let tail = if self.cursor_target.is_some() {
                        (self.scratch_remaining as f32 / (output_rate as f32 * 0.002)).min(1.0)
                    } else {
                        1.0
                    };
                    source[0] *= self.attack * tail;
                    source[1] *= self.attack * tail;
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
                    // Natural EOF is continuous source-clock advancement, not a
                    // user seek. Keep its revision so owned handovers can recover
                    // on the stopped clock instead of yielding to a nonexistent edit.
                    self.position = if self.position < 0.0 {
                        0.0
                    } else {
                        length as f64
                    };
                }
            }
        }
        let gain = self.gain.tick(smoothing) * self.normalization.tick(smoothing);
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
        self.peak = output;
        output
    }
}

#[derive(Clone, Copy)]
struct ControlRamp {
    target: ControlTarget,
    from: f32,
    actual_from: Option<f32>,
    to: f32,
    start: u64,
    end: u64,
    clock: Option<usize>,
    source_revision: Option<u64>,
    target_revision: Option<u64>,
    position_revision: Option<u64>,
    tempo_reference_bpm: Option<f64>,
}
#[derive(Clone, Copy)]
pub(crate) struct ScheduledLaunch {
    pub(crate) clock: usize,
    pub at_seconds: f64,
    revisions: [u64; 2],
    positions: [u64; 2],
    rates: [f64; 2],
    bpms: [Option<f64>; 2],
    grids: [f64; 2],
}
pub(crate) struct Mixer {
    pub master_protection: crate::protection::Protection,
    pub cue_protection: crate::protection::Protection,
    pub quality: crate::AudioQualitySnapshot,
    pub applied_fence: u64,
    pub grid_revisions: [u64; 2],
    pub control_revision: u64,
    pub guarded_control_receipts: [u64; 64],
    pub direct_control_receipts: [Option<crate::DirectControlReceipt>; 64],
    pub deck_control_revisions: [u64; 2],
    pub target_control_revisions: [u64; crate::COMMAND_TARGET_COUNT],
    pub engine_epoch: u64,
    pub scheduled: Option<Box<crate::scheduled::NativeBatch>>,
    batch_internal: bool,
    pub load_revisions: [u64; 2],
    ramps: [Option<ControlRamp>; crate::CONTROL_TARGET_COUNT],
    ramp_boundaries: [Option<(u64, f32, bool)>; crate::CONTROL_TARGET_COUNT],
    pub launches: [Option<ScheduledLaunch>; 2],
    pub decks: [Deck; 2],
    pub main_fx: [crate::effects::EffectBus; 2],
    cue_fx: [crate::effects::EffectBus; 2],
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
    sync_tempo_smoothing: f64,
    smoothing_rate: u32,
    fade_value: f32,
    fade_gains: [f32; 2],
}
impl Mixer {
    pub fn new() -> Self {
        Self {
            master_protection: crate::protection::Protection::new(),
            cue_protection: crate::protection::Protection::new(),
            quality: crate::AudioQualitySnapshot::default(),
            applied_fence: 0,
            grid_revisions: [0; 2],
            control_revision: 0,
            guarded_control_receipts: [0; 64],
            direct_control_receipts: [None; 64],
            deck_control_revisions: [0; 2],
            target_control_revisions: [0; crate::COMMAND_TARGET_COUNT],
            engine_epoch: 0,
            scheduled: None,
            batch_internal: false,
            load_revisions: [0; 2],
            ramps: [None; crate::CONTROL_TARGET_COUNT],
            ramp_boundaries: [None; crate::CONTROL_TARGET_COUNT],
            launches: [None; 2],
            decks: [Deck::new(), Deck::new()],
            main_fx: std::array::from_fn(|_| crate::effects::EffectBus::new()),
            cue_fx: std::array::from_fn(|_| crate::effects::EffectBus::new()),
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
            sync_tempo_smoothing: 0.0,
            smoothing_rate: 0,
            fade_value: f32::NAN,
            fade_gains: [0.70710677; 2],
        }
    }
    fn promote_playing_follower_before_source_change(&mut self, deck: usize) {
        let other = 1 - deck;
        let follower = &self.decks[other];
        if deck == self.master_deck
            && follower.playing
            && follower.grid_valid
            && !follower.jog_touch
            && !follower.natural_eof()
            && follower.local_grid_bpm().is_some()
        {
            self.decks[other].release_sync_preserving_rate();
            self.master_deck = other;
            self.launches.fill(None);
        }
    }
    pub fn load(&mut self, deck: usize, track: Arc<TrackData>) -> Option<Arc<TrackData>> {
        self.promote_playing_follower_before_source_change(deck);
        self.grid_revisions[deck] = self.grid_revisions[deck].wrapping_add(1);
        self.launches.fill(None);
        let d = &mut self.decks[deck];
        let previous = d.track.replace(track);
        d.processor.invalidate();
        d.normalization_db = d
            .track
            .as_ref()
            .and_then(|t| t.analysis.audio_measurements.as_ref())
            .map_or(0., |m| m.normalization_gain_db.clamp(-60., 12.) as f32);
        d.normalization = Smooth::new(if d.normalization_enabled {
            10_f32.powf(d.normalization_db / 20.)
        } else {
            1.
        });
        d.position = 0.0;
        d.position_revision = d.position_revision.wrapping_add(1);
        d.playing = false;
        d.cue = 0.0;
        d.hot_cues = [None; 8];
        d.loop_region = None;
        d.last_loop_region = None;
        d.loop_edit = None;
        d.tempo = 0.0;
        d.sync_enabled = false;
        d.sync_rate_limits = None;
        d.sync_rate_limited = false;
        d.sync_correction = 0.0;
        d.sync_tempo_offset = 0.;
        d.bend = 0.0;
        d.scratch_remaining = 0;
        d.cursor_target = None;
        d.cursor_release_pending = false;
        d.attack = 0.0;
        d.pitch = 0.0;
        d.keylock = false;
        d.loop_in = None;
        d.jog_touch = false;
        d.memory_cues = [None; 10];
        d.band_low = [0.0; 2];
        d.band_high = [0.0; 2];
        d.filter_low = [[0.0; 2]; 2];
        d.filter_high = [[0.0; 2]; 2];
        d.analysis_pending = d.track.as_ref().is_some_and(|t| t.analysis_pending);
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
        d.playback_target_bpm = d.grid_bpm();
        previous
    }
    /// Adopt derived metadata without taking ownership of manual tempo/grid
    /// edits or changing transport when a neural grid is unavailable.
    pub fn update_base_grid(
        &mut self,
        deck: usize,
        bpm: Option<f64>,
        offset: Option<f64>,
        valid: bool,
    ) {
        let d = &mut self.decks[deck];
        if !d.grid_uniform {
            d.bpm = bpm;
        }
        if !d.grid_manual {
            d.grid_offset = offset.filter(|x| x.is_finite()).unwrap_or(0.0);
            d.grid_valid = valid && offset.is_some_and(f64::is_finite);
        }
    }
    pub fn update_grid(
        &mut self,
        deck: usize,
        grid: Arc<audio_analysis::NeuralRhythm>,
    ) -> Option<Arc<audio_analysis::NeuralRhythm>> {
        // Re-publishing identical timing must not cancel a valid armed handover.
        if self.decks[deck].grid.as_ref().is_some_and(|old| {
            old.beats_seconds == grid.beats_seconds
                && old.downbeats_seconds == grid.downbeats_seconds
                && old.bpm == grid.bpm
                && old.beats_per_bar == grid.beats_per_bar
                && old.bar_evidence.iter().any(|e| e.half_phase_corrected)
                    == grid.bar_evidence.iter().any(|e| e.half_phase_corrected)
        }) {
            return self.decks[deck].grid.replace(grid);
        }
        self.grid_revisions[deck] = self.grid_revisions[deck].wrapping_add(1);
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
    ) -> [Option<Arc<TrackData>>; 2] {
        let source = &self.decks[1 - deck];
        let grid = source.grid.clone();
        let analysis_pending = source.analysis_pending;
        let uniform = source.grid_uniform;
        let normalization_enabled = source.normalization_enabled;
        let state = (
            source.position,
            source.playing,
            ((source.transport_rate() + source.sync_correction - 1.) * 100.).clamp(-90., 100.),
            source.cue,
            source.hot_cues,
            source.memory_cues,
            source.loop_region,
            source.loop_in,
            source.pitch,
            source.keylock,
            source.bpm,
            source.grid_offset,
            source.last_loop_region,
            source.grid_valid,
            source.grid_downbeat,
            source.grid_manual,
            source.grid_anchor_index,
            source.grid_scale,
            source.bend,
            source.playback_target_bpm,
        );
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
        d.last_loop_region = state.12;
        d.grid_valid = state.13;
        d.grid_downbeat = state.14;
        d.grid_manual = state.15;
        d.grid_anchor_index = state.16;
        d.grid_scale = state.17;
        d.bend = state.18;
        d.playback_target_bpm = state.19;
        d.grid = grid;
        d.grid_uniform = uniform;
        d.analysis_pending = analysis_pending;
        d.normalization_enabled = normalization_enabled;
        d.normalization = Smooth::new(if normalization_enabled {
            10_f32.powf(d.normalization_db / 20.)
        } else {
            1.
        });
        d.sync_enabled = false;
        [old_track, None]
    }
    pub fn unload(&mut self, deck: usize) -> [Option<Arc<TrackData>>; 2] {
        self.promote_playing_follower_before_source_change(deck);
        self.grid_revisions[deck] = self.grid_revisions[deck].wrapping_add(1);
        self.launches.fill(None);
        let d = &mut self.decks[deck];
        d.playing = false;
        d.analysis_pending = false;
        d.sync_enabled = false;
        d.sync_rate_limits = None;
        d.sync_rate_limited = false;
        d.sync_tempo_offset = 0.;
        d.sync_correction = 0.;
        d.playback_target_bpm = None;
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
        [d.track.take(), None]
    }
    pub fn command(&mut self, command: EngineCommand) {
        if crate::analysis_gated_command(&command)
            && command
                .deck()
                .is_some_and(|d| self.decks[d].analysis_pending)
        {
            return;
        }
        match &command {
            EngineCommand::SetPlaybackBpm { deck, bpm }
                if crate::playback_tempo_percent(self.decks[*deck].local_grid_bpm(), *bpm)
                    .is_err() =>
            {
                return
            }
            EngineCommand::Sync { deck }
            | EngineCommand::SetSync {
                deck,
                enabled: true,
            } if *deck == self.master_deck
                || self.decks[*deck].playing
                    && (!self.decks[self.master_deck].playing
                        || self.decks[self.master_deck].jog_touch) =>
            {
                return
            }
            _ => {}
        }
        if !self.batch_internal {
            // A listening change only takes over a queue that owns its target.
            let listening_only = matches!(
                &command,
                EngineCommand::SetMasterGain { .. }
                    | EngineCommand::SetCueGain { .. }
                    | EngineCommand::SetCueMix { .. }
                    | EngineCommand::SetSamplerGain { .. }
                    | EngineCommand::SetHeadphoneCue { .. }
            );
            self.manual_batch_takeover(
                listening_only,
                crate::scheduled::command_targets(&command),
                crate::scheduled::command_decks(&command),
            );
        }
        let grid_before = match &command {
            EngineCommand::SetBpm { deck, .. } | EngineCommand::SetBeatgrid { deck, .. } => {
                let d = &self.decks[*deck];
                Some((
                    *deck,
                    d.bpm,
                    d.grid_offset,
                    d.grid_anchor_index,
                    d.grid_uniform,
                    d.grid_scale,
                ))
            }
            _ => None,
        };

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
                | EngineCommand::SetPlaybackBpm { .. }
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
            SetTempo { deck, .. } | SetPlaybackBpm { deck, .. } => {
                self.ramps[ControlTarget::TempoPercent { deck: *deck }.index()] = None
            }
            SetFx { deck, .. } => {
                self.ramps[ControlTarget::Echo { deck: *deck }.index()] = None;
                self.ramps[ControlTarget::Reverb { deck: *deck }.index()] = None;
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
                    delay = ((d.grid_time(start) - d.seconds()) / d.transport_rate()).max(0.0);
                    if let Some(beats) = beats {
                        duration =
                            (d.grid_time(start + beats) - d.grid_time(start)) / d.transport_rate();
                    }
                }
                if let ControlTarget::TempoPercent { deck } = target {
                    self.decks[deck].release_sync_preserving_rate();
                }
                let from = self.target_value(target);
                let start = self.rendered_frames + (delay * self.sample_rate as f64).round() as u64;
                let frames = (duration * self.sample_rate as f64).round().max(1.0) as u64;
                let guard = clock.or(target.deck());
                self.ramps[target.index()] = Some(ControlRamp {
                    target,
                    from,
                    actual_from: None,
                    to: value,
                    start,
                    end: start + frames,
                    clock,
                    source_revision: guard.map(|d| self.load_revisions[d]),
                    target_revision: target.deck().map(|d| self.load_revisions[d]),
                    position_revision: clock.map(|d| self.decks[d].position_revision),
                    tempo_reference_bpm: match target {
                        ControlTarget::TempoPercent { deck } => self.decks[deck].local_grid_bpm(),
                        _ => None,
                    },
                });
                self.ramp_boundaries[target.index()] = None;
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
                let beat = a.grid_position(a.seconds());
                let at = at_seconds
                    .unwrap_or(a.grid_time((beat / quantize).floor().mul_add(quantize, quantize)));
                // Owned scheduled actions run on the first renderer sample at
                // or after the selected event. Permit only that bounded sample
                // crossing; ordinary late standalone launches still reject.
                let sample_slack = if self.batch_internal {
                    2. * a.transport_rate() / self.sample_rate as f64
                } else {
                    0.
                };
                if !at.is_finite()
                    || at + sample_slack < a.seconds()
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
                for ramp in &mut self.ramps {
                    if ramp.is_some_and(|r| r.clock == Some(deck)) {
                        *ramp = None;
                    }
                }
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
            SetTempo { deck, percent } => {
                // An explicit tempo move takes ownership from continuous sync.
                // Otherwise the next rendered frame silently overwrites it.
                self.decks[deck].set_explicit_tempo(percent);
            }
            SetPlaybackBpm { deck, bpm } => {
                if let Ok(percent) =
                    crate::playback_tempo_percent(self.decks[deck].local_grid_bpm(), bpm)
                {
                    self.decks[deck].set_explicit_tempo(percent);
                    self.decks[deck].playback_target_bpm = Some(bpm);
                }
            }
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
                let leader = &self.decks[self.master_deck];
                let nominal_bpm = leader.grid_bpm().map(|bpm| bpm * leader.rate());
                let target_bpm = leader.playback_target_bpm;
                let desired_rate = (leader.transport_rate() * self.decks[deck].local_grid_period()
                    / leader.local_grid_period())
                .clamp(0.1, 2.);
                let preserved_rate =
                    self.decks[deck].transport_rate() + self.decks[deck].sync_correction;
                let bounded_rate =
                    self.decks[deck].next_sync_rate(desired_rate, 1., self.sample_rate);
                if let (Some(bpm), Some(base)) = (nominal_bpm, self.decks[deck].grid_bpm()) {
                    self.decks[deck].tempo = ((bpm / base).clamp(0.1, 2.) - 1.) * 100.;
                    let rate = if self.decks[deck].sync_rate_limits.is_some()
                        && self.decks[deck].playing
                    {
                        preserved_rate
                    } else {
                        bounded_rate
                    };
                    self.decks[deck].sync_tempo_offset = rate - self.decks[deck].rate();
                    self.decks[deck].sync_correction = 0.;
                    self.decks[deck].playback_target_bpm = target_bpm;
                }
            }
            SetSync { deck, enabled } => {
                if enabled {
                    self.command(Sync { deck });
                    self.decks[deck].sync_enabled = true;
                } else {
                    self.decks[deck].release_sync_preserving_rate();
                }
            }
            SetSyncRateLimits { deck, limits } => {
                self.decks[deck].sync_rate_limits = limits;
                self.decks[deck].sync_rate_limited = false;
            }
            SetMaster { deck } => {
                self.launches.fill(None);
                self.decks[deck].release_sync_preserving_rate();
                self.master_deck = deck;
                for d in &mut self.decks {
                    // Retain the audible rate while changing the phase reference.
                    d.sync_tempo_offset += d.sync_correction;
                    d.sync_correction = 0.;
                }
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
            SetNormalization { deck, enabled } => {
                let d = &mut self.decks[deck];
                d.normalization_enabled = enabled;
                d.normalization.target = if enabled {
                    10_f32.powf(d.normalization_db / 20.)
                } else {
                    1.
                };
            }
            SetFx { deck, echo, reverb } => {
                self.decks[deck].echo.target = echo;
                self.decks[deck].reverb.target = reverb;
            }
            SetBpm { deck, bpm } => {
                let d = &mut self.decks[deck];
                d.release_sync_preserving_rate();
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
                d.release_sync_preserving_rate();
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
        if let Some((deck, bpm, offset, anchor, uniform, scale)) = grid_before {
            let d = &self.decks[deck];
            if (
                d.bpm,
                d.grid_offset,
                d.grid_anchor_index,
                d.grid_uniform,
                d.grid_scale,
            ) != (bpm, offset, anchor, uniform, scale)
            {
                self.grid_revisions[deck] = self.grid_revisions[deck].wrapping_add(1);
                self.decks[deck].playback_target_bpm = self.decks[deck]
                    .local_grid_bpm()
                    .map(|bpm| bpm * self.decks[deck].rate());
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
            ControlTarget::Echo { deck } => &mut self.decks[deck].echo,
            ControlTarget::Reverb { deck } => &mut self.decks[deck].reverb,
            ControlTarget::TempoPercent { .. } => unreachable!("Tempo uses its deck rate directly"),
        }
    }
    fn target_value(&mut self, target: ControlTarget) -> f32 {
        if let ControlTarget::TempoPercent { deck } = target {
            self.decks[deck].tempo as f32
        } else {
            self.target_mut(target).target
        }
    }
    pub fn active_ramps(&self) -> [bool; crate::CONTROL_TARGET_COUNT] {
        self.ramps.map(|r| r.is_some())
    }
    fn advance_ramps(&mut self) {
        for i in 0..self.ramps.len() {
            let Some(mut ramp) = self.ramps[i] else {
                continue;
            };
            let guard = ramp.clock.or(ramp.target.deck());
            if guard.is_some_and(|d| Some(self.load_revisions[d]) != ramp.source_revision)
                || ramp
                    .target
                    .deck()
                    .is_some_and(|d| Some(self.load_revisions[d]) != ramp.target_revision)
                || ramp.clock.is_some_and(|d| {
                    (!self.decks[d].playing
                        && !(self.decks[d].natural_eof() && self.rendered_frames > ramp.start))
                        || Some(self.decks[d].position_revision) != ramp.position_revision
                })
            {
                self.ramps[i] = None;
                continue;
            }
            if self.rendered_frames < ramp.start {
                continue;
            }
            if ramp.actual_from.is_none() {
                ramp.actual_from = Some(match ramp.target {
                    ControlTarget::TempoPercent { .. } => self.target_value(ramp.target),
                    _ => self.target_mut(ramp.target).value,
                });
                self.ramps[i] = Some(ramp);
            }
            let progress = ((self.rendered_frames - ramp.start) as f64
                / (ramp.end - ramp.start) as f64)
                .min(1.0) as f32;
            let value = if progress >= 1.0 {
                ramp.to
            } else {
                ramp.from + (ramp.to - ramp.from) * progress
            };
            if let ControlTarget::TempoPercent { deck } = ramp.target {
                self.decks[deck].tempo = value as f64;
                self.decks[deck].playback_target_bpm = ramp
                    .tempo_reference_bpm
                    .map(|bpm| bpm * self.decks[deck].rate());
            } else {
                self.target_mut(ramp.target).target = value;
            }
            if progress >= 1.0 {
                // A curve may continue at this exact boundary before its
                // smoother settles. Verify the rendered control trajectory,
                // not merely that the requested duration elapsed. The linear
                // recurrence is x[n+1]=x[n]+alpha*(target[n]-x[n]).
                let tracking = if matches!(ramp.target, ControlTarget::TempoPercent { .. }) {
                    true
                } else {
                    let n = (ramp.end - ramp.start) as f64;
                    let alpha = (1.0
                        - (-1.0 / (self.sample_rate as f32 * CONTROL_SMOOTH_SECONDS as f32)).exp())
                        as f64;
                    let slope = (ramp.to - ramp.from) as f64 / n;
                    let expected = ramp.to as f64 - slope / alpha
                        + (ramp.actual_from.unwrap() as f64 - ramp.from as f64 + slope / alpha)
                            * (1. - alpha).powf(n);
                    (self.target_mut(ramp.target).value as f64 - expected).abs()
                        <= CONTROL_ENDPOINT_TOLERANCE as f64
                };
                self.ramp_boundaries[i] = Some((self.rendered_frames, ramp.to, tracking));
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
            self.smoothing =
                1.0 - (-1.0 / (self.sample_rate as f32 * CONTROL_SMOOTH_SECONDS as f32)).exp();
            self.sync_smoothing = 1.0 - (-1.0 / (self.sample_rate as f64 * 0.250)).exp();
            self.sync_tempo_smoothing = 1.0 - (-1.0 / self.sample_rate as f64).exp();
        }
        self.advance_batch();
        self.advance_launches();
        self.advance_ramps();
        let leader_phase =
            self.decks[self.master_deck].grid_position(self.decks[self.master_deck].seconds());
        let leader_playing =
            self.decks[self.master_deck].playing && !self.decks[self.master_deck].jog_touch;
        let leader_bpm = self.decks[self.master_deck]
            .grid_bpm()
            .map(|bpm| bpm * self.decks[self.master_deck].rate());
        let sync_smoothing = self.sync_smoothing;
        let leader_target = self.decks[self.master_deck].playback_target_bpm;
        let leader_period = self.decks[self.master_deck].local_grid_period();
        let leader_rate = self.decks[self.master_deck].transport_rate();
        for deck in 0..2 {
            if deck == self.master_deck || !self.decks[deck].sync_enabled {
                continue;
            }
            if !leader_playing
                || !self.decks[self.master_deck].grid_valid
                || !self.decks[deck].grid_valid
            {
                // A stopped/replaced master must never retune audible playback.
                self.decks[deck].sync_tempo_offset += self.decks[deck].sync_correction;
                self.decks[deck].sync_correction = 0.;
                self.decks[deck].sync_rate_limited = false;
                continue;
            }
            let desired_rate =
                (leader_rate * self.decks[deck].local_grid_period() / leader_period).clamp(0.1, 2.);
            let old_transport = self.decks[deck].transport_rate();
            let previous_sync_rate = old_transport + self.decks[deck].sync_correction;
            let smoothing = if self.decks[deck].playing {
                self.sync_tempo_smoothing
            } else {
                1.
            };
            let next_rate =
                self.decks[deck].next_sync_rate(desired_rate, smoothing, self.sample_rate);
            if let (Some(bpm), Some(base)) = (leader_bpm, self.decks[deck].grid_bpm()) {
                self.decks[deck].tempo = ((bpm / base).clamp(0.1, 2.) - 1.) * 100.;
            }
            self.decks[deck].playback_target_bpm = leader_target;
            let nominal = self.decks[deck].rate();
            self.decks[deck].sync_tempo_offset = next_rate - nominal;
            let follower = self.decks[deck].grid_position(self.decks[deck].seconds());
            let difference = (leader_phase - follower + 0.5).rem_euclid(1.0) - 0.5;
            let d = &mut self.decks[deck];
            if d.playing && !d.jog_touch && d.cursor_target.is_none() {
                // Keep musical tempo independent of detector-window jitter.
                // Modest, smoothed phase correction is explicit in telemetry.
                let target = if d.sync_rate_limited {
                    0.
                } else {
                    (difference * 0.25).clamp(-0.015, 0.015)
                };
                d.sync_correction += (target - d.sync_correction) * sync_smoothing;
                if let Some(limits) = d.sync_rate_limits {
                    let step = limits.max_slew_per_second / self.sample_rate.max(1) as f64;
                    let actual = (d.transport_rate() + d.sync_correction)
                        .clamp(previous_sync_rate - step, previous_sync_rate + step)
                        .clamp(limits.min_rate, limits.max_rate);
                    d.sync_correction = actual - d.transport_rate();
                }
            } else {
                d.sync_correction = 0.0;
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
        let dry = [left, right];
        let gains = [a * channel_left, b * channel_right];
        let mut main_wet = [[0.; 2]; 2];
        let mut cue_wet = [[0.; 2]; 2];
        for i in 0..2 {
            let d = &mut self.decks[i];
            let echo = d.echo.tick(smoothing);
            let reverb = d.reverb.tick(smoothing);
            let delay = 60. / d.bpm.unwrap_or(120.) / d.rate() * d.fx_beats;
            let restricted = d
                .track
                .as_ref()
                .is_some_and(|t| !t.analysis.recording_allowed);
            main_wet[i] = self.main_fx[i].process(
                dry[i].map(|x| x * gains[i]),
                echo,
                reverb,
                delay,
                self.sample_rate,
                restricted,
            );
            cue_wet[i] =
                self.cue_fx[i].process(dry[i], echo, reverb, delay, self.sample_rate, restricted);
        }
        let mut master = [0.0; 2];
        let mut cue = [0.0; 2];
        for channel in 0..2 {
            master[channel] = (left[channel] * gains[0]
                + right[channel] * gains[1]
                + main_wet[0][channel]
                + main_wet[1][channel])
                * master_gain;
            let selected = if self.decks[0].cue_enabled {
                left[channel] + cue_wet[0][channel]
            } else {
                0.
            } + if self.decks[1].cue_enabled {
                right[channel] + cue_wet[1][channel]
            } else {
                0.
            };
            cue[channel] = (selected * (1. - cue_mix) + master[channel] * cue_mix) * cue_gain;
        }
        let sampler_gain = self.sampler_gain.tick(smoothing);
        let mut sampler = [0.0_f32; 2];
        for slot in 0..16 {
            if let Some(position) = self.sample_positions[slot] {
                let track = &self.samples[slot];
                let index = position as usize;
                if index + 1 < track.samples.len() {
                    let step = track.sample_rate as f64 / self.sample_rate as f64;
                    let value = crate::processor::sample(&track.samples, position, step, None);
                    for (channel, sample) in sampler.iter_mut().enumerate() {
                        *sample += value[channel] * sampler_gain;
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
    fn tails_survive_fader_close_pause_and_source_replacement() {
        for (echo, reverb, probe) in [(1., 0., 24000), (0., 1., 3504)] {
            let mut m = Mixer::new();
            let mut data = Arc::try_unwrap(constant(0., 3.)).ok().unwrap();
            data.samples[1000] = [0.5; 2];
            m.load(0, Arc::new(data));
            m.command(EngineCommand::SetFx {
                deck: 0,
                echo,
                reverb,
            });
            m.command(EngineCommand::SetBpm { deck: 0, bpm: 120. });
            m.command(EngineCommand::SetFxBeat { deck: 0, beats: 1. });
            m.command(EngineCommand::Play { deck: 0 });
            for _ in 0..1500 {
                m.frame();
            }
            m.command(EngineCommand::Pause { deck: 0 });
            m.command(EngineCommand::SetChannelFader { deck: 0, value: 0. });
            m.load(0, constant(0., 3.));
            m.command(EngineCommand::SetFx {
                deck: 0,
                echo: 0.,
                reverb: 0.,
            });
            let mut peak = 0_f32;
            for frame in 1500..(probe + 1100) {
                let out = m.frame().0;
                if frame >= probe + 995 {
                    peak = peak.max(out[0].abs());
                }
            }
            assert!(
                peak > 0.01,
                "The independent return must survive deck replacement: {echo}/{reverb}: {peak}"
            );
            for _ in 0..(48000 * 9) {
                m.frame();
            }
            assert!(!m.main_fx[0].active());
            assert_eq!(m.main_fx[0].peak, [0.; 2]);
        }
    }
    #[test]
    fn cue_effects_do_not_leak_through_closed_main_fader() {
        let mut m = Mixer::new();
        m.load(0, constant(0.15, 2.));
        m.decks[0].channel_fader = Smooth::new(0.);
        m.command(EngineCommand::SetFx {
            deck: 0,
            echo: 1.,
            reverb: 1.,
        });
        m.command(EngineCommand::SetHeadphoneCue {
            deck: 0,
            enabled: true,
        });
        m.command(EngineCommand::Play { deck: 0 });
        let mut cue_peak = 0_f32;
        for _ in 0..30000 {
            let (main, cue) = m.frame();
            assert_eq!(main, [0.; 2]);
            cue_peak = cue_peak.max(cue[0].abs());
        }
        assert!(cue_peak > 0.1);
        assert!(!m.main_fx[0].active());
    }
    #[test]
    fn normalization_is_separate_from_trim_and_smoothly_bypassed() {
        let mut m = Mixer::new();
        let mut data = Arc::try_unwrap(constant(0.1, 3.)).ok().unwrap();
        let mut measured = audio_analysis::measure_audio(&data.samples, 48000);
        measured.normalization_gain_db = 6.0206;
        data.analysis.audio_measurements = Some(measured);
        m.load(0, Arc::new(data));
        m.command(EngineCommand::SetDeckGain { deck: 0, gain: 0.7 });
        m.command(EngineCommand::Play { deck: 0 });
        let mut on = 0.;
        for _ in 0..10000 {
            on = m.frame().0[0];
        }
        m.command(EngineCommand::SetNormalization {
            deck: 0,
            enabled: false,
        });
        let mut off = 0.;
        for _ in 0..10000 {
            off = m.frame().0[0];
        }
        assert!((on / off - 2.).abs() < 0.01);
        assert_eq!(m.decks[0].gain.target, 0.7);
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
            analysis_pending: false,
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
    fn incomplete_analysis_cannot_emit_audio_or_advance_before_the_gate_clears() {
        let mut source = ramp();
        Arc::get_mut(&mut source).unwrap().analysis_pending = true;
        let mut mixer = Mixer::new();
        mixer.load(0, source);
        mixer.command(EngineCommand::Play { deck: 0 });
        mixer.command(EngineCommand::SetHotCue { deck: 0, index: 0 });
        mixer.command(EngineCommand::TriggerHotCue { deck: 0, index: 0 });
        assert!(!mixer.decks[0].playing);
        // Even a stale queued launch cannot bypass the audio renderer guard.
        mixer.decks[0].playing = true;
        for _ in 0..1024 {
            assert_eq!(mixer.decks[0].render(48_000), [0.; 2]);
        }
        assert_eq!(mixer.decks[0].seconds(), 0.);
        mixer.decks[0].analysis_pending = false;
        mixer.command(EngineCommand::Play { deck: 0 });
        assert!((0..1024).any(|_| mixer.decks[0].render(48_000)[0] > 0.01));
        assert!(mixer.decks[0].seconds() > 0.);
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
        let d = &mut mixer.decks[0];
        d.keylock = true;
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
        let position = mixer.decks[0].position;
        let retired = mixer.instant_double(1, source.clone());
        assert!(retired.into_iter().all(|t| t.is_none()));
        let a = &mixer.decks[0];
        let b = &mixer.decks[1];
        assert!(Arc::ptr_eq(
            a.track.as_ref().unwrap(),
            b.track.as_ref().unwrap()
        ));
        assert_eq!(b.position, position);
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
        mixer.instant_double(1, source.clone());
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
    fn continuous_sync_filters_interval_jitter_without_accumulating_phase_drift() {
        let mut m = Mixer::new();
        for deck in 0..2 {
            let mut t = track(0.0);
            let data = Arc::get_mut(&mut t).unwrap();
            let mut beats = vec![0.2];
            for i in 0..60 {
                let interval = if deck == 0 {
                    0.5
                } else {
                    [0.48, 0.5, 0.52, 0.48, 0.54, 0.48][i % 6]
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
        let mut largest_tempo = 0.0_f64;
        let mut actual_peak = 0.0_f64;
        let mut actual_squared = 0.0_f64;
        for _ in 0..12 * 48_000 {
            m.frame();
            let a = m.decks[0].grid_position(m.decks[0].seconds());
            let b = m.decks[1].grid_position(m.decks[1].seconds());
            worst = worst.max(((b - a + 0.5).rem_euclid(1.0) - 0.5).abs());
            largest_tempo = largest_tempo.max(m.decks[1].tempo.abs());
            let error = m.decks[1].actual_rate() - 1.;
            actual_peak = actual_peak.max(error.abs());
            actual_squared += error * error;
            assert!((m.decks[1].actual_rate() - m.decks[1].rate()).abs() <= 0.03);
        }
        // The fixture's adjacent events jitter by up to 0.08 beats around a
        // steady pulse. Do not warp audio to reproduce every inferred jitter;
        // retain bounded alignment without accumulating drift or 4–8% hunts.
        assert!(worst < 0.09, "phase drift {worst}");
        assert!(
            largest_tempo < 1e-10,
            "nominal tempo hunting {largest_tempo}"
        );
        assert!(m.decks.iter().all(|d| d.playing));
        eprintln!(
            "NATIVE_METRIC {}",
            serde_json::json!({"fixture":"zero_mean_timestamp_jitter", "actual_rate_error_rms":(actual_squared / (12. * 48000.)).sqrt(), "actual_rate_error_peak":actual_peak, "nominal_slider_percent_peak":largest_tempo, "phase_error_peak_beats":worst})
        );
    }
    #[test]
    fn initial_sync_uses_stable_reference_even_when_local_periods_disagree() {
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
        assert!(m.decks[1].tempo.abs() < 1e-8);
        assert_eq!(m.decks[1].grid_bpm(), m.decks[0].grid_bpm());
    }
    #[test]
    fn paused_sync_does_not_chase_individual_model_intervals() {
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
            g.beats_seconds = (0..80)
                .map(|i| 0.2 + i as f64 * 0.5 + if deck == 0 && i % 2 == 1 { 0.02 } else { 0.0 })
                .collect();
            g.downbeats_seconds = g.beats_seconds.iter().step_by(4).copied().collect();
            m.load(deck, t);
            m.command(EngineCommand::Seek { deck, seconds: 0.2 });
        }
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        let matched = m.decks[1].tempo;
        for _ in 0..3 * 48_000 {
            m.frame();
            assert!((m.decks[1].tempo - matched).abs() < 1e-10);
            assert!((m.decks[1].seconds() - 0.2).abs() < 1e-10);
        }
        m.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 2.0,
        });
        m.frame();
        assert!((m.decks[1].rate() - 1.02).abs() < 1e-10);
        m.command(EngineCommand::SetTempo {
            deck: 1,
            percent: 1.0,
        });
        for _ in 0..128 {
            m.frame();
        }
        assert!(!m.decks[1].sync_enabled);
        assert!((m.decks[1].tempo - 1.0).abs() < 1e-10);
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
        assert!(m.decks[1].tempo.abs() < 1e-8);
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

include!("scheduled_runtime.rs");

#[cfg(test)]
mod playback_tempo_regressions {
    use super::*;
    fn mixer() -> Mixer {
        let mut m = Mixer::new();
        m.load(0, crate::decode::demo_track(0));
        m.load(1, crate::decode::demo_track(1));
        m
    }
    #[test]
    fn absolute_playback_bpm_takes_manual_ownership_without_changing_grid_or_analysis() {
        let mut m = mixer();
        let base = m.decks[0].grid_bpm().unwrap();
        let original = (
            m.decks[0].bpm,
            m.decks[0].grid_offset,
            m.grid_revisions,
            m.decks[0].track.as_ref().unwrap().analysis.bpm,
        );
        m.decks[0].sync_enabled = true;
        m.decks[0].sync_correction = 0.014;
        m.command(EngineCommand::SetPlaybackBpm {
            deck: 0,
            bpm: base * 1.025,
        });
        assert!((m.decks[0].tempo - 2.5).abs() < 1e-10);
        assert!(!m.decks[0].sync_enabled);
        assert_eq!(m.decks[0].sync_correction, 0.);
        assert_eq!(
            original,
            (
                m.decks[0].bpm,
                m.decks[0].grid_offset,
                m.grid_revisions,
                m.decks[0].track.as_ref().unwrap().analysis.bpm
            )
        );
        m.command(EngineCommand::SetTempo {
            deck: 0,
            percent: -2.,
        });
        assert_eq!(m.decks[0].tempo, -2.);
        let before = (m.decks[0].tempo, m.control_revision);
        m.command(EngineCommand::SetPlaybackBpm { deck: 0, bpm: 300. });
        assert_eq!((m.decks[0].tempo, m.control_revision), before);
        m.decks[0].bpm = None;
        m.decks[0].grid = None;
        m.decks[0].track = None;
        m.command(EngineCommand::SetPlaybackBpm { deck: 0, bpm: 124. });
        assert_eq!((m.decks[0].tempo, m.control_revision), before);
    }
    #[test]
    fn paused_reloaded_or_jog_held_master_cannot_retune_live_follower() {
        let mut m = mixer();
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        m.command(EngineCommand::Play { deck: 1 });
        for _ in 0..100 {
            m.frame();
        }
        let matched = m.decks[1].actual_rate();
        m.command(EngineCommand::Pause { deck: 0 });
        m.load(0, crate::decode::demo_track(1));
        m.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 20.,
        });
        for _ in 0..1000 {
            m.frame();
        }
        assert!((m.decks[1].actual_rate() - matched).abs() < 1e-12);
        assert_eq!(m.master_deck, 1);
        assert_eq!(m.decks[1].sync_correction, 0.);
        m.decks[0].playing = true;
        m.decks[0].jog_touch = true;
        for _ in 0..1000 {
            m.frame();
        }
        assert!((m.decks[1].actual_rate() - matched).abs() < 1e-12);
    }
    #[test]
    fn self_master_sync_does_not_mutate_and_master_promotion_clears_correction() {
        let mut m = mixer();
        m.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 2.,
        });
        let revision = m.control_revision;
        for command in [
            EngineCommand::Sync { deck: 0 },
            EngineCommand::SetSync {
                deck: 0,
                enabled: true,
            },
        ] {
            m.command(command);
            assert_eq!(m.decks[0].tempo, 2.);
            assert!(!m.decks[0].sync_enabled);
            assert_eq!(m.control_revision, revision);
        }
        m.decks[1].sync_correction = 0.015;
        m.decks[1].sync_enabled = true;
        m.command(EngineCommand::SetMaster { deck: 1 });
        assert!(!m.decks[1].sync_enabled);
        assert_eq!(m.decks[1].actual_rate(), m.decks[1].rate());
    }
    #[test]
    fn finished_ramp_keeps_the_exact_requested_target() {
        let mut mixer = Mixer::new();
        mixer.command(EngineCommand::SetCrossfader { position: 0.3 });
        mixer.command(EngineCommand::Ramp {
            target: ControlTarget::Crossfader,
            value: -1.0,
            seconds: 0.001,
            clock: None,
            beats: None,
            quantize: None,
        });
        for _ in 0..100 {
            mixer.frame();
        }
        assert!(!mixer.active_ramps()[ControlTarget::Crossfader.index()]);
        assert_eq!(mixer.crossfader.target, -1.0);
    }

    #[test]
    fn started_beat_ramp_finishes_at_natural_eof_but_explicit_pause_or_seek_cancels() {
        for event in ["eof", "pause", "seek"] {
            let mut m = mixer();
            m.sample_rate = 1000;
            let duration = m.decks[0].duration_frames() / m.decks[0].sample_rate();
            m.command(EngineCommand::Seek {
                deck: 0,
                seconds: duration - 0.05,
            });
            m.command(EngineCommand::Play { deck: 0 });
            m.command(EngineCommand::Ramp {
                target: ControlTarget::ChannelFader { deck: 1 },
                value: 0.3,
                seconds: 1.,
                clock: Some(0),
                beats: Some(1.),
                quantize: None,
            });
            for _ in 0..100 {
                m.frame();
            }
            assert!(!m.decks[0].playing);
            assert!(m.active_ramps()[ControlTarget::ChannelFader { deck: 1 }.index()]);
            match event {
                "pause" => m.command(EngineCommand::Pause { deck: 0 }),
                "seek" => m.command(EngineCommand::Seek {
                    deck: 0,
                    seconds: 0.,
                }),
                _ => {}
            }
            for _ in 0..1000 {
                m.frame();
            }
            assert!(!m.active_ramps()[ControlTarget::ChannelFader { deck: 1 }.index()]);
            if event == "eof" {
                assert_eq!(m.decks[1].channel_fader.target, 0.3);
            } else {
                assert!(m.decks[1].channel_fader.target > 0.3);
            }
        }
    }
}

#[cfg(test)]
mod playback_tempo_snapshot_tests {
    use super::*;
    #[test]
    fn snapshot_exposes_canonical_nominal_bpm_and_actual_corrections() {
        let mut m = Mixer::new();
        m.load(0, crate::decode::demo_track(0));
        let base = m.decks[0].grid_bpm().unwrap();
        m.command(EngineCommand::SetPlaybackBpm {
            deck: 0,
            bpm: base * 1.02,
        });
        m.decks[0].bend = 0.002;
        m.decks[0].sync_correction = 0.005;
        let atoms = crate::snapshot::SnapshotAtoms::new();
        atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
        let s = atoms.read(
            m.decks
                .each_ref()
                .map(|d| d.track.as_ref().map(|t| t.analysis.clone())),
        );
        assert_eq!(s.decks[0].bpm, Some(base));
        assert!((s.decks[0].playback_bpm.unwrap() - base * 1.02).abs() < 1e-10);
        assert!((s.decks[0].playback_rate - 1.02).abs() < 1e-10);
        assert!((s.decks[0].actual_playback_rate - 1.027).abs() < 1e-10);
    }
    #[test]
    fn playback_bpm_and_sync_render_paths_allocate_nothing() {
        let mut m = Mixer::new();
        m.load(0, crate::decode::demo_track(0));
        m.load(1, crate::decode::demo_track(1));
        let base = m.decks[0].grid_bpm().unwrap();
        let counts = crate::realtime_audit::allocation_counts(|| {
            m.command(EngineCommand::SetPlaybackBpm {
                deck: 0,
                bpm: base * 1.02,
            });
            m.command(EngineCommand::Play { deck: 0 });
            m.command(EngineCommand::SetSyncRateLimits {
                deck: 1,
                limits: Some(crate::SyncRateLimits {
                    min_rate: 0.95,
                    max_rate: 1.05,
                    max_slew_per_second: 0.01,
                }),
            });
            m.command(EngineCommand::SetSync {
                deck: 1,
                enabled: true,
            });
            m.command(EngineCommand::Play { deck: 1 });
            for _ in 0..1000 {
                m.frame();
            }
        });
        assert_eq!(counts, [0; 3]);
    }
}

#[cfg(test)]
mod future_ramp_eof_regression {
    use super::*;
    #[test]
    fn natural_eof_does_not_start_a_future_quantized_ramp() {
        let mut m = Mixer::new();
        m.sample_rate = 1000;
        m.load(0, crate::decode::demo_track(0));
        let duration = m.decks[0].duration_frames() / m.decks[0].sample_rate();
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: duration - 0.05,
        });
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::Ramp {
            target: ControlTarget::Crossfader,
            value: 1.,
            seconds: 1.,
            clock: Some(0),
            beats: Some(1.),
            quantize: Some(256.),
        });
        for _ in 0..2000 {
            m.frame();
        }
        assert!(!m.decks[0].playing);
        assert!(!m.active_ramps()[ControlTarget::Crossfader.index()]);
        assert_eq!(m.crossfader.target, 0.);
    }
}

#[cfg(test)]
mod sync_convergence_regression {
    use super::*;
    #[test]
    fn modest_phase_servo_converges_without_seeking_or_moving_nominal_tempo() {
        let mut m = Mixer::new();
        m.sample_rate = 1000;
        let track = crate::decode::demo_track(0);
        m.load(0, track.clone());
        m.load(1, track);
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: 4.,
        });
        m.command(EngineCommand::Seek {
            deck: 1,
            seconds: 4.06,
        });
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::Play { deck: 1 });
        let positions = m.decks.each_ref().map(|d| d.position_revision);
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        assert_eq!(positions, m.decks.each_ref().map(|d| d.position_revision));
        for _ in 0..12_000 {
            m.frame();
            assert_eq!(m.decks[1].tempo, 0.);
            assert!(m.decks[1].sync_correction.abs() <= 0.015 + 1e-12);
        }
        let phase = (m.decks[1].grid_position(m.decks[1].seconds())
            - m.decks[0].grid_position(m.decks[0].seconds())
            + 0.5)
            .rem_euclid(1.)
            - 0.5;
        assert!(phase.abs() < 0.005, "remaining phase error {phase}");
        assert_eq!(positions, m.decks.each_ref().map(|d| d.position_revision));
    }
}

#[cfg(test)]
mod adaptive_sync_regressions {
    use super::*;
    pub(super) fn track(variable: bool) -> Arc<TrackData> {
        let mut analysis = crate::decode::demo_track(0).analysis.clone();
        let mut time = 0.2;
        let beats: Vec<_> = (0..400)
            .map(|i| {
                let at = time;
                time += if variable && i >= 64 { 0.6 } else { 0.5 };
                at
            })
            .collect();
        analysis.bpm = Some(120.);
        analysis.bpm_source = Some("beat_this".into());
        analysis.beatgrid_source = Some("beat_this".into());
        analysis.beatgrid_offset_seconds = Some(0.2);
        analysis.duration_seconds = 240.;
        analysis.neural_rhythm = Some(audio_analysis::NeuralRhythm {
            downbeats_seconds: beats.iter().step_by(4).copied().collect(),
            beats_seconds: beats,
            bpm: 120.,
            beat_confidence: 1.,
            downbeat_confidence: 1.,
            model: "fixture".into(),
            analysis_seconds: 0.,
            bar_evidence: vec![],
            beats_per_bar: Some(4),
        });
        Arc::new(TrackData {
            analysis_pending: false,
            samples: vec![[0.; 2]; 240_000],
            sample_rate: 1000,
            analysis,
        })
    }
    fn mixer() -> Mixer {
        let mut m = Mixer::new();
        m.sample_rate = 1000;
        m.load(0, track(false));
        m.load(1, track(true));
        for deck in 0..2 {
            m.command(EngineCommand::Seek { deck, seconds: 0.2 });
        }
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        m.command(EngineCommand::Play { deck: 1 });
        m
    }
    #[test]
    fn sustained_local_tempo_change_exceeds_phase_cap_without_moving_nominal_target() {
        let mut m = mixer();
        let mut maximum_actual = 0.0_f64;
        let mut maximum_phase = 0.0_f64;
        let mut change_frame = None;
        let mut last_unsettled_frame = 0usize;
        for frame in 0usize..90_000 {
            m.frame();
            assert_eq!(m.decks[1].tempo, 0.);
            maximum_actual = maximum_actual.max(m.decks[1].actual_rate());
            let phase = ((m.decks[1].grid_position(m.decks[1].seconds())
                - m.decks[0].grid_position(m.decks[0].seconds())
                + 0.5)
                .rem_euclid(1.)
                - 0.5)
                .abs();
            maximum_phase = maximum_phase.max(phase);
            if m.decks[1].seconds() >= 32.2 {
                change_frame.get_or_insert(frame);
                if phase >= 0.02 || (m.decks[1].transport_rate() - 1.2).abs() >= 0.001 {
                    last_unsettled_frame = frame;
                }
            }
            assert!(m.decks[1].sync_correction.abs() <= 0.015 + 1e-12);
        }
        assert!(
            maximum_actual > 1.18 && maximum_actual < 1.23,
            "maximum actual rate {maximum_actual}"
        );
        assert!((m.decks[1].transport_rate() - 1.2).abs() < 0.001);
        let phase = (m.decks[1].grid_position(m.decks[1].seconds())
            - m.decks[0].grid_position(m.decks[0].seconds())
            + 0.5)
            .rem_euclid(1.)
            - 0.5;
        assert!(phase.abs() < 0.02, "settled phase {phase}");
        eprintln!(
            "NATIVE_METRIC {}",
            serde_json::json!({"fixture":"abrupt_120_to_100_bpm_source", "actual_rate_peak":maximum_actual, "phase_error_peak_beats":maximum_phase, "settled_phase_error_beats":phase.abs(), "settled_transport_rate_error":(m.decks[1].transport_rate()-1.2).abs(), "recovery_seconds_to_rate_0_001_and_phase_0_02": (last_unsettled_frame.saturating_sub(change_frame.unwrap_or(0))) as f64 / 1000.})
        );
    }
    #[test]
    fn freeze_and_sync_role_changes_preserve_actual_rate_then_explicit_tempo_clears_it() {
        let mut m = mixer();
        m.decks[1].sync_tempo_offset = 0.2;
        m.decks[1].sync_correction = 0.01;
        let actual = m.decks[1].actual_rate();
        m.command(EngineCommand::Pause { deck: 0 });
        m.frame();
        m.load(0, track(false));
        m.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 20.,
        });
        for _ in 0..1000 {
            m.frame();
            assert!((m.decks[1].actual_rate() - actual).abs() < 1e-12);
        }
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: false,
        });
        assert!((m.decks[1].actual_rate() - actual).abs() < 1e-12);
        assert_eq!(m.decks[1].sync_tempo_offset, 0.);
        m.decks[1].sync_tempo_offset = 0.02;
        m.decks[1].sync_correction = 0.005;
        let promoted = m.decks[1].actual_rate();
        m.command(EngineCommand::SetMaster { deck: 1 });
        assert!((m.decks[1].actual_rate() - promoted).abs() < 1e-12);
        assert_eq!(m.decks[1].sync_tempo_offset, 0.);
        eprintln!(
            "NATIVE_METRIC {}",
            serde_json::json!({"fixture":"promotion_rate_continuity", "actual_rate_delta":m.decks[1].actual_rate()-promoted})
        );
        for command in [
            EngineCommand::SetTempo {
                deck: 1,
                percent: 2.,
            },
            EngineCommand::SetPlaybackBpm {
                deck: 1,
                bpm: 122.4,
            },
        ] {
            m.decks[1].sync_tempo_offset = 0.15;
            m.decks[1].sync_correction = 0.01;
            m.command(command);
            assert!((m.decks[1].actual_rate() - 1.02).abs() < 1e-12);
            assert_eq!(m.decks[1].sync_tempo_offset, 0.);
        }
    }
    #[test]
    fn initial_local_fit_launch_and_beat_duration_use_adaptive_rate_without_allocations() {
        let mut m = Mixer::new();
        m.sample_rate = 1000;
        m.load(0, track(false));
        m.load(1, track(true));
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: 60.2,
        });
        m.command(EngineCommand::Seek {
            deck: 1,
            seconds: 60.2,
        });
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        assert!((m.decks[1].transport_rate() - 1.2).abs() < 1e-12);
        assert_eq!(m.decks[1].tempo, 0.);
        let at = m.decks[0].grid_time(m.decks[0].grid_position(m.decks[0].seconds()).floor() + 2.);
        m.command(EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: 0,
            at_seconds: Some(at),
            quantize: 1.,
        });
        assert!(m.launches[1].is_some());
        let counts = crate::realtime_audit::allocation_counts(|| {
            for _ in 0..1200 {
                m.frame();
            }
        });
        assert_eq!(counts, [0; 3]);
        assert!(m.decks[1].playing && m.launches[1].is_none());
        m.command(EngineCommand::Ramp {
            target: ControlTarget::Crossfader,
            value: 1.,
            seconds: 1.,
            clock: Some(1),
            beats: Some(1.),
            quantize: None,
        });
        let ramp = m.ramps[ControlTarget::Crossfader.index()].unwrap();
        assert_eq!(ramp.end - ramp.start, 500);
        let source = m.decks[1].track.as_ref().unwrap().clone();
        let actual = m.decks[1].actual_rate();
        let old = m.instant_double(0, source);
        assert!((m.decks[0].actual_rate() - actual).abs() < 1e-12);
        assert_eq!(m.decks[0].sync_tempo_offset, 0.);
        drop(old);
        m.load(1, track(false));
        assert_eq!(m.decks[1].sync_tempo_offset, 0.);
    }
}

#[cfg(test)]
mod bounded_sync_regressions {
    use super::*;

    fn limits() -> crate::SyncRateLimits {
        crate::SyncRateLimits {
            min_rate: 0.98,
            max_rate: 1.02,
            max_slew_per_second: 0.01,
        }
    }

    #[test]
    fn anomalous_local_pulse_preserves_audible_rate_and_reports_degradation() {
        let mut m = Mixer::new();
        m.sample_rate = 1000;
        m.load(0, super::adaptive_sync_regressions::track(false));
        let mut source = super::adaptive_sync_regressions::track(false);
        let rhythm = Arc::get_mut(&mut source)
            .unwrap()
            .analysis
            .neural_rhythm
            .as_mut()
            .unwrap();
        let mut at = 0.2;
        rhythm.beats_seconds = (0..400)
            .map(|i| {
                let time = at;
                at += if (64..96).contains(&i) { 1. / 3. } else { 0.5 };
                time
            })
            .collect();
        rhythm.downbeats_seconds = rhythm.beats_seconds.iter().step_by(4).copied().collect();
        m.load(1, source);
        for deck in 0..2 {
            m.command(EngineCommand::Seek { deck, seconds: 0.2 });
        }
        m.command(EngineCommand::SetSyncRateLimits {
            deck: 1,
            limits: Some(limits()),
        });
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        m.command(EngineCommand::Play { deck: 1 });
        let revisions = m.decks.each_ref().map(|d| d.position_revision);
        let mut previous = m.decks[1].actual_rate();
        let mut limited_frames = 0;
        for _ in 0..70_000 {
            m.frame();
            let d = &m.decks[1];
            assert!(limits().contains(d.actual_rate()));
            assert!((d.actual_rate() - previous).abs() <= 0.000_010_001);
            if d.sync_rate_limited {
                limited_frames += 1;
                assert!((d.transport_rate() - 1.).abs() < 1e-8);
            }
            previous = d.actual_rate();
        }
        assert!(
            limited_frames > 5000,
            "the corrupt-pulse region must be rejected"
        );
        assert!(
            !m.decks[1].sync_rate_limited,
            "normal evidence can resume bounded following"
        );
        assert_eq!(m.decks[1].target_playback_bpm(), Some(120.));
        assert_eq!(revisions, m.decks.each_ref().map(|d| d.position_revision));
        assert_eq!(m.decks[1].bpm, Some(120.));
    }

    #[test]
    fn explicitly_wide_policy_still_follows_real_variable_tempo() {
        let mut m = Mixer::new();
        m.sample_rate = 1000;
        m.load(0, super::adaptive_sync_regressions::track(false));
        m.load(1, super::adaptive_sync_regressions::track(true));
        m.command(EngineCommand::SetSyncRateLimits {
            deck: 1,
            limits: Some(crate::SyncRateLimits {
                min_rate: 0.95,
                max_rate: 1.25,
                max_slew_per_second: 0.1,
            }),
        });
        m.command(EngineCommand::Play { deck: 0 });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        m.command(EngineCommand::Play { deck: 1 });
        for _ in 0..90_000 {
            m.frame();
            assert!(!m.decks[1].sync_rate_limited);
        }
        assert!((m.decks[1].actual_rate() - 1.2).abs() < 0.002);
    }

    #[test]
    fn limits_are_observable_and_manual_tempo_or_source_change_releases_them() {
        let mut m = Mixer::new();
        m.load(0, super::adaptive_sync_regressions::track(false));
        m.command(EngineCommand::SetSyncRateLimits {
            deck: 0,
            limits: Some(limits()),
        });
        m.decks[0].sync_rate_limited = true;
        let atoms = crate::snapshot::SnapshotAtoms::new();
        atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
        let s = atoms.read([None, None]);
        assert_eq!(s.decks[0].sync_rate_limits, Some(limits()));
        assert!(s.decks[0].sync_rate_limited);
        m.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 3.,
        });
        assert!(m.decks[0].sync_rate_limits.is_none());
        assert!(!m.decks[0].sync_rate_limited);
        m.command(EngineCommand::SetSyncRateLimits {
            deck: 0,
            limits: Some(limits()),
        });
        m.load(0, super::adaptive_sync_regressions::track(false));
        assert!(m.decks[0].sync_rate_limits.is_none());
    }
}

#[cfg(test)]
mod local_absolute_bpm_regressions {
    use super::adaptive_sync_regressions::track;
    use super::*;
    #[test]
    fn absolute_bpm_uses_current_local_grid_and_acknowledges_stable_target() {
        let mut m = Mixer::new();
        m.sample_rate = 1000;
        m.load(0, track(false));
        m.load(1, track(true));
        m.command(EngineCommand::Seek {
            deck: 1,
            seconds: 60.2,
        });
        assert_eq!(m.decks[1].grid_bpm(), Some(120.));
        assert!((m.decks[1].local_grid_bpm().unwrap() - 100.).abs() < 1e-10);
        m.command(EngineCommand::SetPlaybackBpm { deck: 1, bpm: 124. });
        assert!((m.decks[1].rate() - 1.24).abs() < 1e-10);
        assert_eq!(m.decks[1].target_playback_bpm(), Some(124.));
        let atoms = crate::snapshot::SnapshotAtoms::new();
        atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
        let s = atoms.read(
            m.decks
                .each_ref()
                .map(|d| d.track.as_ref().map(|t| t.analysis.clone())),
        );
        assert_eq!(s.decks[1].bpm, Some(120.));
        assert!((s.decks[1].playback_bpm.unwrap() - 124.).abs() < 1e-10);
        assert!((s.decks[1].actual_playback_bpm.unwrap() - 124.).abs() < 1e-10);
        assert_eq!(s.decks[1].target_playback_bpm, Some(124.));
        eprintln!(
            "NATIVE_METRIC {}",
            serde_json::json!({"fixture":"local_absolute_124_from_source_120_local_100", "absolute_bpm_error":s.decks[1].actual_playback_bpm.unwrap()-124.})
        );
        m.command(EngineCommand::Seek {
            deck: 1,
            seconds: 0.2,
        });
        assert_eq!(m.decks[1].target_playback_bpm(), Some(124.));
        assert!((m.decks[1].local_grid_bpm().unwrap() * m.decks[1].rate() - 148.8).abs() < 1e-10);
    }
    #[test]
    fn quantized_start_preserves_tempo_for_locally_matched_and_deliberately_unmatched_decks() {
        for matched in [false, true] {
            let mut m = Mixer::new();
            m.sample_rate = 1000;
            m.load(0, track(false));
            m.load(1, track(true));
            for deck in 0..2 {
                m.command(EngineCommand::Seek {
                    deck,
                    seconds: 60.2,
                });
            }
            m.command(EngineCommand::SetPlaybackBpm { deck: 0, bpm: 124. });
            if matched {
                m.command(EngineCommand::SetPlaybackBpm { deck: 1, bpm: 124. });
            }
            let requested = m.decks[1].tempo;
            m.command(EngineCommand::Play { deck: 0 });
            let at =
                m.decks[0].grid_time(m.decks[0].grid_position(m.decks[0].seconds()).floor() + 2.);
            m.command(EngineCommand::PlayQuantized {
                deck: 1,
                clock: 0,
                clock_revision: 0,
                at_seconds: Some(at),
                quantize: 1.,
            });
            assert!(m.launches[1].is_some());
            for _ in 0..1500 {
                m.frame();
            }
            assert!(m.decks[1].playing && m.launches[1].is_none());
            assert_eq!(m.decks[1].tempo, requested);
            assert!(!m.decks[1].sync_enabled);
        }
    }
    #[test]
    fn source_change_promotes_only_healthy_playing_follower_without_rate_jump() {
        for unload in [false, true] {
            let mut m = Mixer::new();
            m.sample_rate = 1000;
            m.load(0, track(false));
            m.load(1, track(true));
            m.command(EngineCommand::Seek {
                deck: 1,
                seconds: 60.2,
            });
            m.command(EngineCommand::Play { deck: 0 });
            m.command(EngineCommand::SetSync {
                deck: 1,
                enabled: true,
            });
            m.command(EngineCommand::Play { deck: 1 });
            m.decks[1].sync_correction = 0.005;
            let actual = m.decks[1].actual_rate();
            if unload {
                drop(m.unload(0));
            } else {
                drop(m.load(0, track(true)));
            }
            assert_eq!(m.master_deck, 1);
            assert!((m.decks[1].actual_rate() - actual).abs() < 1e-12);
            assert!(!m.decks[1].sync_enabled);
            let before = m.master_deck;
            m.command(EngineCommand::Pause { deck: 1 });
            for _ in 0..100 {
                m.frame();
            }
            assert_eq!(m.master_deck, before);
        }
        let mut m = Mixer::new();
        m.load(0, track(false));
        m.load(1, track(true));
        m.load(0, track(true));
        assert_eq!(m.master_deck, 0);
        m.command(EngineCommand::Play { deck: 1 });
        m.decks[1].jog_touch = true;
        m.unload(0);
        assert_eq!(m.master_deck, 0);
    }
}

impl Mixer {
    pub(crate) fn direct_deck_state(&self, deck: usize) -> crate::DirectDeckState {
        let d = &self.decks[deck];
        crate::DirectDeckState {
            load_revision: self.load_revisions[deck],
            position_revision: d.position_revision,
            playing: d.playing,
            position_seconds: d.seconds(),
            channel_fader: d.channel_fader.target,
            channel_fader_actual: d.channel_fader.value,
            launch_pending: self.launches[deck].is_some(),
            launch_clock: self.launches[deck].map(|l| l.clock),
            launch_at_seconds: self.launches[deck].map(|l| l.at_seconds),
            channel_fader_ramp_active: self.ramps[ControlTarget::ChannelFader { deck }.index()]
                .is_some(),
        }
    }
}

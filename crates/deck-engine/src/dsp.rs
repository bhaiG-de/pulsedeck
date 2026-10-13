use crate::{
    decode::TrackData,
    model::{EngineCommand, LoopBoundary},
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

pub(crate) struct Mixer {
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
    smoothing_rate: u32,
    fade_value: f32,
    fade_gains: [f32; 2],
}
impl Mixer {
    pub fn new() -> Self {
        Self {
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
            smoothing_rate: 0,
            fade_value: f32::NAN,
            fade_gains: [0.70710677; 2],
        }
    }
    pub fn load(&mut self, deck: usize, track: Arc<TrackData>) -> Option<Arc<TrackData>> {
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
        d.preparation_generation = generation;
        d.prepared_generation = if prepared.is_some() { generation } else { 0 };
        d.prepared = prepared;
        d.sync_enabled = false;
        [old_track, old_prepared]
    }
    pub fn unload(&mut self, deck: usize) -> [Option<Arc<TrackData>>; 2] {
        let d = &mut self.decks[deck];
        d.playing = false;
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
        use EngineCommand::*;
        match command {
            Play { deck } => {
                let d = &mut self.decks[deck];
                if d.track.is_some() {
                    if d.position >= d.duration_frames() {
                        d.seek(d.cue);
                    }
                    d.playing = true;
                }
            }
            Pause { deck } => self.decks[deck].playing = false,
            TogglePlay { deck } => {
                let playing = self.decks[deck].playing;
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
                    let period = 60.0 / d.bpm.unwrap_or(120.0);
                    let start = (((d.seconds() - d.grid_offset) / period).floor() * period
                        + d.grid_offset)
                        .max(0.0)
                        * d.sample_rate();
                    let end = (start + beats * period * d.sample_rate()).min(d.duration_frames());
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
            RestoreMemoryCues { deck, cues } => self.decks[deck].memory_cues = cues,
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
                let time = d.seconds() + beats * 60.0 / d.bpm.unwrap_or(120.0);
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
                let leader_tempo = leader.bpm.unwrap_or(120.0) * leader.rate();
                let leader_phase = ((leader.seconds() - leader.grid_offset)
                    / (60.0 / leader.bpm.unwrap_or(120.0)))
                .rem_euclid(1.0);
                let d = &mut self.decks[deck];
                if let Some(bpm) = d.bpm {
                    d.tempo = ((leader_tempo / bpm - 1.0) * 100.0).clamp(-90.0, 100.0);
                    let period = 60.0 / bpm;
                    let beat = ((d.seconds() - d.grid_offset) / period).floor();
                    d.seek(d.grid_offset + (beat + leader_phase) * period);
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
            SetBpm { deck, bpm } => self.decks[deck].bpm = Some(bpm),
            SetBeatgrid {
                deck,
                offset_seconds,
            } => self.decks[deck].grid_offset = offset_seconds,
        }
    }
    #[inline]
    pub fn frame(&mut self) -> ([f32; 2], [f32; 2]) {
        let leader_phase = (self.decks[self.master_deck].seconds()
            - self.decks[self.master_deck].grid_offset)
            * self.decks[self.master_deck].bpm.unwrap_or(120.0)
            / 60.0;
        let leader_bpm = self.decks[self.master_deck]
            .bpm
            .map(|bpm| bpm * self.decks[self.master_deck].rate());
        for deck in 0..2 {
            if deck != self.master_deck && self.decks[deck].sync_enabled {
                if let (Some(target), Some(bpm)) = (leader_bpm, self.decks[deck].bpm) {
                    self.decks[deck].tempo = ((target / bpm - 1.0) * 100.0).clamp(-90.0, 100.0);
                    let follower =
                        (self.decks[deck].seconds() - self.decks[deck].grid_offset) * bpm / 60.0;
                    let difference = (leader_phase - follower + 0.5).rem_euclid(1.0) - 0.5;
                    self.decks[deck].sync_correction = (difference * 0.1).clamp(-0.02, 0.02);
                }
            }
        }
        let left = self.decks[0].render(self.sample_rate);
        let right = self.decks[1].render(self.sample_rate);
        if self.smoothing_rate != self.sample_rate {
            self.smoothing_rate = self.sample_rate;
            self.smoothing = 1.0 - (-1.0 / (self.sample_rate as f32 * 0.008)).exp();
        }
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

use crate::{dsp::Mixer, model::*};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
fn float(x: f32) -> AtomicU32 {
    AtomicU32::new(x.to_bits())
}
fn double(x: f64) -> AtomicU64 {
    AtomicU64::new(x.to_bits())
}
fn putf(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}
fn putd(a: &AtomicU64, v: f64) {
    a.store(v.to_bits(), Ordering::Relaxed);
}
fn getf(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}
fn getd(a: &AtomicU64) -> f64 {
    f64::from_bits(a.load(Ordering::Relaxed))
}
struct DeckAtoms {
    playing: AtomicBool,
    position: AtomicU64,
    jog_touch: AtomicBool,
    position_revision: AtomicU64,
    rate: AtomicU64,
    tempo: AtomicU64,
    gain: AtomicU32,
    fader: AtomicU32,
    eq: [AtomicU32; 3],
    filter: AtomicU32,
    cue: AtomicU64,
    hot: [AtomicU64; 8],
    memory: [AtomicU64; 10],
    peak: [AtomicU32; 2],
    loop_start: AtomicU64,
    loop_end: AtomicU64,
    loop_edit: AtomicU32,
    fx_beats: AtomicU64,
    cue_enabled: AtomicBool,
    echo: AtomicU32,
    reverb: AtomicU32,
    bpm: AtomicU64,
    grid: AtomicU64,
    sync: AtomicBool,
    master: AtomicBool,
    pitch: AtomicU64,
    keylock: AtomicBool,
    pending: AtomicBool,
}
impl DeckAtoms {
    fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            position: double(0.0),
            jog_touch: AtomicBool::new(false),
            position_revision: AtomicU64::new(0),
            rate: double(1.0),
            tempo: double(0.0),
            gain: float(1.0),
            fader: float(1.0),
            eq: std::array::from_fn(|_| float(1.0)),
            filter: float(0.0),
            cue: double(0.0),
            hot: std::array::from_fn(|_| double(f64::NAN)),
            memory: std::array::from_fn(|_| double(f64::NAN)),
            peak: std::array::from_fn(|_| float(0.0)),
            loop_start: double(f64::NAN),
            loop_end: double(f64::NAN),
            loop_edit: AtomicU32::new(0),
            fx_beats: double(0.5),
            cue_enabled: AtomicBool::new(false),
            echo: float(0.0),
            reverb: float(0.0),
            bpm: double(f64::NAN),
            grid: double(0.0),
            sync: AtomicBool::new(false),
            master: AtomicBool::new(false),
            pitch: double(0.0),
            keylock: AtomicBool::new(false),
            pending: AtomicBool::new(false),
        }
    }
    fn read(&self, track: Option<TrackAnalysis>) -> DeckSnapshot {
        fn optional(x: f64) -> Option<f64> {
            x.is_finite().then_some(x)
        }
        DeckSnapshot {
            track,
            playing: self.playing.load(Ordering::Relaxed),
            position_seconds: getd(&self.position),
            jog_touch: self.jog_touch.load(Ordering::Relaxed),
            position_revision: self.position_revision.load(Ordering::Relaxed),
            playback_rate: getd(&self.rate),
            tempo_percent: getd(&self.tempo),
            gain: getf(&self.gain),
            channel_fader: getf(&self.fader),
            eq_low: getf(&self.eq[0]),
            eq_mid: getf(&self.eq[1]),
            eq_high: getf(&self.eq[2]),
            filter: getf(&self.filter),
            cue_seconds: getd(&self.cue),
            hot_cues: std::array::from_fn(|i| optional(getd(&self.hot[i]))),
            memory_cues: std::array::from_fn(|i| optional(getd(&self.memory[i]))),
            peak: std::array::from_fn(|i| getf(&self.peak[i])),
            loop_start_seconds: optional(getd(&self.loop_start)),
            loop_end_seconds: optional(getd(&self.loop_end)),
            loop_edit: match self.loop_edit.load(Ordering::Relaxed) {
                1 => Some(crate::LoopBoundary::In),
                2 => Some(crate::LoopBoundary::Out),
                _ => None,
            },
            fx_beats: getd(&self.fx_beats),
            cue_enabled: self.cue_enabled.load(Ordering::Relaxed),
            echo: getf(&self.echo),
            reverb: getf(&self.reverb),
            bpm: optional(getd(&self.bpm)),
            beatgrid_offset_seconds: getd(&self.grid),
            sync_enabled: self.sync.load(Ordering::Relaxed),
            is_master: self.master.load(Ordering::Relaxed),
            pitch_semitones: getd(&self.pitch),
            keylock: self.keylock.load(Ordering::Relaxed),
            preparation_pending: self.pending.load(Ordering::Relaxed),
        }
    }
}
pub(crate) struct SnapshotAtoms {
    sequence: AtomicU64,
    rate: AtomicU32,
    channels: AtomicU32,
    buffer: AtomicU32,
    pub running: AtomicBool,
    pub stream_revision: AtomicU64,
    frames: AtomicU64,
    master_peak: [AtomicU32; 2],
    cue_peak: [AtomicU32; 2],
    pub errors: AtomicU64,
    gain: AtomicU32,
    crossfader: AtomicU32,
    cue_gain: AtomicU32,
    cue_mix: AtomicU32,
    sampler: AtomicU32,
    decks: [DeckAtoms; 2],
}
impl SnapshotAtoms {
    pub fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            rate: AtomicU32::new(48000),
            channels: AtomicU32::new(0),
            buffer: AtomicU32::new(0),
            running: AtomicBool::new(false),
            stream_revision: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            master_peak: std::array::from_fn(|_| float(0.0)),
            cue_peak: std::array::from_fn(|_| float(0.0)),
            errors: AtomicU64::new(0),
            gain: float(0.8),
            crossfader: float(0.0),
            cue_gain: float(0.8),
            cue_mix: float(0.0),
            sampler: float(0.7),
            decks: std::array::from_fn(|_| DeckAtoms::new()),
        }
    }
    pub fn publish(&self, m: &Mixer, channels: u16, frames: u32, master: [f32; 2], cue: [f32; 2]) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.rate.store(m.sample_rate, Ordering::Relaxed);
        self.channels.store(channels as u32, Ordering::Relaxed);
        self.buffer.store(frames, Ordering::Relaxed);
        self.frames.store(m.rendered_frames, Ordering::Relaxed);
        putf(&self.gain, m.master_gain.target);
        putf(&self.crossfader, m.crossfader.target);
        putf(&self.cue_gain, m.cue_gain.target);
        putf(&self.cue_mix, m.cue_mix.target);
        putf(&self.sampler, m.sampler_gain.target);
        for i in 0..2 {
            putf(&self.master_peak[i], master[i]);
            putf(&self.cue_peak[i], cue[i]);
            let d = &m.decks[i];
            let a = &self.decks[i];
            a.playing.store(d.playing, Ordering::Relaxed);
            putd(&a.position, d.seconds());
            a.jog_touch.store(d.jog_touch, Ordering::Relaxed);
            a.position_revision
                .store(d.position_revision, Ordering::Relaxed);
            putd(&a.rate, d.rate());
            putd(&a.tempo, d.tempo);
            putf(&a.gain, d.gain.target);
            putf(&a.fader, d.channel_fader.target);
            for j in 0..3 {
                putf(&a.eq[j], d.eq[j].target);
            }
            putf(&a.filter, d.filter.target);
            putd(&a.cue, d.cue);
            for j in 0..8 {
                putd(&a.hot[j], d.hot_cues[j].unwrap_or(f64::NAN));
            }
            for j in 0..10 {
                putd(&a.memory[j], d.memory_cues[j].unwrap_or(f64::NAN));
            }
            for j in 0..2 {
                putf(&a.peak[j], d.peak[j].abs());
            }
            putd(
                &a.loop_start,
                d.loop_region.map_or(f64::NAN, |(s, _)| s / d.sample_rate()),
            );
            putd(
                &a.loop_end,
                d.loop_region.map_or(f64::NAN, |(_, e)| e / d.sample_rate()),
            );
            a.loop_edit.store(
                match d.loop_edit {
                    Some(crate::LoopBoundary::In) => 1,
                    Some(crate::LoopBoundary::Out) => 2,
                    None => 0,
                },
                Ordering::Relaxed,
            );
            putd(&a.fx_beats, d.fx_beats);
            a.cue_enabled.store(d.cue_enabled, Ordering::Relaxed);
            putf(&a.echo, d.echo.target);
            putf(&a.reverb, d.reverb.target);
            putd(&a.bpm, d.bpm.unwrap_or(f64::NAN));
            putd(&a.grid, d.grid_offset);
            a.sync.store(d.sync_enabled, Ordering::Relaxed);
            a.master.store(m.master_deck == i, Ordering::Relaxed);
            putd(&a.pitch, d.pitch);
            a.keylock.store(d.keylock, Ordering::Relaxed);
            a.pending.store(
                (d.keylock || d.pitch.abs() > 0.0001)
                    && (d.prepared_generation != d.preparation_generation
                        || (d.prepared_rate - d.rate()).abs() > 1e-6),
                Ordering::Relaxed,
            );
        }
        self.sequence.fetch_add(1, Ordering::Release);
    }
    pub fn read(&self, tracks: [Option<TrackAnalysis>; 2]) -> EngineSnapshot {
        loop {
            let before = self.sequence.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let mut result = EngineSnapshot {
                sample_rate: self.rate.load(Ordering::Relaxed),
                output_channels: self.channels.load(Ordering::Relaxed) as u16,
                buffer_frames: self.buffer.load(Ordering::Relaxed),
                running: self.running.load(Ordering::Relaxed),
                rendered_frames: self.frames.load(Ordering::Relaxed),
                master_peak: std::array::from_fn(|i| getf(&self.master_peak[i])),
                cue_peak: std::array::from_fn(|i| getf(&self.cue_peak[i])),
                underruns: self.errors.load(Ordering::Relaxed),
                master_gain: getf(&self.gain),
                crossfader: getf(&self.crossfader),
                cue_gain: getf(&self.cue_gain),
                cue_mix: getf(&self.cue_mix),
                sampler_gain: getf(&self.sampler),
                decks: std::array::from_fn(|i| self.decks[i].read(None)),
            };
            std::sync::atomic::fence(Ordering::Acquire);
            if self.sequence.load(Ordering::Relaxed) == before {
                for (deck, track) in result.decks.iter_mut().zip(tracks) {
                    deck.track = track;
                }
                return result;
            }
        }
    }
}

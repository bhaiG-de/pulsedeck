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
    grid_revision: AtomicU64,
    rate: AtomicU64,
    tempo: AtomicU64,
    actual_rate: AtomicU64,
    playback_bpm: AtomicU64,
    local_bpm: AtomicU64,
    actual_playback_bpm: AtomicU64,
    target_playback_bpm: AtomicU64,
    fx_tail_active: AtomicBool,
    restricted_tail_active: AtomicBool,
    fx_tail_peak: [AtomicU32; 2],
    normalization_enabled: AtomicBool,
    normalization_db: AtomicU32,
    gain: AtomicU32,
    fader: AtomicU32,
    fader_actual: AtomicU32,
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
    grid_valid: AtomicBool,
    grid_downbeat: AtomicBool,
    grid_manual: AtomicBool,
    grid_uniform: AtomicBool,
    grid_anchor: AtomicU64,
    beat_position: AtomicU64,
    launch_at: AtomicU64,
    launch_clock: AtomicU64,
    sync: AtomicBool,
    sync_min_rate: AtomicU64,
    sync_max_rate: AtomicU64,
    sync_max_slew: AtomicU64,
    sync_rate_limited: AtomicBool,
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
            grid_revision: AtomicU64::new(0),
            rate: double(1.0),
            tempo: double(0.0),
            actual_rate: double(1.0),
            playback_bpm: double(f64::NAN),
            local_bpm: double(f64::NAN),
            actual_playback_bpm: double(f64::NAN),
            target_playback_bpm: double(f64::NAN),
            fx_tail_active: AtomicBool::new(false),
            restricted_tail_active: AtomicBool::new(false),
            fx_tail_peak: [float(0.), float(0.)],
            normalization_enabled: AtomicBool::new(true),
            normalization_db: float(0.),
            gain: float(1.0),
            fader: float(1.0),
            fader_actual: float(1.0),
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
            grid_valid: AtomicBool::new(false),
            grid_downbeat: AtomicBool::new(false),
            grid_manual: AtomicBool::new(false),
            grid_uniform: AtomicBool::new(false),
            grid_anchor: AtomicU64::new(u64::MAX),
            beat_position: double(f64::NAN),
            launch_at: double(f64::NAN),
            launch_clock: AtomicU64::new(u64::MAX),
            sync: AtomicBool::new(false),
            sync_min_rate: double(f64::NAN),
            sync_max_rate: double(f64::NAN),
            sync_max_slew: double(f64::NAN),
            sync_rate_limited: AtomicBool::new(false),
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
            grid_revision: self.grid_revision.load(Ordering::Relaxed),
            track,
            playing: self.playing.load(Ordering::Relaxed),
            position_seconds: getd(&self.position),
            jog_touch: self.jog_touch.load(Ordering::Relaxed),
            position_revision: self.position_revision.load(Ordering::Relaxed),
            playback_rate: getd(&self.rate),
            actual_playback_rate: getd(&self.actual_rate),
            playback_bpm: optional(getd(&self.playback_bpm)),
            local_bpm: optional(getd(&self.local_bpm)),
            actual_playback_bpm: optional(getd(&self.actual_playback_bpm)),
            target_playback_bpm: optional(getd(&self.target_playback_bpm)),
            tempo_percent: getd(&self.tempo),
            fx_tail_active: self.fx_tail_active.load(Ordering::Relaxed),
            restricted_tail_active: self.restricted_tail_active.load(Ordering::Relaxed),
            fx_tail_peak: self.fx_tail_peak.each_ref().map(getf),
            normalization_enabled: self.normalization_enabled.load(Ordering::Relaxed),
            normalization_gain_db: getf(&self.normalization_db),
            gain: getf(&self.gain),
            channel_fader: getf(&self.fader),
            channel_fader_actual: getf(&self.fader_actual),
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
            beatgrid_valid: self.grid_valid.load(Ordering::Relaxed),
            beatgrid_downbeat: self.grid_downbeat.load(Ordering::Relaxed),
            beatgrid_manual: self.grid_manual.load(Ordering::Relaxed),
            beatgrid_uniform: self.grid_uniform.load(Ordering::Relaxed),
            beatgrid_anchor_index: (self.grid_anchor.load(Ordering::Relaxed) != u64::MAX)
                .then(|| self.grid_anchor.load(Ordering::Relaxed) as usize),
            beat_position: optional(getd(&self.beat_position)),
            launch_pending: getd(&self.launch_at).is_finite(),
            launch_clock: {
                let clock = self.launch_clock.load(Ordering::Relaxed);
                (clock < 2).then_some(clock as usize)
            },
            launch_at_seconds: optional(getd(&self.launch_at)),
            sync_enabled: self.sync.load(Ordering::Relaxed),
            sync_rate_limits: optional(getd(&self.sync_min_rate)).map(|min_rate| SyncRateLimits {
                min_rate,
                max_rate: getd(&self.sync_max_rate),
                max_slew_per_second: getd(&self.sync_max_slew),
            }),
            sync_rate_limited: self.sync_rate_limited.load(Ordering::Relaxed),
            is_master: self.master.load(Ordering::Relaxed),
            pitch_semitones: getd(&self.pitch),
            keylock: self.keylock.load(Ordering::Relaxed),
            preparation_pending: self.pending.load(Ordering::Relaxed),
        }
    }
}
struct QualityAtoms {
    input_peaks: [AtomicU32; 4],
    reductions: [AtomicU32; 2],
    counts: [AtomicU64; 10],
}
impl QualityAtoms {
    fn new() -> Self {
        Self {
            input_peaks: std::array::from_fn(|_| float(0.)),
            reductions: std::array::from_fn(|_| float(0.)),
            counts: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
    fn publish(&self, q: &crate::AudioQualitySnapshot) {
        for i in 0..2 {
            putf(&self.input_peaks[i], q.master_input_peak[i]);
            putf(&self.input_peaks[i + 2], q.cue_input_peak[i]);
        }
        putf(&self.reductions[0], q.master_gain_reduction_db);
        putf(&self.reductions[1], q.cue_gain_reduction_db);
        let counts = [
            q.master_limited_frames,
            q.cue_limited_frames,
            q.invalid_signal_frames,
            q.processor_resets[0],
            q.processor_resets[1],
            q.processor_blocks[0],
            q.processor_blocks[1],
            q.callback_count,
            q.callback_max_ns,
            q.callback_deadline_exceeded,
        ];
        for (slot, value) in self.counts.iter().zip(counts) {
            slot.store(value, Ordering::Relaxed);
        }
    }
    fn read(&self) -> crate::AudioQualitySnapshot {
        let c = self.counts.each_ref().map(|v| v.load(Ordering::Relaxed));
        crate::AudioQualitySnapshot {
            headroom_db: 6.,
            output_latency_frames: 128,
            master_input_peak: [getf(&self.input_peaks[0]), getf(&self.input_peaks[1])],
            cue_input_peak: [getf(&self.input_peaks[2]), getf(&self.input_peaks[3])],
            master_gain_reduction_db: getf(&self.reductions[0]),
            cue_gain_reduction_db: getf(&self.reductions[1]),
            master_limited_frames: c[0],
            cue_limited_frames: c[1],
            invalid_signal_frames: c[2],
            processor_resets: [c[3], c[4]],
            processor_blocks: [c[5], c[6]],
            callback_count: c[7],
            callback_max_ns: c[8],
            callback_deadline_exceeded: c[9],
        }
    }
}
pub(crate) struct SnapshotAtoms {
    quality: QualityAtoms,
    load_revisions: [AtomicU64; 2],
    pub applied_fence: AtomicU64,
    pub guarded_control_receipts: [AtomicU64; 64],
    direct_control_receipts: [[AtomicU64; 25]; 64],
    control_revision: AtomicU64,
    deck_control_revisions: [AtomicU64; 2],
    target_control_revisions: [AtomicU64; crate::COMMAND_TARGET_COUNT],
    batch: [AtomicU64; 17],
    batch_events: [[AtomicU64; 23]; crate::scheduled::MAX_SCHEDULED_STEPS],
    batch_updates: [[AtomicU64; 5]; crate::scheduled::MAX_QUEUE_UPDATES],
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
    sample_positions: [AtomicU64; 16],
    active_ramps: [AtomicBool; crate::CONTROL_TARGET_COUNT],
    decks: [DeckAtoms; 2],
}
impl SnapshotAtoms {
    /// Same seqlock as transport publication: receipt state is one callback
    /// decision, never a mixture of a reused ring slot and newer playback.
    pub fn direct_control_receipt(&self, ticket: u64) -> Option<crate::DirectControlReceipt> {
        loop {
            let start = self.sequence.load(Ordering::Acquire);
            if start & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let words = self.direct_control_receipts[ticket as usize % 64]
                .each_ref()
                .map(|a| a.load(Ordering::Relaxed));
            std::sync::atomic::fence(Ordering::Acquire);
            if self.sequence.load(Ordering::Acquire) != start {
                continue;
            }
            if words[0] == 0 {
                return None;
            }
            let state = |w: &[u64]| crate::DirectDeckState {
                load_revision: w[0],
                position_revision: w[1],
                playing: w[2] != 0,
                position_seconds: f64::from_bits(w[3]),
                channel_fader: f32::from_bits(w[4] as u32),
                channel_fader_actual: f32::from_bits(w[5] as u32),
                launch_pending: w[6] != 0,
                launch_at_seconds: {
                    let at = f64::from_bits(w[7]);
                    at.is_finite().then_some(at)
                },
                channel_fader_ramp_active: w[8] != 0,
                launch_clock: (w[9] < 2).then_some(w[9] as usize),
            };
            return Some(crate::DirectControlReceipt {
                ticket: words[0],
                frame: words[1],
                deck: words[2] as usize,
                outcome: match words[3] {
                    0 => crate::DirectControlOutcome::Applied,
                    1 => crate::DirectControlOutcome::CancelledPending,
                    2 => crate::DirectControlOutcome::NoPendingLaunch,
                    _ => crate::DirectControlOutcome::Rejected,
                },
                failure: crate::CommandFailureCause::from_code(words[4]),
                before: state(&words[5..15]),
                after: state(&words[15..25]),
            });
        }
    }
    pub fn new() -> Self {
        Self {
            quality: QualityAtoms::new(),
            applied_fence: AtomicU64::new(0),
            guarded_control_receipts: std::array::from_fn(|_| AtomicU64::new(0)),
            direct_control_receipts: std::array::from_fn(|_| {
                std::array::from_fn(|_| AtomicU64::new(0))
            }),
            control_revision: AtomicU64::new(0),
            deck_control_revisions: std::array::from_fn(|_| AtomicU64::new(0)),
            target_control_revisions: std::array::from_fn(|_| AtomicU64::new(0)),
            batch: std::array::from_fn(|_| AtomicU64::new(0)),
            batch_events: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
            batch_updates: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
            load_revisions: std::array::from_fn(|_| AtomicU64::new(0)),
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
            sample_positions: std::array::from_fn(|_| double(f64::NAN)),
            active_ramps: std::array::from_fn(|_| AtomicBool::new(false)),
            decks: std::array::from_fn(|_| DeckAtoms::new()),
        }
    }
    pub fn publish(&self, m: &Mixer, channels: u16, frames: u32, master: [f32; 2], cue: [f32; 2]) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        for (atoms, receipt) in self
            .direct_control_receipts
            .iter()
            .zip(m.direct_control_receipts)
        {
            let Some(receipt) = receipt else { continue };
            if atoms[0].load(Ordering::Relaxed) == receipt.ticket {
                continue;
            }
            let mut words = [0; 25];
            words[..5].copy_from_slice(&[
                receipt.ticket,
                receipt.frame,
                receipt.deck as u64,
                match receipt.outcome {
                    crate::DirectControlOutcome::Applied => 0,
                    crate::DirectControlOutcome::CancelledPending => 1,
                    crate::DirectControlOutcome::NoPendingLaunch => 2,
                    crate::DirectControlOutcome::Rejected => 3,
                },
                receipt.failure.map_or(0, |f| f as u64),
            ]);
            for (chunk, state) in words[5..]
                .chunks_mut(10)
                .zip([receipt.before, receipt.after])
            {
                chunk.copy_from_slice(&[
                    state.load_revision,
                    state.position_revision,
                    state.playing as u64,
                    state.position_seconds.to_bits(),
                    state.channel_fader.to_bits() as u64,
                    state.channel_fader_actual.to_bits() as u64,
                    state.launch_pending as u64,
                    state.launch_at_seconds.unwrap_or(f64::NAN).to_bits(),
                    state.channel_fader_ramp_active as u64,
                    state.launch_clock.map_or(u64::MAX, |c| c as u64),
                ]);
            }
            for (atom, word) in atoms.iter().zip(words) {
                atom.store(word, Ordering::Relaxed);
            }
        }
        for (a, v) in self
            .guarded_control_receipts
            .iter()
            .zip(m.guarded_control_receipts)
        {
            a.store(v, Ordering::Release);
        }
        self.control_revision
            .store(m.control_revision, Ordering::Relaxed);
        for (a, v) in self
            .deck_control_revisions
            .iter()
            .zip(m.deck_control_revisions)
        {
            a.store(v, Ordering::Relaxed);
        }
        for (a, v) in self
            .target_control_revisions
            .iter()
            .zip(m.target_control_revisions)
        {
            a.store(v, Ordering::Relaxed);
        }
        if let Some(b) = &m.scheduled {
            let (decks, targets) = crate::scheduled::active_ownership(b);
            let values = [
                b.id,
                b.state.code(),
                b.reason as u64,
                b.receipts[..b.len].iter().filter(|r| r.state == 2).count() as u64,
                b.len as u64,
                b.frame,
                targets as u64,
                b.revision,
                b.open as u64,
                b.clock as u64,
                b.completion_carrier as u64,
                b.completion_event as u64,
                b.expires_frame,
                b.update_count as u64,
                b.command_queue as u64,
                decks as u64,
                (b.recovery_end != 0) as u64,
            ];
            for (a, v) in self.batch.iter().zip(values) {
                a.store(v, Ordering::Relaxed);
            }
            for (a, e) in self.batch_updates.iter().zip(b.updates) {
                for (slot, v) in a.iter().zip([
                    e.patch_id,
                    e.revision,
                    e.accepted as u64,
                    e.reason as u64,
                    e.frame,
                ]) {
                    slot.store(v, Ordering::Relaxed);
                }
            }
            for (a, e) in self.batch_events.iter().zip(b.receipts).take(b.len) {
                for (slot, v) in a.iter().zip([
                    e.step as u64,
                    e.state,
                    e.frame,
                    e.clock_seconds.to_bits(),
                    e.action_id,
                    e.completed_frame,
                    e.completed_clock_seconds[0].to_bits(),
                    e.completed_clock_seconds[1].to_bits(),
                    e.completed_load_revisions[0],
                    e.completed_load_revisions[1],
                    e.completed_grid_revisions[0],
                    e.completed_grid_revisions[1],
                    e.completed_position_revisions[0],
                    e.completed_position_revisions[1],
                    e.failure.map_or(0, |f| f.cause as u64),
                    e.failure.and_then(|f| f.deck).map_or(0, |d| d as u64 + 1),
                    e.failure.map_or(0, |f| f.action_id),
                    e.resolved_clock.map_or(0, |d| d as u64 + 1),
                    e.resolved_clock_seconds.unwrap_or(f64::NAN).to_bits(),
                    e.resolved_frame.unwrap_or(u64::MAX),
                    e.clock_source_headroom_seconds
                        .unwrap_or(f64::NAN)
                        .to_bits(),
                    e.ramp_end_frame.unwrap_or(u64::MAX),
                    e.curve_boundary_verified as u64,
                ]) {
                    slot.store(v, Ordering::Relaxed);
                }
            }
        } else {
            for a in &self.batch {
                a.store(0, Ordering::Relaxed);
            }
        }
        for i in 0..2 {
            self.decks[i]
                .grid_revision
                .store(m.grid_revisions[i], Ordering::Relaxed);
        }
        self.quality.publish(&m.quality);
        self.rate.store(m.sample_rate, Ordering::Relaxed);
        self.channels.store(channels as u32, Ordering::Relaxed);
        self.buffer.store(frames, Ordering::Relaxed);
        self.frames.store(m.rendered_frames, Ordering::Relaxed);
        for i in 0..2 {
            self.load_revisions[i].store(m.load_revisions[i], Ordering::Relaxed);
        }
        putf(&self.gain, m.master_gain.target);
        putf(&self.crossfader, m.crossfader.target);
        putf(&self.cue_gain, m.cue_gain.target);
        putf(&self.cue_mix, m.cue_mix.target);
        putf(&self.sampler, m.sampler_gain.target);
        for (i, active) in m.active_ramps().iter().enumerate() {
            self.active_ramps[i].store(*active, Ordering::Relaxed);
        }
        for i in 0..16 {
            putd(
                &self.sample_positions[i],
                m.sample_positions[i]
                    .map_or(f64::NAN, |p| p / m.samples[i].analysis.sample_rate as f64),
            );
        }
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
            putd(&a.actual_rate, d.actual_rate());
            putd(
                &a.playback_bpm,
                d.local_grid_bpm().map_or(f64::NAN, |bpm| bpm * d.rate()),
            );
            putd(&a.local_bpm, d.local_grid_bpm().unwrap_or(f64::NAN));
            putd(
                &a.actual_playback_bpm,
                d.local_grid_bpm()
                    .map_or(f64::NAN, |bpm| bpm * d.actual_rate()),
            );
            putd(
                &a.target_playback_bpm,
                d.target_playback_bpm().unwrap_or(f64::NAN),
            );
            putd(&a.tempo, d.tempo);
            a.fx_tail_active
                .store(m.main_fx[i].active(), Ordering::Relaxed);
            a.restricted_tail_active
                .store(m.main_fx[i].restricted, Ordering::Relaxed);
            for c in 0..2 {
                putf(&a.fx_tail_peak[c], m.main_fx[i].peak[c]);
            }
            a.normalization_enabled
                .store(d.normalization_enabled, Ordering::Relaxed);
            putf(
                &a.normalization_db,
                if d.normalization_enabled {
                    d.normalization_db
                } else {
                    0.
                },
            );
            putf(&a.gain, d.gain.target);
            putf(&a.fader, d.channel_fader.target);
            putf(&a.fader_actual, d.channel_fader.value);
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
            putd(&a.bpm, d.grid_bpm().unwrap_or(f64::NAN));
            putd(&a.grid, d.grid_offset);
            a.grid_valid.store(d.grid_valid, Ordering::Relaxed);
            a.grid_downbeat.store(d.grid_downbeat, Ordering::Relaxed);
            a.grid_manual.store(d.grid_manual, Ordering::Relaxed);
            a.grid_uniform.store(d.grid_uniform, Ordering::Relaxed);
            a.grid_anchor
                .store(d.grid_anchor_index as u64, Ordering::Relaxed);
            putd(
                &a.beat_position,
                if d.grid_valid {
                    d.grid_position(d.seconds())
                } else {
                    f64::NAN
                },
            );
            putd(
                &a.launch_at,
                m.launches[i].map_or(f64::NAN, |p| p.at_seconds),
            );
            a.launch_clock.store(
                m.launches[i].map_or(u64::MAX, |p| p.clock as u64),
                Ordering::Relaxed,
            );
            a.sync.store(d.sync_enabled, Ordering::Relaxed);
            putd(
                &a.sync_min_rate,
                d.sync_rate_limits.map_or(f64::NAN, |l| l.min_rate),
            );
            putd(
                &a.sync_max_rate,
                d.sync_rate_limits.map_or(f64::NAN, |l| l.max_rate),
            );
            putd(
                &a.sync_max_slew,
                d.sync_rate_limits
                    .map_or(f64::NAN, |l| l.max_slew_per_second),
            );
            a.sync_rate_limited
                .store(d.sync_rate_limited, Ordering::Relaxed);
            a.master.store(m.master_deck == i, Ordering::Relaxed);
            putd(&a.pitch, d.pitch);
            a.keylock.store(d.keylock, Ordering::Relaxed);
            a.pending.store(d.preparation_pending(), Ordering::Relaxed);
        }
        self.sequence.fetch_add(1, Ordering::Release);
        self.applied_fence.store(m.applied_fence, Ordering::Release);
    }
    pub fn read(&self, tracks: [Option<TrackAnalysis>; 2]) -> EngineSnapshot {
        loop {
            let before = self.sequence.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let batch: [u64; 17] = std::array::from_fn(|i| self.batch[i].load(Ordering::Relaxed));
            let mut result = EngineSnapshot {
                engine_epoch: self.stream_revision.load(Ordering::Relaxed),
                control_revision: self.control_revision.load(Ordering::Relaxed),
                deck_control_revisions: std::array::from_fn(|i| {
                    self.deck_control_revisions[i].load(Ordering::Relaxed)
                }),
                target_control_revisions: std::array::from_fn(|i| {
                    self.target_control_revisions[i].load(Ordering::Relaxed)
                }),
                scheduled_batch: crate::scheduled::ScheduledBatchSnapshot {
                    command_queue: batch[14] != 0,
                    recovery_applied: batch[16] != 0,
                    revision: batch[7],
                    open: batch[8] != 0,
                    clock: batch[9] as usize,
                    completion_carrier: batch[10] as usize,
                    completion_event: batch[11] as i64,
                    expires_frame: batch[12],
                    updates: self
                        .batch_updates
                        .iter()
                        .take((batch[13] as usize).min(crate::scheduled::MAX_QUEUE_UPDATES))
                        .map(|a| crate::scheduled::BatchPatchReceipt {
                            patch_id: a[0].load(Ordering::Relaxed),
                            revision: a[1].load(Ordering::Relaxed),
                            accepted: a[2].load(Ordering::Relaxed) != 0,
                            reason: crate::scheduled::BatchReason::from_code(
                                a[3].load(Ordering::Relaxed),
                            ),
                            frame: a[4].load(Ordering::Relaxed),
                        })
                        .collect(),
                    id: batch[0],
                    state: crate::scheduled::BatchLifecycle::from_code(batch[1]),
                    reason: crate::scheduled::BatchReason::from_code(batch[2]),
                    completed: batch[3] as usize,
                    total: batch[4] as usize,
                    frame: batch[5],
                    owned_targets: batch[6] as u32,
                    owned_decks: batch[15] as u8,
                    events: self
                        .batch_events
                        .iter()
                        .take(batch[4].min(crate::scheduled::MAX_SCHEDULED_STEPS as u64) as usize)
                        .map(|a| crate::scheduled::BatchEventReceipt {
                            step: a[0].load(Ordering::Relaxed) as usize,
                            action_id: a[4].load(Ordering::Relaxed),
                            completed_frame: a[5].load(Ordering::Relaxed),
                            completed_load_revisions: [
                                a[8].load(Ordering::Relaxed),
                                a[9].load(Ordering::Relaxed),
                            ],
                            completed_grid_revisions: [
                                a[10].load(Ordering::Relaxed),
                                a[11].load(Ordering::Relaxed),
                            ],
                            completed_position_revisions: [
                                a[12].load(Ordering::Relaxed),
                                a[13].load(Ordering::Relaxed),
                            ],
                            completed_clock_seconds: [
                                f64::from_bits(a[6].load(Ordering::Relaxed)),
                                f64::from_bits(a[7].load(Ordering::Relaxed)),
                            ],
                            state: a[1].load(Ordering::Relaxed),
                            frame: a[2].load(Ordering::Relaxed),
                            clock_seconds: f64::from_bits(a[3].load(Ordering::Relaxed)),
                            failure: crate::scheduled::CommandFailureCause::from_code(
                                a[14].load(Ordering::Relaxed),
                            )
                            .map(|cause| {
                                crate::scheduled::CommandFailure {
                                    cause,
                                    deck: a[15]
                                        .load(Ordering::Relaxed)
                                        .checked_sub(1)
                                        .map(|d| d as usize),
                                    action_id: a[16].load(Ordering::Relaxed),
                                }
                            }),
                            resolved_clock: a[17]
                                .load(Ordering::Relaxed)
                                .checked_sub(1)
                                .map(|d| d as usize),
                            resolved_clock_seconds: {
                                let v = f64::from_bits(a[18].load(Ordering::Relaxed));
                                v.is_finite().then_some(v)
                            },
                            resolved_frame: {
                                let v = a[19].load(Ordering::Relaxed);
                                (v != u64::MAX).then_some(v)
                            },
                            clock_source_headroom_seconds: {
                                let v = f64::from_bits(a[20].load(Ordering::Relaxed));
                                v.is_finite().then_some(v)
                            },
                            ramp_end_frame: {
                                let v = a[21].load(Ordering::Relaxed);
                                (v != u64::MAX).then_some(v)
                            },
                            curve_boundary_verified: a[22].load(Ordering::Relaxed) != 0,
                        })
                        .collect(),
                },
                audio_quality: self.quality.read(),
                load_revisions: std::array::from_fn(|i| {
                    self.load_revisions[i].load(Ordering::Relaxed)
                }),
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
                sampler_positions_seconds: std::array::from_fn(|i| {
                    let v = getd(&self.sample_positions[i]);
                    v.is_finite().then_some(v)
                }),
                active_ramps: std::array::from_fn(|i| self.active_ramps[i].load(Ordering::Relaxed)),
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

//! Independent native two-deck audio engine. UI and device controllers submit
//! bounded commands; file decoding and state serialization happen off the render thread.
#[cfg(test)]
mod automation_tests;
mod decode;
mod dsp;
mod effects;
mod embedded_metadata;
mod grid;
mod model;
mod processor;
mod protection;
#[cfg(test)]
mod quality_tests;
#[cfg(test)]
mod real_library_tests;
mod recording;
mod scheduled;
#[cfg(test)]
mod scheduled_tests;
mod snapshot;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, StreamConfig,
};
use decode::TrackData;
pub use decode::{
    analysis_cache_counts, analysis_current, analyze_beats, analyze_file, analyze_file_cached,
    analyze_file_with_beat_model, analyze_pcm_cached as analyze_decoded_audio_cached,
    apply_beat_model, beat_stage_current, embedded_artwork, playback_analysis_ready, probe_file,
    take_analysis_diagnostics, AnalysisDiagnostics, AudioSourceInfo,
};
use dsp::Mixer;
pub use embedded_metadata::{read_file_metadata, write_file_metadata, EmbeddedTag};
pub use model::*;
pub use recording::RecordingStatus;
use rtrb::{Consumer, Producer, RingBuffer};
pub use scheduled::{
    command_decks, command_queue_supported, event_seconds, validate_command_group,
    validate_queue_timing, BatchEventReceipt, BatchLifecycle, BatchPatchReceipt, BatchPrerequisite,
    BatchQueuePatch, BatchReason, BatchRecovery, BatchSchedule, CommandFailure,
    CommandFailureCause, CommandQueuePatch, QueueTiming, QueuedCommand, ScheduledBatchSnapshot,
    ScheduledCommand, ScheduledQueueAction,
};
use snapshot::SnapshotAtoms;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
pub use symphonia::core::io::MediaSource as EncodedAudioSource;

const QUEUE_CAPACITY: usize = 1024;
/// Validate a complete scheduled transition against a captured engine state.
/// This performs the same compilation checks as arming, without an audio device.
pub fn validate_scheduled_plan(
    schedule: &BatchSchedule,
    commands: Vec<ScheduledCommand>,
    snapshot: &EngineSnapshot,
) -> Result<(), String> {
    scheduled::compile(0, schedule, commands, snapshot).map(|_| ())
}

/// Opaque generation captured before an asynchronous provider begins preparing.
#[derive(Clone)]
pub struct SourceLoadTicket {
    deck: usize,
    generation: u64,
    owner: usize,
}
enum Retired {
    Grid(Arc<audio_analysis::NeuralRhythm>),
    Track(Arc<TrackData>),
    Recorder(recording::Recorder),
    Batch(Box<scheduled::NativeBatch>),
    BatchPatch(Box<scheduled::CompiledQueuePatch>),
}
// Cue arrays stay inline so the audio callback allocates and frees nothing.
#[allow(clippy::large_enum_variant)]
enum RenderCommand {
    ArmBatch(Box<scheduled::NativeBatch>),
    PatchBatch(Box<scheduled::CompiledQueuePatch>),
    CancelBatch {
        id: u64,
        stabilize: bool,
    },
    Fence(u64),
    UpdateGrid {
        deck: usize,
        revision: u64,
        grid: Arc<audio_analysis::NeuralRhythm>,
    },
    AnalysisComplete {
        deck: usize,
        revision: u64,
    },
    UpdateBaseGrid {
        deck: usize,
        revision: u64,
        bpm: Option<f64>,
        offset: Option<f64>,
        valid: bool,
    },
    Unload {
        revision: u64,
        deck: usize,
    },
    Control(EngineCommand),
    GuardedControl {
        command: EngineCommand,
        deck: usize,
        revision: u64,
        control_revision: Option<u64>,
        receipt_ticket: Option<u64>,
    },
    SessionEndpoint {
        engine_epoch: u64,
        control_revision: u64,
        receipt_ticket: u64,
    },
    Load {
        revision: u64,
        deck: usize,
        track: Arc<TrackData>,
    },
    InstantDouble {
        revision: u64,
        deck: usize,
        track: Arc<TrackData>,
    },
    LoadSample {
        slot: usize,
        track: Arc<TrackData>,
    },
    StartRecording(recording::Recorder),
    StopRecording,
}
struct RenderState {
    mixer: Mixer,
    commands: Consumer<RenderCommand>,
    retired: Producer<Retired>,
    recording: Option<recording::Recorder>,
    routing: AudioRouting,
    channels: u16,
}
impl RenderState {
    fn drain(&mut self) {
        // At most 64 commands per callback, with space reserved before popping an
        // asset-changing command. A full retirement ring defers work; never free PCM here.
        for _ in 0..64 {
            if self.retired.slots() < 4 {
                break;
            }
            let Ok(command) = self.commands.pop() else {
                break;
            };
            match command {
                RenderCommand::ArmBatch(batch) => {
                    if let Some(old) = self.mixer.arm_batch(batch) {
                        let _ = self.retired.push(Retired::Batch(old));
                    }
                }
                RenderCommand::PatchBatch(patch) => {
                    let patch = self.mixer.patch_batch(patch);
                    let _ = self.retired.push(Retired::BatchPatch(patch));
                }
                RenderCommand::CancelBatch { id, stabilize } => {
                    self.mixer.cancel_batch(id, stabilize)
                }
                RenderCommand::UpdateGrid {
                    deck,
                    revision,
                    grid,
                } => {
                    if self.mixer.load_revisions[deck] == revision {
                        if let Some(old) = self.mixer.update_grid(deck, grid) {
                            let _ = self.retired.push(Retired::Grid(old));
                        }
                    } else {
                        let _ = self.retired.push(Retired::Grid(grid));
                    }
                }
                RenderCommand::AnalysisComplete { deck, revision } => {
                    if self.mixer.load_revisions[deck] == revision {
                        self.mixer.decks[deck].analysis_pending = false;
                    }
                }
                RenderCommand::UpdateBaseGrid {
                    deck,
                    revision,
                    bpm,
                    offset,
                    valid,
                } => {
                    if self.mixer.load_revisions[deck] == revision {
                        self.mixer.update_base_grid(deck, bpm, offset, valid);
                    }
                }
                RenderCommand::Fence(sequence) => self.mixer.applied_fence = sequence,
                RenderCommand::Unload { deck, revision } => {
                    if let Some(old) = self.mixer.decks[deck].grid.take() {
                        let _ = self.retired.push(Retired::Grid(old));
                    }
                    self.mixer.load_revisions[deck] = revision;
                    for track in self.mixer.unload(deck).into_iter().flatten() {
                        let _ = self.retired.push(Retired::Track(track));
                    }
                }
                RenderCommand::Control(command) => self.mixer.command(command),
                RenderCommand::GuardedControl {
                    command,
                    deck,
                    revision,
                    control_revision,
                    receipt_ticket,
                } => {
                    let mut accepted = self.mixer.load_revisions[deck] == revision
                        && control_revision.is_none_or(|r| r == self.mixer.control_revision);
                    if accepted {
                        let before = self.mixer.control_revision;
                        self.mixer.command(command);
                        accepted = self.mixer.control_revision == before.wrapping_add(1);
                    }
                    if let Some(ticket) = receipt_ticket {
                        self.mixer.guarded_control_receipts[ticket as usize % 64] =
                            (ticket << 2) | if accepted { 1 } else { 2 };
                    }
                }
                RenderCommand::SessionEndpoint {
                    engine_epoch,
                    control_revision,
                    receipt_ticket,
                } => {
                    let accepted = self.mixer.engine_epoch == engine_epoch
                        && self.mixer.control_revision == control_revision;
                    if accepted {
                        for deck in 0..2 {
                            self.mixer.command(EngineCommand::CancelLaunch { deck });
                            self.mixer.command(EngineCommand::Pause { deck });
                        }
                    }
                    self.mixer.guarded_control_receipts[receipt_ticket as usize % 64] =
                        (receipt_ticket << 2) | if accepted { 1 } else { 2 };
                }
                RenderCommand::Load {
                    deck,
                    track,
                    revision,
                } => {
                    if let Some(old) = self.mixer.decks[deck].grid.take() {
                        let _ = self.retired.push(Retired::Grid(old));
                    }
                    self.mixer.load_revisions[deck] = revision;
                    if !track.analysis.recording_allowed {
                        if let Some(old) = self.recording.take() {
                            old.stats.active.store(false, Ordering::Release);
                            let _ = self.retired.push(Retired::Recorder(old));
                        }
                    }
                    if let Some(previous) = self.mixer.load(deck, track) {
                        let _ = self.retired.push(Retired::Track(previous));
                    }
                }
                RenderCommand::InstantDouble {
                    revision,
                    deck,
                    track,
                } => {
                    if let Some(old) = self.mixer.decks[deck].grid.take() {
                        let _ = self.retired.push(Retired::Grid(old));
                    }
                    if !track.analysis.recording_allowed {
                        if let Some(old) = self.recording.take() {
                            old.stats.active.store(false, Ordering::Release);
                            let _ = self.retired.push(Retired::Recorder(old));
                        }
                    }
                    self.mixer.load_revisions[deck] = revision;
                    for old in self.mixer.instant_double(deck, track).into_iter().flatten() {
                        let _ = self.retired.push(Retired::Track(old));
                    }
                }
                RenderCommand::StartRecording(recorder) => {
                    if self.mixer.main_fx.iter().any(|fx| fx.restricted)
                        || self.mixer.decks.iter().any(|d| {
                            d.track
                                .as_ref()
                                .is_some_and(|t| !t.analysis.recording_allowed)
                        })
                    {
                        recorder.stats.active.store(false, Ordering::Release);
                        let _ = self.retired.push(Retired::Recorder(recorder));
                        continue;
                    }
                    if let Some(old) = self.recording.replace(recorder) {
                        old.stats.active.store(false, Ordering::Release);
                        let _ = self.retired.push(Retired::Recorder(old));
                    }
                }
                RenderCommand::StopRecording => {
                    if let Some(old) = self.recording.take() {
                        old.stats.active.store(false, Ordering::Release);
                        let _ = self.retired.push(Retired::Recorder(old));
                    }
                }
                RenderCommand::LoadSample { slot, track } => {
                    let previous = std::mem::replace(&mut self.mixer.samples[slot], track);
                    self.mixer.sample_positions[slot] = None;
                    let _ = self.retired.push(Retired::Track(previous));
                }
            }
        }
    }
    fn render<T: cpal::SizedSample + cpal::FromSample<f32>>(
        &mut self,
        output: &mut [T],
        atoms: &SnapshotAtoms,
    ) {
        let started = std::time::Instant::now();
        self.drain();
        self.mixer.quality.master_input_peak = [0.; 2];
        self.mixer.quality.cue_input_peak = [0.; 2];
        let channels = self.channels as usize;
        let mut master_peak = [0.0_f32; 2];
        let mut cue_peak = [0.0_f32; 2];
        let mut deck_peak = [[0.0_f32; 2]; 2];
        for frame in output.chunks_exact_mut(channels) {
            let (raw_master, raw_cue) = self.mixer.frame();
            let master_input = raw_master.map(|v| v * protection::HEADROOM);
            let cue_input = raw_cue.map(|v| v * protection::HEADROOM);
            for c in 0..2 {
                if master_input[c].is_finite() {
                    self.mixer.quality.master_input_peak[c] =
                        self.mixer.quality.master_input_peak[c].max(master_input[c].abs());
                }
                if cue_input[c].is_finite() {
                    self.mixer.quality.cue_input_peak[c] =
                        self.mixer.quality.cue_input_peak[c].max(cue_input[c].abs());
                }
            }
            let master = self
                .mixer
                .master_protection
                .frame(master_input, self.mixer.sample_rate);
            let cue = self
                .mixer
                .cue_protection
                .frame(cue_input, self.mixer.sample_rate);
            if let Some(recorder) = self.recording.as_mut() {
                if recorder.stats.active.load(Ordering::Relaxed)
                    && recorder.producer.push(master).is_err()
                {
                    recorder.stats.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            for (deck, peaks) in deck_peak.iter_mut().enumerate() {
                for (channel, peak) in peaks.iter_mut().enumerate() {
                    *peak = peak.max(self.mixer.decks[deck].peak[channel].abs());
                }
            }
            for c in 0..2 {
                master_peak[c] = master_peak[c].max(master[c].abs());
                cue_peak[c] = cue_peak[c].max(cue[c].abs());
            }
            for sample in frame.iter_mut() {
                *sample = T::from_sample(0.0_f32);
            }
            frame[self.routing.master_left as usize] = T::from_sample(master[0].clamp(-1.0, 1.0));
            frame[self.routing.master_right as usize] = T::from_sample(master[1].clamp(-1.0, 1.0));
            if let (Some(left), Some(right)) = (self.routing.cue_left, self.routing.cue_right) {
                frame[left as usize] = T::from_sample(cue[0].clamp(-1.0, 1.0));
                frame[right as usize] = T::from_sample(cue[1].clamp(-1.0, 1.0));
            }
        }
        for (deck, peak) in deck_peak.iter().enumerate() {
            self.mixer.decks[deck].peak = *peak;
        }
        let quality = &mut self.mixer.quality;
        quality.headroom_db = 6.;
        quality.output_latency_frames = self.mixer.master_protection.latency_frames() as u32;
        quality.master_gain_reduction_db = self.mixer.master_protection.reduction_db();
        quality.cue_gain_reduction_db = self.mixer.cue_protection.reduction_db();
        quality.master_limited_frames = self.mixer.master_protection.overloaded_frames;
        quality.cue_limited_frames = self.mixer.cue_protection.overloaded_frames;
        quality.invalid_signal_frames =
            self.mixer.master_protection.invalid_frames + self.mixer.cue_protection.invalid_frames;
        quality.processor_resets = self.mixer.decks.each_ref().map(|d| d.processor.resets);
        quality.processor_blocks = self.mixer.decks.each_ref().map(|d| d.processor.blocks);
        let elapsed = started.elapsed().as_nanos() as u64;
        quality.callback_count += 1;
        quality.callback_max_ns = quality.callback_max_ns.max(elapsed);
        let budget =
            (output.len() / channels) as u64 * 1_000_000_000 / self.mixer.sample_rate as u64;
        if elapsed > budget {
            quality.callback_deadline_exceeded += 1;
        }
        atoms.publish(
            &self.mixer,
            self.channels,
            (output.len() / channels) as u32,
            master_peak,
            cue_peak,
        );
    }
}

/// Atomic transfer of exclusive ownership, not a shared mutable DSP object.
/// CPAL callbacks take and return the same preallocated Box. The lifecycle thread
/// accesses it only before building a stream or after dropping that stream. Swap
/// also guards against an unexpected concurrent/reentrant callback: it outputs silence.
struct StateCell {
    state: AtomicPtr<RenderState>,
}
impl StateCell {
    fn new(state: Box<RenderState>) -> Self {
        Self {
            state: AtomicPtr::new(Box::into_raw(state)),
        }
    }
    fn take(&self) -> Option<Box<RenderState>> {
        let pointer = self.state.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if pointer.is_null() {
            None
        } else {
            // SAFETY: swap grants exclusive ownership of a Box originally installed
            // by put/new, and another taker observes null until ownership is returned.
            Some(unsafe { Box::from_raw(pointer) })
        }
    }
    fn put(&self, state: Box<RenderState>) {
        debug_assert!(self.state.load(Ordering::Acquire).is_null());
        self.state.store(Box::into_raw(state), Ordering::Release);
    }
}
impl Drop for StateCell {
    fn drop(&mut self) {
        let pointer = *self.state.get_mut();
        if !pointer.is_null() {
            // SAFETY: final Arc owner, no concurrent callbacks or lifecycle users remain.
            drop(unsafe { Box::from_raw(pointer) });
        }
    }
}
struct Shared {
    next_fence: AtomicU64,
    next_control_receipt: AtomicU64,
    analysis_pending: [AtomicBool; 2],
    metadata_revision: AtomicU64,
    committed_load_revision: [AtomicU64; 2],
    load_generation: [AtomicU64; 2],
    recording: Mutex<Option<recording::RecordingJob>>,
    sources: Mutex<[Option<Arc<TrackData>>; 2]>,
    producer: Mutex<Producer<RenderCommand>>,
    tracks: Mutex<[Option<TrackAnalysis>; 2]>,
    samples: Mutex<[Option<TrackAnalysis>; 16]>,
    atoms: Arc<SnapshotAtoms>,
    lifecycle: mpsc::Sender<Lifecycle>,
}
enum Lifecycle {
    Start {
        device: Option<String>,
        buffer: Option<u32>,
        routing: AudioRouting,
        reply: mpsc::Sender<Result<AudioStatus, String>>,
    },
    Stop {
        reply: mpsc::Sender<()>,
    },
    Shutdown,
}
/// Safe to keep in application state. CPAL's non-Send native stream lives on its
/// own lifecycle thread, never in a Tauri command or in the webview.
pub(crate) fn analysis_gated_command(command: &EngineCommand) -> bool {
    matches!(
        command,
        EngineCommand::Play { .. }
            | EngineCommand::TogglePlay { .. }
            | EngineCommand::PlayQuantized { .. }
            | EngineCommand::TriggerHotCue { .. }
            | EngineCommand::CuePress { .. }
            | EngineCommand::Jog { .. }
            | EngineCommand::ScrubTo { .. }
    )
}
pub struct EngineHandle {
    shared: Arc<Shared>,
}
impl EngineHandle {
    pub fn new() -> Result<Self, String> {
        let (producer, consumer) = RingBuffer::new(QUEUE_CAPACITY);
        let (retired, mut retire_consumer) = RingBuffer::new(QUEUE_CAPACITY * 2);
        let atoms = Arc::new(SnapshotAtoms::new());
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |t| t.as_micros() as u64);
        atoms.stream_revision.store(epoch, Ordering::Release);
        let (render_tx, render_rx) = mpsc::channel();
        let mut mixer = Mixer::new();
        mixer.engine_epoch = epoch;
        let samples = std::array::from_fn(|i| Some(mixer.samples[i].analysis.clone()));
        let state = Arc::new(StateCell::new(Box::new(RenderState {
            mixer,
            commands: consumer,
            retired,
            recording: None,
            routing: AudioRouting::default(),
            channels: 2,
        })));
        let shutdown = Arc::new(AtomicBool::new(false));
        let retire_shutdown = shutdown.clone();
        thread::Builder::new()
            .name("dj-pcm-reclaim".into())
            .spawn(move || loop {
                while let Ok(retired) = retire_consumer.pop() {
                    match retired {
                        Retired::Track(track) => drop(track),
                        Retired::Grid(grid) => drop(grid),
                        Retired::Recorder(recorder) => drop(recorder),
                        Retired::Batch(batch) => drop(batch),
                        Retired::BatchPatch(patch) => drop(patch),
                    }
                }
                if retire_shutdown.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(Duration::from_millis(4));
            })
            .map_err(|e| format!("Cannot start asset worker: {e}"))?;
        let lifecycle_atoms = atoms.clone();
        thread::Builder::new()
            .name("dj-audio-device".into())
            .spawn(move || audio_thread(render_rx, state, lifecycle_atoms, shutdown))
            .map_err(|e| format!("Cannot start audio worker: {e}"))?;
        let shared = Arc::new(Shared {
            next_fence: AtomicU64::new(1),
            next_control_receipt: AtomicU64::new(1),
            analysis_pending: std::array::from_fn(|_| AtomicBool::new(false)),
            metadata_revision: AtomicU64::new(0),
            committed_load_revision: std::array::from_fn(|_| AtomicU64::new(0)),
            load_generation: std::array::from_fn(|_| AtomicU64::new(0)),
            recording: Mutex::new(None),
            sources: Mutex::new([None, None]),
            producer: Mutex::new(producer),
            tracks: Mutex::new([None, None]),
            samples: Mutex::new(samples),
            atoms,
            lifecycle: render_tx,
        });
        Ok(Self { shared })
    }
    pub fn devices(&self) -> Result<Vec<AudioDeviceInfo>, String> {
        devices()
    }
    pub fn start_audio(
        &self,
        device_name: Option<String>,
        buffer_frames: Option<u32>,
        routing: AudioRouting,
    ) -> Result<AudioStatus, String> {
        // Serialize with recording creation until the device change has finished.
        let recording = self
            .shared
            .recording
            .lock()
            .map_err(|_| "Recording state poisoned")?;
        if recording
            .as_ref()
            .is_some_and(|job| job.stats.active.load(Ordering::Acquire))
        {
            return Err("Stop recording before changing audio output".into());
        }
        let (tx, rx) = mpsc::channel();
        self.shared
            .lifecycle
            .send(Lifecycle::Start {
                device: device_name,
                buffer: buffer_frames,
                routing,
                reply: tx,
            })
            .map_err(|_| "Audio worker stopped")?;
        let result = rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "Audio device did not respond within 10 seconds".to_string())?;
        drop(recording);
        result
    }
    pub fn start_recording(&self, path: &Path) -> Result<RecordingStatus, String> {
        self.start_recording_with_policy(path, false)
    }
    pub fn start_recording_with_policy(
        &self,
        path: &Path,
        overwrite: bool,
    ) -> Result<RecordingStatus, String> {
        if self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?
            .iter()
            .flatten()
            .any(|t| !t.recording_allowed)
        {
            return Err(
                "Recording is unavailable while a streaming-service track is loaded".into(),
            );
        }
        if self
            .transport_snapshot()
            .decks
            .iter()
            .any(|d| d.restricted_tail_active)
        {
            return Err(
                "Recording is unavailable while a streaming-service effect tail is active".into(),
            );
        }
        let mut current = self
            .shared
            .recording
            .lock()
            .map_err(|_| "Recording state poisoned")?;
        if current
            .as_ref()
            .is_some_and(|j| j.stats.active.load(Ordering::Acquire))
        {
            return Err("Recording is already active".into());
        }
        if let Some(mut old) = current.take() {
            if let Some(worker) = old.worker.take() {
                let _ = worker.join();
            }
        }
        let (recorder, job) =
            recording::create_with_policy(path, self.snapshot().sample_rate, overwrite)?;
        if let Err(error) = self.enqueue(RenderCommand::StartRecording(recorder)) {
            job.stats.active.store(false, Ordering::Release);
            if let Some(worker) = job.worker {
                let _ = worker.join();
            }
            return Err(error);
        }
        let status = job.stats.status();
        *current = Some(job);
        Ok(status)
    }
    pub fn recording_status(&self) -> RecordingStatus {
        self.shared
            .recording
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(
                RecordingStatus {
                    active: false,
                    path: None,
                    sample_rate: 0,
                    frames_written: 0,
                    dropped_frames: 0,
                    error: None,
                },
                |job| job.stats.status(),
            )
    }
    pub fn stop_recording(&self) -> Result<RecordingStatus, String> {
        self.enqueue(RenderCommand::StopRecording)?;
        if !self.snapshot().running {
            self.stop_audio();
        }
        let mut current = self
            .shared
            .recording
            .lock()
            .map_err(|_| "Recording state poisoned")?;
        let Some(job) = current.as_mut() else {
            return Ok(RecordingStatus {
                active: false,
                path: None,
                sample_rate: 0,
                frames_written: 0,
                dropped_frames: 0,
                error: None,
            });
        };
        for _ in 0..500 {
            if !job.stats.active.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if job.stats.active.load(Ordering::Acquire) {
            return Err(
                "Audio callback did not stop recording; stop the audio device and retry".into(),
            );
        }
        if let Some(worker) = job.worker.take() {
            worker.join().map_err(|_| "Recording writer failed")?;
        }
        Ok(job.stats.status())
    }
    pub fn scheduled_snapshot(&self) -> Result<EngineSnapshot, String> {
        let metadata = self.metadata_snapshot();
        let mut snapshot = self.transport_snapshot();
        if metadata.load_revisions != snapshot.load_revisions {
            return Err(
                "Loaded source revision conflict: source metadata is still being applied".into(),
            );
        }
        for (d, track) in snapshot.decks.iter_mut().zip(metadata.tracks) {
            d.track = track;
        }
        Ok(snapshot)
    }
    pub fn validate_scheduled_batch(
        &self,
        schedule: &BatchSchedule,
        commands: Vec<ScheduledCommand>,
    ) -> Result<(), String> {
        validate_scheduled_plan(schedule, commands, &self.scheduled_snapshot()?)
    }
    pub fn arm_scheduled_batch(
        &self,
        id: u64,
        schedule: &BatchSchedule,
        commands: Vec<ScheduledCommand>,
    ) -> Result<(), String> {
        let batch = scheduled::compile(id, schedule, commands, &self.scheduled_snapshot()?)?;
        self.enqueue(RenderCommand::ArmBatch(batch))
    }
    pub fn arm_command_queue(&self, id: u64) -> Result<(), String> {
        self.enqueue(RenderCommand::ArmBatch(scheduled::compile_command_queue(
            id,
            &self.transport_snapshot(),
        )?))
    }
    pub fn validate_command_queue_patch(
        &self,
        id: u64,
        patch: &CommandQueuePatch,
    ) -> Result<(), String> {
        scheduled::compile_command_patch(id, patch.clone(), &self.command_queue_snapshot(patch)?)
            .map(|_| ())
    }
    pub fn patch_command_queue(&self, id: u64, patch: CommandQueuePatch) -> Result<(), String> {
        let snapshot = self.command_queue_snapshot(&patch)?;
        self.enqueue(RenderCommand::PatchBatch(scheduled::compile_command_patch(
            id, patch, &snapshot,
        )?))
    }
    fn command_queue_snapshot(&self, patch: &CommandQueuePatch) -> Result<EngineSnapshot, String> {
        if patch.append.iter().any(|a| {
            matches!(
                a.when,
                QueueTiming::AtEvent { .. }
                    | QueueTiming::AtSeconds { .. }
                    | QueueTiming::AfterBeats { .. }
            )
        }) {
            self.scheduled_snapshot()
        } else {
            Ok(self.transport_snapshot())
        }
    }
    pub fn validate_scheduled_patch(&self, id: u64, patch: &BatchQueuePatch) -> Result<(), String> {
        scheduled::compile_patch(id, patch.clone(), &self.scheduled_snapshot()?).map(|_| ())
    }
    pub fn patch_scheduled_batch(&self, id: u64, patch: BatchQueuePatch) -> Result<(), String> {
        let patch = scheduled::compile_patch(id, patch, &self.scheduled_snapshot()?)?;
        self.enqueue(RenderCommand::PatchBatch(patch))
    }
    pub fn cancel_scheduled_batch(&self, id: u64, stabilize: bool) -> Result<(), String> {
        self.enqueue(RenderCommand::CancelBatch { id, stabilize })
    }
    pub fn stop_audio(&self) {
        let (tx, rx) = mpsc::channel();
        if self
            .shared
            .lifecycle
            .send(Lifecycle::Stop { reply: tx })
            .is_ok()
        {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
    }
    /// A fence acknowledges preceding control application, including while
    /// audio is stopped. Native continuous pitch processing needs no asynchronous preparation.
    pub fn fence(&self) -> Result<u64, String> {
        let mut producer = self
            .shared
            .producer
            .lock()
            .map_err(|_| "Command queue poisoned")?;
        let sequence = self.shared.next_fence.fetch_add(1, Ordering::Relaxed);
        producer
            .push(RenderCommand::Fence(sequence))
            .map_err(|_| "Audio command queue is full; retry shortly")?;
        Ok(sequence)
    }
    pub fn wait_for_fence(&self, sequence: u64, timeout: Duration) -> Result<(), String> {
        let started = std::time::Instant::now();
        while self.shared.atoms.applied_fence.load(Ordering::Acquire) < sequence {
            if started.elapsed() >= timeout {
                return Err("Timed out waiting for command application".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
    pub fn command(&self, command: EngineCommand) -> Result<(), String> {
        self.command_for_revision(command, None)
    }
    /// Check preparation before native command side effects (such as manual
    /// takeover cancellation). The renderer repeats this gate for queued races.
    pub fn ensure_prepared(&self, command: &EngineCommand) -> Result<(), String> {
        if analysis_gated_command(command)
            && command
                .deck()
                .is_some_and(|deck| self.shared.analysis_pending[deck].load(Ordering::Acquire))
        {
            return Err(
                "Track analysis is not complete; wait for BPM, key and waveform preparation".into(),
            );
        }
        Ok(())
    }
    pub fn command_for_revision(
        &self,
        command: EngineCommand,
        expected: Option<u64>,
    ) -> Result<(), String> {
        self.command_with_revisions(command, expected, None)
    }
    /// Apply a preparation control only while the observed controls still belong
    /// to the caller. The render thread repeats the check so queued manual input
    /// wins over preparation that was computed from an older snapshot.
    pub fn command_for_control_revision(
        &self,
        command: EngineCommand,
        expected_load_revision: Option<u64>,
        expected_control_revision: u64,
    ) -> Result<u64, String> {
        if command.deck().is_none() {
            return Err("Preparation control guards require a deck command".into());
        }
        let ticket = self
            .shared
            .next_control_receipt
            .fetch_add(1, Ordering::Relaxed);
        self.command_with_revisions(
            command,
            expected_load_revision,
            Some((expected_control_revision, ticket)),
        )?;
        Ok(ticket)
    }
    /// Atomically finish an application-owned session. Source loads may change,
    /// but manual controls and audio restarts revoke this queued endpoint.
    /// Consume the returned ticket with `wait_for_control_receipt`.
    pub fn pause_transports_for_session(
        &self,
        expected_engine_epoch: u64,
        expected_control_revision: u64,
    ) -> Result<u64, String> {
        let snapshot = self.transport_snapshot();
        if snapshot.engine_epoch != expected_engine_epoch
            || snapshot.control_revision != expected_control_revision
        {
            return Err("Session endpoint ownership changed".into());
        }
        let ticket = self
            .shared
            .next_control_receipt
            .fetch_add(1, Ordering::Relaxed);
        self.enqueue(RenderCommand::SessionEndpoint {
            engine_epoch: expected_engine_epoch,
            control_revision: expected_control_revision,
            receipt_ticket: ticket,
        })?;
        Ok(ticket)
    }
    /// Wait for the render-thread decision, rather than inferring acceptance from
    /// a revision that unrelated manual input could also have incremented.
    /// Receipts are bounded; callers must consume them before 64 newer controls.
    pub fn wait_for_control_receipt(&self, ticket: u64, timeout: Duration) -> Result<(), String> {
        if ticket == 0 || ticket >= (1 << 62) {
            return Err("Invalid control receipt ticket".into());
        }
        let started = std::time::Instant::now();
        loop {
            let receipt = self.shared.atoms.guarded_control_receipts[ticket as usize % 64]
                .load(Ordering::Acquire);
            if receipt >> 2 == ticket {
                return if receipt & 3 == 1 {
                    Ok(())
                } else {
                    Err(
                        "Control revision conflict: guarded command rejected before application"
                            .into(),
                    )
                };
            }
            if receipt >> 2 > ticket {
                return Err("Control receipt expired".into());
            }
            if started.elapsed() >= timeout {
                return Err("Timed out waiting for guarded command application".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn command_with_revisions(
        &self,
        mut command: EngineCommand,
        expected: Option<u64>,
        guard: Option<(u64, u64)>,
    ) -> Result<(), String> {
        command.validate()?;
        self.ensure_prepared(&command)?;
        let transport = if matches!(
            command,
            EngineCommand::Play { .. }
                | EngineCommand::TogglePlay { .. }
                | EngineCommand::PlayQuantized { .. }
        ) {
            self.snapshot()
        } else {
            self.transport_snapshot()
        };
        if guard.is_some_and(|(r, _)| r != transport.control_revision) {
            return Err("Control revision conflict: manual controls changed".into());
        }
        if let EngineCommand::Play { deck } | EngineCommand::TogglePlay { deck } = command {
            let d = &transport.decks[deck];
            if d.launch_pending {
                if matches!(command, EngineCommand::TogglePlay { .. }) {
                    command = EngineCommand::CancelLaunch { deck };
                } else {
                    if guard.is_none() {
                        return Ok(());
                    }
                    return Err("Launch already pending; guarded control was not applied".into());
                }
            } else if !d.playing && d.sync_enabled {
                let clock = transport
                    .decks
                    .iter()
                    .position(|d| d.is_master)
                    .unwrap_or(1 - deck);
                if clock != deck && transport.decks[clock].playing {
                    command = EngineCommand::PlayQuantized {
                        deck,
                        clock,
                        clock_revision: transport.load_revisions[clock],
                        at_seconds: None,
                        quantize: 1.0,
                    };
                }
            }
        }
        if let EngineCommand::SetPlaybackBpm { deck, bpm } = command {
            playback_tempo_percent(transport.decks[deck].local_bpm, bpm).map_err(str::to_owned)?;
        }
        if let EngineCommand::SetSyncRateLimits {
            deck,
            limits: Some(limits),
        } = command
        {
            if !limits.contains(transport.decks[deck].actual_playback_rate) {
                return Err("Prepare the deck's actual playback rate inside the Sync limits before applying them".into());
            }
        }
        if let EngineCommand::Sync { deck }
        | EngineCommand::SetSync {
            deck,
            enabled: true,
        } = command
        {
            let master = transport
                .decks
                .iter()
                .position(|d| d.is_master)
                .unwrap_or(0);
            if master == deck {
                return Err("The master deck cannot sync to itself; select the playing leader as master first".into());
            }
            let clock = master;
            if transport.decks[deck].playing
                && (!transport.decks[clock].playing || transport.decks[clock].jog_touch)
            {
                return Err(
                    "Live beat sync requires a playing master with its jog released".into(),
                );
            }
            if !transport.decks[deck].beatgrid_valid || !transport.decks[clock].beatgrid_valid {
                return Err(
                    "Beat sync requires detected or manually aligned grids on both decks".into(),
                );
            }
        }
        if let EngineCommand::PlayQuantized {
            deck,
            clock,
            clock_revision,
            at_seconds,
            quantize,
        } = command
        {
            let a = &transport.decks[clock];
            let b = &transport.decks[deck];
            if self.load_revision(clock) != clock_revision {
                return Err("Loaded source revision conflict on launch clock".into());
            }
            if !transport.running || !a.playing || b.playing {
                return Err("Quantized launch requires running audio, a playing clock and a paused incoming deck".into());
            }
            if !a.beatgrid_valid || !b.beatgrid_valid {
                return Err(
                    "Quantized launch requires detected or manually aligned grids on both decks"
                        .into(),
                );
            }
            if a.jog_touch
                || b.jog_touch
                || a.loop_start_seconds.is_some()
                || b.loop_start_seconds.is_some()
            {
                return Err("Release jog holds and loops before scheduling a launch".into());
            }
            if b.preparation_pending {
                return Err(
                    "Wait for incoming tempo/pitch preparation before scheduling a launch".into(),
                );
            }
            let beat = a
                .grid_position(a.position_seconds)
                .ok_or("Clock grid is unavailable")?;
            let at = at_seconds.unwrap_or(
                a.grid_time(((beat / quantize).floor() + 1.0) * quantize)
                    .ok_or("Clock grid is unavailable")?,
            );
            if at <= a.position_seconds
                || (a.snapped_grid_time(at).ok_or("Clock grid is unavailable")? - at).abs() > 0.001
            {
                return Err("Launch timestamp must be a future beat on the clock grid".into());
            }
            let entry = b
                .snapped_grid_time(b.position_seconds)
                .ok_or("Incoming grid is unavailable")?;
            let tracks = self
                .shared
                .tracks
                .lock()
                .map_err(|_| "Track cache poisoned")?;
            if tracks[clock]
                .as_ref()
                .is_none_or(|t| at >= t.duration_seconds)
                || tracks[deck]
                    .as_ref()
                    .is_none_or(|t| entry < 0.0 || entry >= t.duration_seconds)
            {
                return Err(
                    "Launch clock and snapped incoming entry must lie inside their tracks".into(),
                );
            }
            // Explicit phrase timestamps stay fixed; next-boundary requests
            // resolve inside the renderer so queue latency cannot miss a beat.
        }
        let command_deck = command.deck();
        let mut producer = self
            .shared
            .producer
            .lock()
            .map_err(|_| "Command queue poisoned")?;
        let render = if let Some(deck) = command_deck {
            let revision = self.load_revision(deck);
            if expected.is_some_and(|r| r != revision) {
                return Err("Loaded source revision conflict".into());
            }
            RenderCommand::GuardedControl {
                command,
                deck,
                revision,
                control_revision: guard.map(|(r, _)| r),
                receipt_ticket: guard.map(|(_, ticket)| ticket),
            }
        } else {
            RenderCommand::Control(command)
        };
        producer
            .push(render)
            .map_err(|_| "Audio command queue is full; retry shortly".into())
    }
    fn enqueue(&self, command: RenderCommand) -> Result<(), String> {
        self.shared
            .producer
            .lock()
            .map_err(|_| "Command queue poisoned")?
            .push(command)
            .map_err(|_| "Audio command queue is full; retry shortly".into())
    }
    pub fn load_track(&self, deck: usize, path: &Path) -> Result<TrackAnalysis, String> {
        self.load_track_cached(deck, path, None)
    }
    /// Load local audio for playback, reusing content-verified stored analysis.
    /// Missing analysis is completed separately with `complete_loaded_analysis`;
    /// transport commands remain gated until that completion commits.
    pub fn load_track_cached(
        &self,
        deck: usize,
        path: &Path,
        cached: Option<TrackAnalysis>,
    ) -> Result<TrackAnalysis, String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        let generation = self.begin_load(deck)?;
        let track = decode::decode_file_playable(path, cached)?;
        self.load_data(deck, track, generation)
    }
    /// A named application preparation retains the ticket so stop/deadline can
    /// invalidate only this generation while decoding happens off-thread.
    pub fn load_track_cached_for_ticket(
        &self,
        ticket: SourceLoadTicket,
        path: &Path,
        cached: Option<TrackAnalysis>,
    ) -> Result<TrackAnalysis, String> {
        if ticket.owner != Arc::as_ptr(&self.shared) as usize {
            return Err("Source load belongs to another engine".into());
        }
        if self.shared.load_generation[ticket.deck].load(Ordering::Acquire) != ticket.generation {
            return Err("A newer load request replaced this source".into());
        }
        let track = decode::decode_file_playable(path, cached)?;
        self.load_data(ticket.deck, track, ticket.generation)
    }
    pub fn cancel_source_load(&self, ticket: &SourceLoadTicket) -> Result<bool, String> {
        if ticket.owner != Arc::as_ptr(&self.shared) as usize {
            return Err("Source load belongs to another engine".into());
        }
        let _tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        Ok(self.shared.load_generation[ticket.deck]
            .compare_exchange(
                ticket.generation,
                ticket.generation.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok())
    }
    /// Load the other deck's RAM asset without decoding, then copy its actual
    /// position and transport atomically at the render callback boundary.
    pub fn instant_double(&self, deck: usize) -> Result<(), String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        let source_deck = 1 - deck;
        let mut tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        let metadata = tracks[source_deck]
            .clone()
            .ok_or("Other deck has no loaded track")?;
        let mut producer = self
            .shared
            .producer
            .lock()
            .map_err(|_| "Command queue poisoned")?;
        if producer.slots() == 0 {
            return Err("Audio command queue is full; retry shortly".into());
        }
        let mut sources = self
            .shared
            .sources
            .lock()
            .map_err(|_| "Source cache poisoned")?;
        let source = sources[source_deck]
            .clone()
            .ok_or("Other deck audio source is unavailable")?;
        // Producer lock makes the asset plus transport command a single queue item.
        producer
            .push(RenderCommand::InstantDouble {
                revision: self.load_revision(deck) + 1,
                deck,
                track: source,
            })
            .map_err(|_| "Audio command queue is full; retry shortly")?;
        self.shared.load_generation[deck].fetch_add(1, Ordering::AcqRel);
        sources[deck] = sources[source_deck].clone();
        self.shared.analysis_pending[deck].store(
            self.shared.analysis_pending[source_deck].load(Ordering::Acquire),
            Ordering::Release,
        );
        tracks[deck] = Some(metadata);
        self.shared.committed_load_revision[deck].fetch_add(1, Ordering::Release);
        self.shared
            .metadata_revision
            .fetch_add(1, Ordering::Release);
        Ok(())
    }
    /// Feed an authorized provider's Read/Seek source into the native deck.
    /// Call from a worker: this first decodes the complete source into RAM.
    /// Provider access, expiry, reporting and capability enforcement belong to
    /// the integration; this method supplies no Spotify authentication or SDK.
    pub fn load_source(
        &self,
        deck: usize,
        source: Box<dyn EncodedAudioSource>,
        info: AudioSourceInfo,
    ) -> Result<TrackAnalysis, String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        let generation = self.begin_load(deck)?;
        let track = decode::decode_source(source, info)?;
        self.load_data(deck, track, generation)
    }
    pub fn begin_source_load(&self, deck: usize) -> Result<SourceLoadTicket, String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        Ok(SourceLoadTicket {
            deck,
            generation: self.begin_load(deck)?,
            owner: Arc::as_ptr(&self.shared) as usize,
        })
    }
    pub fn unload(&self, deck: usize) -> Result<(), String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        let mut tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        let mut producer = self
            .shared
            .producer
            .lock()
            .map_err(|_| "Command queue poisoned")?;
        if producer.slots() == 0 {
            return Err("Audio command queue is full; retry shortly".into());
        }
        let mut sources = self
            .shared
            .sources
            .lock()
            .map_err(|_| "Source cache poisoned")?;
        self.shared.load_generation[deck].fetch_add(1, Ordering::AcqRel);
        sources[deck] = None;
        self.shared.analysis_pending[deck].store(false, Ordering::Release);
        tracks[deck] = None;
        self.shared.committed_load_revision[deck].fetch_add(1, Ordering::Release);
        self.shared
            .metadata_revision
            .fetch_add(1, Ordering::Release);
        producer
            .push(RenderCommand::Unload {
                deck,
                revision: self.load_revision(deck),
            })
            .map_err(|_| "Audio command queue is full; retry shortly".into())
    }
    /// Accept normal decoded provider output on a worker. No SDK or network runs
    /// in the audio callback. The same native clocks, cues and DSP then apply.
    pub fn load_decoded_source(
        &self,
        ticket: SourceLoadTicket,
        sample_rate: u32,
        samples: Vec<[f32; 2]>,
        info: AudioSourceInfo,
        recording_allowed: bool,
    ) -> Result<TrackAnalysis, String> {
        self.load_decoded_source_cached(ticket, sample_rate, samples, info, recording_allowed, None)
    }
    /// Reuse versioned analysis while still obtaining and validating normal PCM.
    pub fn load_decoded_source_cached(
        &self,
        ticket: SourceLoadTicket,
        sample_rate: u32,
        samples: Vec<[f32; 2]>,
        info: AudioSourceInfo,
        recording_allowed: bool,
        cached: Option<TrackAnalysis>,
    ) -> Result<TrackAnalysis, String> {
        if ticket.owner != Arc::as_ptr(&self.shared) as usize {
            return Err("Source load belongs to another engine".into());
        }
        if self.shared.load_generation[ticket.deck].load(Ordering::Acquire) != ticket.generation {
            return Err("A newer load request replaced this source".into());
        }
        let track =
            decode::prepare_pcm_cached(samples, sample_rate, info, recording_allowed, cached)?;
        self.load_data(ticket.deck, track, ticket.generation)
    }
    pub fn load_sample(&self, slot: usize, path: &Path) -> Result<TrackAnalysis, String> {
        if slot >= 16 {
            return Err("Sample slot must be 0..15".into());
        }
        let track = decode::decode_file(path)?;
        let analysis = track.analysis.clone();
        self.enqueue(RenderCommand::LoadSample { slot, track })?;
        self.shared
            .samples
            .lock()
            .map_err(|_| "Sample metadata unavailable")?[slot] = Some(analysis.clone());
        self.shared
            .metadata_revision
            .fetch_add(1, Ordering::Release);
        Ok(analysis)
    }
    pub fn load_demo(&self, deck: usize) -> Result<TrackAnalysis, String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        let generation = self.begin_load(deck)?;
        self.load_data(deck, decode::demo_track(deck), generation)
    }
    fn begin_load(&self, deck: usize) -> Result<u64, String> {
        let _tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        Ok(self.shared.load_generation[deck].fetch_add(1, Ordering::AcqRel) + 1)
    }
    fn load_data(
        &self,
        deck: usize,
        track: Arc<TrackData>,
        generation: u64,
    ) -> Result<TrackAnalysis, String> {
        let analysis = track.analysis.clone();
        let mut tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        if self.shared.load_generation[deck].load(Ordering::Acquire) != generation {
            return Err("A newer load request replaced this track".into());
        }
        let mut producer = self
            .shared
            .producer
            .lock()
            .map_err(|_| "Command queue poisoned")?;
        if producer.slots() == 0 {
            return Err("Audio command queue is full; retry shortly".into());
        }
        let mut sources = self
            .shared
            .sources
            .lock()
            .map_err(|_| "Source cache poisoned")?;
        producer
            .push(RenderCommand::Load {
                revision: self.load_revision(deck) + 1,
                deck,
                track: track.clone(),
            })
            .map_err(|_| "Audio command queue is full")?;
        self.shared.analysis_pending[deck].store(track.analysis_pending, Ordering::Release);
        sources[deck] = Some(track);
        tracks[deck] = Some(analysis.clone());
        self.shared.committed_load_revision[deck].fetch_add(1, Ordering::Release);
        self.shared
            .metadata_revision
            .fetch_add(1, Ordering::Release);
        Ok(analysis)
    }
    /// Run the neural beat stage on the deck's RAM source (blocks the calling
    /// worker for seconds) and replace the deck's metadata if the same load is
    /// still current. Transport, BPM and grid commands are left to the caller.
    pub fn beat_model_loaded(&self, deck: usize) -> Result<TrackAnalysis, String> {
        let (source, mut analysis, generation) = self.loaded_source(deck)?;
        if decode::beat_stage_current(&analysis)
            && (analysis.beat_model.is_some() || analysis.neural_rhythm.is_none())
        {
            return Ok(analysis);
        }
        let replaced = || self.shared.load_generation[deck].load(Ordering::Acquire) != generation;
        let result = decode::analyze_beats(
            &source.samples,
            source.sample_rate,
            &mut analysis,
            &replaced,
        );
        if replaced() {
            return Err("Track changed during analysis".into());
        }
        // A terminal stage failure is committed once; passive loads do not retry it.
        let _ = result; // Persist the terminal rhythm error with its stage stamp.
        self.replace_loaded_metadata(deck, generation, analysis)
    }
    pub fn update_loaded_analysis(
        &self,
        deck: usize,
        track_id: &str,
        change: impl FnOnce(&mut TrackAnalysis),
    ) -> Result<TrackAnalysis, String> {
        self.update_loaded_analysis_for_revision(deck, track_id, None, change)
    }
    pub fn update_loaded_analysis_for_revision(
        &self,
        deck: usize,
        track_id: &str,
        expected: Option<u64>,
        change: impl FnOnce(&mut TrackAnalysis),
    ) -> Result<TrackAnalysis, String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        let mut tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        if expected.is_some_and(|r| r != self.load_revision(deck)) {
            return Err("Loaded source revision conflict".into());
        }
        let analysis = tracks[deck].as_mut().ok_or("Deck has no loaded track")?;
        if analysis.id != track_id {
            return Err("Track changed".into());
        }
        change(analysis);
        let result = analysis.clone();
        self.shared
            .metadata_revision
            .fetch_add(1, Ordering::Release);
        Ok(result)
    }
    fn loaded_source(&self, deck: usize) -> Result<(Arc<TrackData>, TrackAnalysis, u64), String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        let tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        let current = tracks[deck].clone().ok_or("Deck has no loaded track")?;
        let sources = self
            .shared
            .sources
            .lock()
            .map_err(|_| "Source cache poisoned")?;
        let source = sources[deck]
            .clone()
            .ok_or("Deck audio source is unavailable")?;
        Ok((
            source,
            current,
            self.shared.load_generation[deck].load(Ordering::Acquire),
        ))
    }
    fn replace_loaded_metadata(
        &self,
        deck: usize,
        generation: u64,
        analysis: TrackAnalysis,
    ) -> Result<TrackAnalysis, String> {
        self.replace_loaded_metadata_with_grid(deck, generation, analysis, false)
    }
    fn replace_loaded_metadata_with_grid(
        &self,
        deck: usize,
        generation: u64,
        mut analysis: TrackAnalysis,
        update_grid: bool,
    ) -> Result<TrackAnalysis, String> {
        let mut tracks = self
            .shared
            .tracks
            .lock()
            .map_err(|_| "Track cache poisoned")?;
        if self.shared.load_generation[deck].load(Ordering::Acquire) != generation
            || tracks[deck]
                .as_ref()
                .is_none_or(|track| track.id != analysis.id)
        {
            return Err("Track changed during analysis".into());
        }
        if let Some(current) = tracks[deck].as_ref() {
            analysis.cue_points = current.cue_points.clone();
        }
        if update_grid {
            if let Some(grid) = analysis.neural_rhythm.clone() {
                self.enqueue(RenderCommand::UpdateGrid {
                    deck,
                    revision: self.load_revision(deck),
                    grid: Arc::new(grid),
                })?;
            } else {
                self.enqueue(RenderCommand::UpdateBaseGrid {
                    deck,
                    revision: self.load_revision(deck),
                    bpm: analysis.bpm,
                    offset: analysis.beatgrid_offset_seconds,
                    valid: analysis.rhythm_error.is_none(),
                })?;
            }
        }
        if update_grid && decode::playback_analysis_ready(&analysis) {
            self.enqueue(RenderCommand::AnalysisComplete {
                deck,
                revision: self.load_revision(deck),
            })?;
            self.shared.analysis_pending[deck].store(false, Ordering::Release);
        }
        tracks[deck] = Some(analysis.clone());
        self.shared
            .metadata_revision
            .fetch_add(1, Ordering::Release);
        Ok(analysis)
    }
    /// Reanalyze the deck's existing RAM source on the calling worker, including
    /// the beat model. Native position, playback and cues are untouched; the
    /// caller decides whether the new BPM and grid reach the deck.
    pub fn reanalyze_loaded(&self, deck: usize) -> Result<TrackAnalysis, String> {
        self.analyze_loaded(deck, true)
    }
    /// Complete missing analysis from the deck's existing PCM, without decoding
    /// again or retrying compatible persisted terminal stages.
    pub fn complete_loaded_analysis(&self, deck: usize) -> Result<TrackAnalysis, String> {
        self.analyze_loaded(deck, false)
    }
    fn analyze_loaded(&self, deck: usize, force: bool) -> Result<TrackAnalysis, String> {
        let (source, current, generation) = self.loaded_source(deck)?;
        let mut retry = current.clone();
        if force {
            decode::retry_neural(&mut retry);
        }
        let mut analysis = decode::analyze_pcm_cached(
            &source.samples,
            source.sample_rate,
            &AudioSourceInfo {
                id: current.id.clone(),
                location: current.path.clone(),
                title: current.title.clone(),
                artist: current.artist.clone(),
                extension: None,
                release_identity: current.source_release.clone(),
            },
            current.recording_allowed,
            Some(retry),
        );
        analysis.analysis_stages.source_file = current.analysis_stages.source_file.clone();
        // Embedded BPM tags are provenance-bearing metadata, never replaced by
        // a statistical reanalysis. The candidate list still describes the PCM.
        if current.bpm_source.as_deref() == Some("tag") {
            analysis.bpm = current.bpm;
            analysis.bpm_source = current.bpm_source;
            analysis.bpm_confidence = current.bpm_confidence;
            analysis.bpm_ambiguous = false;
            if analysis.neural_rhythm.is_none() {
                let phase = audio_analysis::estimate_phase(
                    &source.samples,
                    source.sample_rate,
                    analysis.bpm,
                );
                analysis.beatgrid_offset_seconds = phase.as_ref().map(|p| p.offset_seconds);
                analysis.beatgrid_confidence = phase.map(|p| p.confidence);
                analysis.beatgrid_source = analysis.beatgrid_offset_seconds.map(|_| "tag".into());
            }
        }
        if analysis.neural_rhythm.is_none() {
            analysis.neural_rhythm = current.neural_rhythm.clone();
            if analysis.neural_rhythm.is_some() {
                analysis.beatgrid_offset_seconds = current.beatgrid_offset_seconds;
                analysis.beatgrid_source = current.beatgrid_source.clone();
            }
        }
        let replaced = || self.shared.load_generation[deck].load(Ordering::Acquire) != generation;
        // Reanalysis never owns the DJ’s saved cue edits.
        analysis.cue_points = current.cue_points.clone();
        match decode::analyze_beats(
            &source.samples,
            source.sample_rate,
            &mut analysis,
            &replaced,
        ) {
            Ok(()) => {}
            Err(_) if replaced() => return Err("Track changed during analysis".into()),
            // Keep the DSP result; the stage stays unset so a later pass retries.
            Err(_) => {}
        }
        let transport = self.transport_snapshot();
        let d = &transport.decks[deck];
        if d.beatgrid_manual {
            analysis.beatgrid_source = Some("manual".into());
            analysis.beatgrid_offset_seconds = Some(d.beatgrid_offset_seconds);
            let previous_anchor = current
                .neural_rhythm
                .as_ref()
                .and_then(|g| d.beatgrid_anchor_index.and_then(|i| g.beats_seconds.get(i)))
                .copied()
                .unwrap_or(d.beatgrid_offset_seconds);
            analysis.manual_grid_anchor_index = analysis
                .neural_rhythm
                .as_ref()
                .map(|g| crate::grid::nearest_index(&g.beats_seconds, previous_anchor));
        }
        self.replace_loaded_metadata_with_grid(deck, generation, analysis, true)
    }
    /// Fast transport/meters only: no metadata mutex or waveform-vector clones.
    pub fn transport_snapshot(&self) -> EngineSnapshot {
        self.shared.atoms.read([None, None])
    }
    /// Return track analysis only when the caller observes a new revision.
    pub fn metadata_snapshot(&self) -> EngineMetadataSnapshot {
        self.project_metadata(Clone::clone)
    }
    /// Transport identity fields, without copying derived analysis arrays.
    pub fn metadata_summary_snapshot(&self) -> EngineMetadataSnapshot {
        self.project_metadata(|track| TrackAnalysis {
            id: track.id.clone(),
            path: track.path.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            duration_seconds: track.duration_seconds,
            bpm: track.bpm,
            key: track.key.clone(),
            recording_allowed: track.recording_allowed,
            ..Default::default()
        })
    }
    fn project_metadata(
        &self,
        project: impl Fn(&TrackAnalysis) -> TrackAnalysis,
    ) -> EngineMetadataSnapshot {
        let tracks = self.shared.tracks.lock().unwrap_or_else(|e| e.into_inner());
        EngineMetadataSnapshot {
            load_revisions: std::array::from_fn(|i| {
                self.shared.committed_load_revision[i].load(Ordering::Acquire)
            }),
            revision: self.shared.metadata_revision.load(Ordering::Acquire),
            tracks: tracks.each_ref().map(|track| track.as_ref().map(&project)),
            samples: self
                .shared
                .samples
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .each_ref()
                .map(|track| track.as_ref().map(&project)),
        }
    }
    pub fn load_revision(&self, deck: usize) -> u64 {
        self.shared
            .committed_load_revision
            .get(deck)
            .map(|v| v.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    /// Read a loaded track's length without cloning its waveform metadata.
    pub fn track_duration(&self, deck: usize) -> Option<f64> {
        self.shared
            .tracks
            .lock()
            .ok()?
            .get(deck)?
            .as_ref()
            .map(|t| t.duration_seconds)
    }
    pub fn metadata_revision(&self) -> u64 {
        self.shared.metadata_revision.load(Ordering::Acquire)
    }
    pub fn snapshot(&self) -> EngineSnapshot {
        let tracks = self
            .shared
            .tracks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        self.shared.atoms.read(tracks)
    }
}
/// Analyze decoded PCM off-thread without touching a deck or writing audio.
pub fn analyze_decoded_audio(
    samples: &[[f32; 2]],
    sample_rate: u32,
    info: &AudioSourceInfo,
    recording_allowed: bool,
) -> Result<TrackAnalysis, String> {
    if !(8000..=192000).contains(&sample_rate) || samples.is_empty() || samples.len() > 180_000_000
    {
        return Err("Invalid or oversized decoded audio source".into());
    }
    Ok(decode::analyze_pcm(
        samples,
        sample_rate,
        info,
        recording_allowed,
    ))
}
impl Drop for EngineHandle {
    fn drop(&mut self) {
        if self.recording_status().active {
            let _ = self.stop_recording();
        }
        let _ = self.shared.lifecycle.send(Lifecycle::Shutdown);
    }
}

pub fn devices() -> Result<Vec<AudioDeviceInfo>, String> {
    let host = cpal::default_host();
    let default = host.default_output_device().and_then(|d| d.name().ok());
    let devices = host
        .output_devices()
        .map_err(|e| format!("Cannot list audio devices: {e}"))?;
    let mut result = Vec::new();
    for device in devices {
        let name = device
            .name()
            .unwrap_or_else(|_| "Unnamed audio device".into());
        let configs = device
            .supported_output_configs()
            .map_err(|e| format!("Cannot query {name}: {e}"))?;
        let mut channels = 0;
        let mut rates = Vec::new();
        for config in configs {
            channels = channels.max(config.channels());
            for rate in [44_100, 48_000, 88_200, 96_000, 192_000] {
                if config.min_sample_rate().0 <= rate
                    && config.max_sample_rate().0 >= rate
                    && !rates.contains(&rate)
                {
                    rates.push(rate);
                }
            }
        }
        result.push(AudioDeviceInfo {
            is_default: default.as_ref() == Some(&name),
            name,
            channels,
            sample_rates: rates,
        });
    }
    Ok(result)
}

fn audio_thread(
    receiver: mpsc::Receiver<Lifecycle>,
    state: Arc<StateCell>,
    atoms: Arc<SnapshotAtoms>,
    shutdown: Arc<AtomicBool>,
) {
    let mut stream: Option<cpal::Stream> = None;
    // Explicit debug integration harness: the actual renderer, no device/driver.
    let null_test = cfg!(debug_assertions)
        && std::env::var("PULSEDECK_ISOLATED").as_deref() == Ok("1")
        && std::env::var("PULSEDECK_TEST_NULL_AUDIO").as_deref() == Ok("1")
        && std::env::var_os("PULSEDECK_DATA_DIR").is_some()
        && std::env::var_os("PULSEDECK_SOCKET").is_some();
    let mut null_running = null_test;
    let null_started = std::time::Instant::now();
    let mut null_output = if null_test {
        vec![0_f32; 8192 * 2]
    } else {
        Vec::new()
    };
    if null_test {
        atoms.running.store(true, Ordering::Release);
    }
    loop {
        match receiver.recv_timeout(Duration::from_millis(if null_running { 6 } else { 10 })) {
            Ok(Lifecycle::Start {
                device,
                buffer,
                routing,
                reply,
            }) => {
                if null_test {
                    let _ = reply.send(Err(
                        "Hardware audio is disabled in the isolated null-renderer harness".into(),
                    ));
                    continue;
                }
                // Reject unavailable devices and invalid routing before stopping healthy audio.
                let config = match stream_config(device, buffer, &routing) {
                    Ok(config) => config,
                    Err(error) => {
                        let _ = reply.send(Err(error));
                        continue;
                    }
                };
                atoms.stream_revision.fetch_add(1, Ordering::AcqRel);
                if let Some(old_stream) = stream.take() {
                    let _ = old_stream.pause();
                    drop(old_stream);
                }
                atoms.running.store(false, Ordering::Release);
                if let Some(mut render) = state.take() {
                    render.mixer.engine_epoch = atoms.stream_revision.load(Ordering::Acquire);
                    render.mixer.interrupt_batch();
                    atoms.publish(&render.mixer, render.channels, 0, [0.; 2], [0.; 2]);
                    state.put(render);
                }
                let result = start_stream(config, buffer, routing, state.clone(), atoms.clone());
                match result {
                    Ok((new_stream, status)) => {
                        stream = Some(new_stream);
                        atoms.running.store(true, Ordering::Release);
                        let _ = reply.send(Ok(status));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            Ok(Lifecycle::Stop { reply }) => {
                null_running = false;
                atoms.stream_revision.fetch_add(1, Ordering::AcqRel);
                if let Some(old_stream) = stream.take() {
                    let _ = old_stream.pause();
                    drop(old_stream);
                }
                atoms.running.store(false, Ordering::Release);
                if let Some(mut render) = state.take() {
                    render.mixer.engine_epoch = atoms.stream_revision.load(Ordering::Acquire);
                    render.mixer.interrupt_batch();
                    atoms.publish(&render.mixer, render.channels, 0, [0.; 2], [0.; 2]);
                    state.put(render);
                }
                let _ = reply.send(());
            }
            Ok(Lifecycle::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if stream.is_some() && !atoms.running.load(Ordering::Acquire) {
            if let Some(failed) = stream.take() {
                let _ = failed.pause();
                drop(failed);
            }
            atoms.stream_revision.fetch_add(1, Ordering::AcqRel);
            if let Some(mut render) = state.take() {
                render.mixer.engine_epoch = atoms.stream_revision.load(Ordering::Acquire);
                render.mixer.interrupt_batch();
                atoms.publish(&render.mixer, render.channels, 0, [0.; 2], [0.; 2]);
                state.put(render);
            }
        }
        if stream.is_none() {
            if let Some(mut render) = state.take() {
                if null_running && null_started.elapsed() < Duration::from_secs(900) {
                    render.mixer.sample_rate = 48000;
                    render.channels = 2;
                    render.routing = AudioRouting::default();
                    // Accelerate only native schedules; preparation has a paced clock.
                    let frames = if render
                        .mixer
                        .scheduled
                        .as_ref()
                        .is_some_and(|b| b.state.active())
                    {
                        8192
                    } else {
                        256
                    };
                    render.render(&mut null_output[..frames * 2], &atoms);
                } else {
                    if null_running {
                        null_running = false;
                        atoms.running.store(false, Ordering::Release);
                        render.mixer.interrupt_batch();
                    }
                    render.drain();
                    atoms.publish(&render.mixer, render.channels, 0, [0.0; 2], [0.0; 2]);
                }
                state.put(render);
            }
        }
    }
    atoms.stream_revision.fetch_add(1, Ordering::AcqRel);
    if let Some(stream) = stream {
        let _ = stream.pause();
        drop(stream);
    }
    atoms.running.store(false, Ordering::Release);
    drop(state);
    shutdown.store(true, Ordering::Release);
}

fn validate_routing(routing: &AudioRouting, channels: u16) -> Result<(), String> {
    let mut used = vec![routing.master_left, routing.master_right];
    match (routing.cue_left, routing.cue_right) {
        (Some(l), Some(r)) => {
            used.push(l);
            used.push(r);
        }
        (None, None) => {}
        _ => return Err("Headphone routing requires both left and right channels".into()),
    }
    if used.iter().any(|c| *c >= channels) {
        return Err(format!(
            "Routing requests an unavailable channel; device has {channels} channels"
        ));
    }
    for i in 0..used.len() {
        if used[i + 1..].contains(&used[i]) {
            return Err("Master and headphone outputs must use distinct channels".into());
        }
    }
    Ok(())
}

fn stream_config(
    name: Option<String>,
    buffer: Option<u32>,
    routing: &AudioRouting,
) -> Result<(cpal::Device, StreamConfig, SampleFormat), String> {
    let host = cpal::default_host();
    let device = match name {
        Some(name) => host
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().ok().as_deref() == Some(&name))
            .ok_or_else(|| format!("Audio device not found: {name}"))?,
        None => host
            .default_output_device()
            .ok_or("No audio output device is available")?,
    };
    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let needed = [
        Some(routing.master_left),
        Some(routing.master_right),
        routing.cue_left,
        routing.cue_right,
    ]
    .into_iter()
    .flatten()
    .map(u32::from)
    .max()
    .unwrap_or(1)
        + 1;
    let mut candidates: Vec<_> = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .filter(|c| {
            u32::from(c.channels()) >= needed
                && matches!(
                    c.sample_format(),
                    SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
                )
        })
        .collect();
    candidates.sort_by_key(|c| {
        (
            u32::from(c.channels()) != needed,
            c.sample_format() != SampleFormat::F32,
            !(c.min_sample_rate().0 <= 48000 && c.max_sample_rate().0 >= 48000),
        )
    });
    let selected = candidates
        .into_iter()
        .next()
        .ok_or("No supported output configuration for the requested channel routing")?;
    let rate = if selected.min_sample_rate().0 <= 48000 && selected.max_sample_rate().0 >= 48000 {
        48000
    } else {
        default
            .sample_rate()
            .0
            .clamp(selected.min_sample_rate().0, selected.max_sample_rate().0)
    };
    let format = selected.sample_format();
    let mut config: StreamConfig = selected.with_sample_rate(cpal::SampleRate(rate)).config();
    if let Some(frames) = buffer {
        if frames == 0 {
            return Err("Audio buffer must contain at least one frame".into());
        }
        if let cpal::SupportedBufferSize::Range { min, max } = selected.buffer_size() {
            if frames < *min || frames > *max {
                return Err(format!(
                    "Buffer must be between {min} and {max} frames for this device"
                ));
            }
        }
        config.buffer_size = cpal::BufferSize::Fixed(frames);
    }
    validate_routing(routing, config.channels)?;
    Ok((device, config, format))
}

fn start_stream(
    (device, config, format): (cpal::Device, StreamConfig, SampleFormat),
    buffer: Option<u32>,
    routing: AudioRouting,
    state: Arc<StateCell>,
    atoms: Arc<SnapshotAtoms>,
) -> Result<(cpal::Stream, AudioStatus), String> {
    let rate = config.sample_rate.0;
    let mut render = state.take().ok_or("Audio render state is unavailable")?;
    render.mixer.sample_rate = rate;
    render.routing = routing.clone();
    render.channels = config.channels;
    render.drain();
    if render
        .recording
        .as_ref()
        .is_some_and(|r| r.stats.rate != rate)
    {
        state.put(render);
        return Err("Stop recording before changing output sample rate".into());
    }
    atoms.publish(
        &render.mixer,
        config.channels,
        buffer.unwrap_or(0),
        [0.0; 2],
        [0.0; 2],
    );
    state.put(render);
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, state, atoms)?,
        SampleFormat::I16 => build::<i16>(&device, &config, state, atoms)?,
        SampleFormat::U16 => build::<u16>(&device, &config, state, atoms)?,
        _ => return Err("Unsupported output sample format".into()),
    };
    stream
        .play()
        .map_err(|e| format!("Cannot start audio device: {e}"))?;
    let status = AudioStatus {
        running: true,
        device_name: device.name().unwrap_or_default(),
        sample_rate: rate,
        channels: config.channels,
        buffer_frames: buffer.unwrap_or(0),
        routing,
    };
    Ok((stream, status))
}
fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &StreamConfig,
    state: Arc<StateCell>,
    atoms: Arc<SnapshotAtoms>,
) -> Result<cpal::Stream, String> {
    let error_atoms = atoms.clone();
    let stream_revision = atoms.stream_revision.load(Ordering::Acquire);
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                if atoms.stream_revision.load(Ordering::Acquire) != stream_revision {
                    output.fill(T::from_sample(0.0));
                    return;
                }
                if let Some(mut render) = state.take() {
                    render.render(output, &atoms);
                    state.put(render);
                } else {
                    for sample in output {
                        *sample = T::from_sample(0.0);
                    }
                }
            },
            move |_| {
                if error_atoms.stream_revision.load(Ordering::Acquire) != stream_revision {
                    return;
                }
                error_atoms.errors.fetch_add(1, Ordering::Relaxed);
                error_atoms.running.store(false, Ordering::Release);
            },
            None,
        )
        .map_err(|e| format!("Cannot open audio stream: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_changes_are_rejected_before_touching_an_active_recording() {
        let engine = EngineHandle::new().unwrap();
        let path =
            std::env::temp_dir().join(format!("pd-recording-output-{}.wav", std::process::id()));
        engine.start_recording(&path).unwrap();
        let error = engine
            .start_audio(
                Some("Missing test output".into()),
                Some(0),
                AudioRouting::default(),
            )
            .unwrap_err();
        assert_eq!(error, "Stop recording before changing audio output");
        assert!(engine.recording_status().active);
        engine.stop_recording().unwrap();
        assert!(!engine.recording_status().active);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn completion_fences_source_guards_and_explicit_cues_preserve_transport() {
        let engine = EngineHandle::new().unwrap();
        engine.load_demo(0).unwrap();
        engine
            .command(EngineCommand::Seek {
                deck: 0,
                seconds: 4.0,
            })
            .unwrap();
        engine.command(EngineCommand::Play { deck: 0 }).unwrap();
        let fence = engine.fence().unwrap();
        engine
            .wait_for_fence(fence, Duration::from_secs(2))
            .unwrap();
        let before = engine.snapshot();
        let revision = engine.load_revision(0);
        engine
            .command_for_revision(
                EngineCommand::SetHotCuePoint {
                    deck: 0,
                    index: 7,
                    seconds: 8.0,
                },
                Some(revision),
            )
            .unwrap();
        let fence = engine.fence().unwrap();
        engine
            .wait_for_fence(fence, Duration::from_secs(2))
            .unwrap();
        let after = engine.snapshot();
        assert_eq!(
            after.decks[0].position_seconds,
            before.decks[0].position_seconds
        );
        assert_eq!(after.decks[0].playing, before.decks[0].playing);
        assert_eq!(after.decks[0].hot_cues[7], Some(8.0));
        engine.load_demo(0).unwrap();
        assert!(engine
            .command_for_revision(
                EngineCommand::SetHotCuePoint {
                    deck: 0,
                    index: 0,
                    seconds: 2.0
                },
                Some(revision)
            )
            .is_err());
        let fence = engine.fence().unwrap();
        engine
            .wait_for_fence(fence, Duration::from_secs(2))
            .unwrap();
        assert!(engine.snapshot().decks[0]
            .hot_cues
            .iter()
            .all(Option::is_none));
    }
    #[test]
    fn named_cue_metadata_updates_are_guarded_even_on_same_source_reload_and_both_copies() {
        let engine = EngineHandle::new().unwrap();
        let track = engine.load_demo(0).unwrap();
        engine.instant_double(1).unwrap();
        let revisions = engine.metadata_snapshot().load_revisions;
        let point = audio_analysis::CuePoint::manual("user:1".into(), 2., "Entry".into());
        for (deck, revision) in revisions.iter().enumerate() {
            engine
                .update_loaded_analysis_for_revision(deck, &track.id, Some(*revision), |a| {
                    a.cue_points = vec![point.clone()]
                })
                .unwrap();
        }
        assert!(engine
            .metadata_snapshot()
            .tracks
            .iter()
            .all(|a| a.as_ref().unwrap().cue_points[0].name == "Entry"));
        engine.load_demo(0).unwrap();
        assert!(engine
            .update_loaded_analysis_for_revision(0, &track.id, Some(revisions[0]), |a| a
                .cue_points
                .clear())
            .is_err());
        assert_eq!(
            engine.metadata_snapshot().tracks[1]
                .as_ref()
                .unwrap()
                .cue_points[0]
                .name,
            "Entry"
        );
    }
    #[test]
    fn invalid_routes_are_rejected() {
        assert!(validate_routing(
            &AudioRouting {
                cue_left: Some(2),
                cue_right: Some(3),
                ..Default::default()
            },
            2
        )
        .is_err());
        assert!(validate_routing(
            &AudioRouting {
                cue_left: Some(2),
                cue_right: Some(3),
                ..Default::default()
            },
            4
        )
        .is_ok());
        assert!(validate_routing(
            &AudioRouting {
                master_left: 1,
                ..Default::default()
            },
            2
        )
        .is_err());
    }
    #[test]
    fn queue_and_offline_state_work_without_hardware() {
        let e = EngineHandle::new().unwrap();
        e.load_demo(0).unwrap();
        e.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 8.0,
        })
        .unwrap();
        e.command(EngineCommand::Seek {
            deck: 0,
            seconds: 4.0,
        })
        .unwrap();
        for _ in 0..100 {
            if e.snapshot().decks[0].tempo_percent == 8.0 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let s = e.snapshot();
        assert_eq!(s.decks[0].tempo_percent, 8.0);
        assert_eq!(s.decks[0].position_seconds, 4.0);
        assert!(!s.running);
        assert!(e
            .command(EngineCommand::SetEq {
                deck: 0,
                low: f32::NAN,
                mid: 1.0,
                high: 1.0
            })
            .is_err());
    }
    #[test]
    fn queued_manual_takeover_rejects_preparation_before_any_mixer_change() {
        let (mut producer, consumer) = RingBuffer::new(8);
        let (retired, _reclaimed) = RingBuffer::new(8);
        let mut render = RenderState {
            mixer: Mixer::new(),
            commands: consumer,
            retired,
            recording: None,
            routing: Default::default(),
            channels: 2,
        };
        producer
            .push(RenderCommand::Control(EngineCommand::SetChannelFader {
                deck: 1,
                value: 0.8,
            }))
            .unwrap();
        producer
            .push(RenderCommand::GuardedControl {
                command: EngineCommand::SetChannelFader { deck: 1, value: 0. },
                deck: 1,
                revision: 0,
                control_revision: Some(0),
                receipt_ticket: Some(1),
            })
            .unwrap();
        render.drain();
        assert_eq!(render.mixer.decks[1].channel_fader.target, 0.8);
        assert_eq!(render.mixer.control_revision, 1);
        assert_eq!(render.mixer.guarded_control_receipts[1], (1 << 2) | 2);
        producer
            .push(RenderCommand::GuardedControl {
                command: EngineCommand::SetChannelFader { deck: 1, value: 0. },
                deck: 1,
                revision: 0,
                control_revision: Some(1),
                receipt_ticket: Some(2),
            })
            .unwrap();
        render.drain();
        assert_eq!(render.mixer.decks[1].channel_fader.target, 0.);
        assert_eq!(render.mixer.guarded_control_receipts[2], (2 << 2) | 1);
    }
    #[test]
    fn session_endpoint_survives_source_changes_but_yields_to_manual_input_and_restart() {
        for scenario in [
            "source_load",
            "manual_before",
            "manual_after",
            "audio_restart",
        ] {
            let (mut producer, consumer) = RingBuffer::new(8);
            let (retired, _reclaimed) = RingBuffer::new(8);
            let mut mixer = Mixer::new();
            mixer.load(0, decode::demo_track(0));
            mixer.load(1, decode::demo_track(1));
            mixer.decks[0].playing = true;
            mixer.decks[1].playing = true;
            mixer.engine_epoch = if scenario == "audio_restart" { 8 } else { 7 };
            mixer.load_revisions = [17, 30];
            let mut render = RenderState {
                mixer,
                commands: consumer,
                retired,
                recording: None,
                routing: Default::default(),
                channels: 2,
            };
            if scenario == "manual_before" {
                producer
                    .push(RenderCommand::Control(EngineCommand::SetChannelFader {
                        deck: 0,
                        value: 0.8,
                    }))
                    .unwrap();
            }
            producer
                .push(RenderCommand::SessionEndpoint {
                    engine_epoch: 7,
                    control_revision: 0,
                    receipt_ticket: 1,
                })
                .unwrap();
            if scenario == "manual_after" {
                producer
                    .push(RenderCommand::Control(EngineCommand::Play { deck: 0 }))
                    .unwrap();
            }
            render.drain();
            let applied = render.mixer.guarded_control_receipts[1] & 3 == 1;
            if scenario == "manual_before" || scenario == "audio_restart" {
                assert!(!applied, "{scenario}");
                assert!(render.mixer.decks.iter().all(|d| d.playing), "{scenario}");
            } else {
                assert!(applied, "{scenario}");
                assert!(!render.mixer.decks[1].playing, "{scenario}");
                assert_eq!(
                    render.mixer.decks[0].playing,
                    scenario == "manual_after",
                    "{scenario}"
                );
            }
        }
    }
    #[test]
    fn guarded_control_receipts_confirm_application_without_an_audio_device() {
        let engine = EngineHandle::new().unwrap();
        let revision = engine.transport_snapshot().control_revision;
        let ticket = engine
            .command_for_control_revision(
                EngineCommand::SetChannelFader { deck: 1, value: 0. },
                Some(0),
                revision,
            )
            .unwrap();
        engine
            .wait_for_control_receipt(ticket, Duration::from_secs(2))
            .unwrap();
        assert!(engine
            .command_for_control_revision(
                EngineCommand::Seek {
                    deck: 1,
                    seconds: 0.
                },
                Some(0),
                revision,
            )
            .is_err());
        assert!(engine.wait_for_control_receipt(0, Duration::ZERO).is_err());
    }
    #[test]
    fn assets_are_retired_outside_render() {
        let (p, consumer) = RingBuffer::new(8);
        let (retired, mut reclaimed) = RingBuffer::new(8);
        let mut render = RenderState {
            mixer: Mixer::new(),
            commands: consumer,
            retired,
            recording: None,
            routing: Default::default(),
            channels: 2,
        };
        let mut producer = p;
        let old = decode::demo_track(0);
        let weak = Arc::downgrade(&old);
        producer
            .push(RenderCommand::Load {
                revision: 0,
                deck: 0,
                track: old,
            })
            .ok()
            .unwrap();
        render.drain();
        producer
            .push(RenderCommand::Load {
                revision: 0,
                deck: 0,
                track: decode::demo_track(1),
            })
            .ok()
            .unwrap();
        render.drain();
        assert!(weak.upgrade().is_some());
        drop(reclaimed.pop().unwrap());
        assert!(weak.upgrade().is_none());
    }
}

#[cfg(test)]
mod realtime_audit {
    use super::*;
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };
    thread_local! {static ENABLED:Cell<bool>=const{Cell::new(false)};static COUNTS:Cell<[usize;3]>=const{Cell::new([0;3])};}
    struct AuditedAllocator;
    fn count(index: usize) {
        let _ = ENABLED.try_with(|enabled| {
            if enabled.get() {
                let _ = COUNTS.try_with(|counter| {
                    let mut value = counter.get();
                    value[index] += 1;
                    counter.set(value);
                });
            }
        });
    }
    unsafe impl GlobalAlloc for AuditedAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count(0);
            System.alloc(layout)
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            count(1);
            System.realloc(pointer, layout, size)
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            count(2);
            System.dealloc(pointer, layout)
        }
    }
    #[global_allocator]
    static ALLOCATOR: AuditedAllocator = AuditedAllocator;
    pub(crate) fn allocation_counts(f: impl FnOnce()) -> [usize; 3] {
        COUNTS.with(|x| x.set([0; 3]));
        ENABLED.with(|x| x.set(true));
        f();
        ENABLED.with(|x| x.set(false));
        COUNTS.with(Cell::get)
    }
    #[test]
    fn slim_transport_snapshot_does_not_allocate_or_free_metadata() {
        let engine = EngineHandle::new().unwrap();
        engine.load_demo(0).unwrap();
        engine.load_demo(1).unwrap();
        COUNTS.with(|x| x.set([0; 3]));
        ENABLED.with(|x| x.set(true));
        for _ in 0..1000 {
            let snapshot = engine.transport_snapshot();
            std::hint::black_box(snapshot);
        }
        ENABLED.with(|x| x.set(false));
        let counts = COUNTS.with(Cell::get);
        assert_eq!(counts, [0; 3]);
    }
    #[test]
    fn continuous_pitch_render_seek_and_rate_changes_allocate_nothing() {
        let (mut producer, consumer) = RingBuffer::new(128);
        let (retired, _reclaim) = RingBuffer::new(256);
        let atoms = SnapshotAtoms::new();
        let mut state = RenderState {
            mixer: Mixer::new(),
            commands: consumer,
            retired,
            recording: None,
            routing: AudioRouting::default(),
            channels: 2,
        };
        for deck in 0..2 {
            state.mixer.load(deck, decode::demo_track(deck));
            state.mixer.command(EngineCommand::Play { deck });
            state.mixer.command(EngineCommand::SetKeylock {
                deck,
                enabled: true,
            });
        }
        let mut output = [0f32; 256];
        COUNTS.with(|x| x.set([0; 3]));
        ENABLED.with(|x| x.set(true));
        for step in 0..256 {
            producer
                .push(RenderCommand::Control(EngineCommand::SetTempo {
                    deck: 1,
                    percent: 4. + step as f64 * 0.001,
                }))
                .ok()
                .unwrap();
            if step % 64 == 0 {
                producer
                    .push(RenderCommand::Control(EngineCommand::Seek {
                        deck: 0,
                        seconds: step as f64 / 256.,
                    }))
                    .ok()
                    .unwrap();
            }
            state.render(&mut output, &atoms);
        }
        ENABLED.with(|x| x.set(false));
        assert_eq!(
            COUNTS.with(Cell::get),
            [0; 3],
            "Rust render allocations/reallocations/frees"
        );
        assert_eq!(
            state.mixer.decks[1].processor.resets, 1,
            "tempo automation keeps continuous DSP state"
        );
    }
    #[test]
    fn callback_allocates_and_frees_nothing_including_asset_changes() {
        let (mut producer, consumer) = RingBuffer::new(128);
        let (retired, mut reclamation) = RingBuffer::new(256);
        let atoms = SnapshotAtoms::new();
        let mut state = RenderState {
            mixer: Mixer::new(),
            commands: consumer,
            retired,
            recording: None,
            routing: AudioRouting {
                cue_left: Some(2),
                cue_right: Some(3),
                ..Default::default()
            },
            channels: 4,
        };
        state.mixer.load(0, decode::demo_track(0));
        producer
            .push(RenderCommand::Load {
                revision: 0,
                deck: 0,
                track: decode::demo_track(1),
            })
            .ok()
            .unwrap();
        for shift in [0.0, 0.01] {
            let grid = Arc::new(audio_analysis::NeuralRhythm {
                beats_seconds: vec![0.2 + shift, 0.7 + shift, 1.3 + shift, 1.9 + shift],
                downbeats_seconds: vec![0.2 + shift],
                bpm: 120.0,
                beat_confidence: 0.8,
                downbeat_confidence: 0.7,
                model: "allocation-fixture".into(),
                analysis_seconds: 0.0,
                bar_evidence: Vec::new(),
                beats_per_bar: None,
            });
            producer
                .push(RenderCommand::UpdateGrid {
                    deck: 0,
                    revision: 0,
                    grid,
                })
                .ok()
                .unwrap();
        }
        for command in [
            EngineCommand::Play { deck: 0 },
            EngineCommand::SetEq {
                deck: 0,
                low: 0.4,
                mid: 1.2,
                high: 0.9,
            },
            EngineCommand::MemoryCueSave { deck: 0 },
            EngineCommand::SetFx {
                deck: 0,
                echo: 0.3,
                reverb: 0.2,
            },
            EngineCommand::TriggerSample { slot: 15 },
            EngineCommand::SetFxBeat {
                deck: 0,
                beats: 1.0,
            },
            EngineCommand::SetLoop {
                deck: 0,
                beats: Some(4.0),
            },
            EngineCommand::SetLoopEdit {
                deck: 0,
                boundary: Some(LoopBoundary::Out),
            },
            EngineCommand::Jog {
                deck: 0,
                delta_seconds: 0.01,
                scratch: true,
            },
            EngineCommand::Seek {
                deck: 0,
                seconds: 0.5,
            },
            EngineCommand::JogTouch {
                deck: 0,
                touched: true,
            },
            EngineCommand::ScrubTo {
                deck: 0,
                seconds: 0.25,
                duration_ms: 32.0,
            },
            EngineCommand::ScrubEnd {
                deck: 0,
                seconds: 0.25,
            },
            EngineCommand::SetLoopEdit {
                deck: 0,
                boundary: None,
            },
            EngineCommand::SetLoop {
                deck: 0,
                beats: None,
            },
            EngineCommand::SetHotCuePoint {
                deck: 0,
                index: 7,
                seconds: 7.0,
            },
            EngineCommand::Ramp {
                target: ControlTarget::Crossfader,
                value: 0.5,
                seconds: 0.05,
                clock: None,
                beats: None,
                quantize: None,
            },
            EngineCommand::Pause { deck: 0 },
            EngineCommand::Jog {
                deck: 0,
                delta_seconds: 0.02,
                scratch: false,
            },
        ] {
            producer.push(RenderCommand::Control(command)).ok().unwrap();
        }
        producer
            .push(RenderCommand::InstantDouble {
                revision: 0,
                deck: 1,
                track: state.mixer.decks[0].track.as_ref().unwrap().clone(),
            })
            .ok()
            .unwrap();
        let mut output = [0.0_f32; 4096];
        let mut audible = false;
        COUNTS.with(|x| x.set([0; 3]));
        ENABLED.with(|x| x.set(true));
        for _ in 0..16 {
            state.render(&mut output, &atoms);
            audible |= output.iter().any(|x| x.abs() > 0.01);
        }
        ENABLED.with(|x| x.set(false));
        let counts = COUNTS.with(Cell::get);
        assert_eq!(counts, [0; 3], "alloc, realloc, dealloc in callback");
        assert!(reclamation.pop().is_ok());
        assert!(audible);
    }
    #[test]
    fn touch_holds_platter_and_sample_is_rendered() {
        let mut mixer = Mixer::new();
        mixer.load(0, decode::demo_track(0));
        mixer.command(EngineCommand::Play { deck: 0 });
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: true,
        });
        for _ in 0..1000 {
            mixer.frame();
        }
        assert_eq!(mixer.decks[0].position, 0.0);
        mixer.command(EngineCommand::Jog {
            deck: 0,
            delta_seconds: 0.05,
            scratch: true,
        });
        for _ in 0..1680 {
            mixer.frame();
        }
        assert!((mixer.decks[0].seconds() - 0.05).abs() < 1e-8);
        for _ in 0..1000 {
            mixer.frame();
        }
        assert!((mixer.decks[0].seconds() - 0.05).abs() < 1e-8);
        mixer.command(EngineCommand::JogTouch {
            deck: 0,
            touched: false,
        });
        for _ in 0..1000 {
            mixer.frame();
        }
        assert!(mixer.decks[0].seconds() > 0.06);
        mixer.command(EngineCommand::Pause { deck: 0 });
        mixer.command(EngineCommand::TriggerSample { slot: 0 });
        assert!((0..100).map(|_| mixer.frame().0[0].abs()).sum::<f32>() > 0.1);
    }
}

#[cfg(test)]
mod preparation_and_loading_tests {
    use super::*;
    fn source_info() -> AudioSourceInfo {
        AudioSourceInfo {
            id: "fixture:normal-output".into(),
            location: "fixture:normal-output".into(),
            title: "Generated provider output".into(),
            artist: None,
            extension: None,
            release_identity: None,
        }
    }
    #[test]
    fn decoded_output_load_obeys_generation_and_recording_capability() {
        let engine = EngineHandle::new().unwrap();
        let old = engine.begin_source_load(0).unwrap();
        let ticket = engine.begin_source_load(0).unwrap();
        assert!(engine
            .load_decoded_source(old, 8000, vec![[0.1; 2]; 16000], source_info(), false)
            .is_err());
        let analysis = engine
            .load_decoded_source(ticket, 8000, vec![[0.1; 2]; 16000], source_info(), false)
            .unwrap();
        assert!(!analysis.recording_allowed);
        assert_eq!(analysis.duration_seconds, 2.);
        assert_eq!(analysis.waveform.len(), 4096);
        let path =
            std::env::temp_dir().join(format!("restricted-source-test-{}.wav", std::process::id()));
        assert!(engine.start_recording(&path).is_err());
        assert!(!path.exists());
        engine
            .command(EngineCommand::Seek {
                deck: 0,
                seconds: 1.,
            })
            .unwrap();
        for _ in 0..100 {
            if engine.snapshot().decks[0].position_seconds == 1. {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(engine.snapshot().decks[0].position_seconds, 1.);
        engine.unload(0).unwrap();
        assert!(engine.snapshot().decks[0].track.is_none());
    }
    #[test]
    fn normally_decoded_output_uses_native_tempo_cue_and_two_deck_mix() {
        let info = source_info();
        let samples: Vec<[f32; 2]> = (0..24000)
            .map(|i| [((i as f32 * 0.1).sin()) * 0.1; 2])
            .collect();
        let source = decode::prepare_pcm(samples, 8000, info, false).unwrap();
        let mut mixer = Mixer::new();
        mixer.load(0, source.clone());
        mixer.load(1, source);
        mixer.command(EngineCommand::SetTempo {
            deck: 0,
            percent: 25.,
        });
        mixer.command(EngineCommand::Play { deck: 0 });
        mixer.command(EngineCommand::Play { deck: 1 });
        let peak = (0..48000)
            .map(|_| mixer.frame().0[0].abs())
            .fold(0.0_f32, f32::max);
        assert!(peak > 0.02);
        assert!((mixer.decks[0].seconds() - 1.25).abs() < 0.002);
        assert!((mixer.decks[1].seconds() - 1.).abs() < 0.002);
        mixer.command(EngineCommand::Cue { deck: 0 });
        assert!(!mixer.decks[0].playing);
        assert_eq!(mixer.decks[0].seconds(), 0.);
    }
    #[test]
    fn decoded_output_rejects_invalid_format_and_cross_engine_ticket() {
        let a = EngineHandle::new().unwrap();
        let b = EngineHandle::new().unwrap();
        let ticket = a.begin_source_load(0).unwrap();
        assert!(b
            .load_decoded_source(ticket, 8000, vec![[0.1; 2]; 16], source_info(), true)
            .is_err());
        for (rate, samples) in [(0, vec![[0.1; 2]; 16]), (8000, vec![])] {
            let ticket = a.begin_source_load(0).unwrap();
            assert!(a
                .load_decoded_source(ticket, rate, samples, source_info(), true)
                .is_err());
        }
    }
    #[test]
    fn scoped_source_cancellation_cannot_cancel_a_newer_load_owner() {
        let engine = EngineHandle::new().unwrap();
        let old = engine.begin_source_load(0).unwrap();
        let newer = engine.begin_source_load(0).unwrap();
        assert!(!engine.cancel_source_load(&old).unwrap());
        assert!(engine.cancel_source_load(&newer).unwrap());
        assert!(!engine.cancel_source_load(&newer).unwrap());
        assert!(engine
            .load_track_cached_for_ticket(old, Path::new("/must-not-be-opened.mp3"), None)
            .unwrap_err()
            .contains("newer load"));
        let other = EngineHandle::new().unwrap();
        assert!(other
            .cancel_source_load(&newer)
            .unwrap_err()
            .contains("another engine"));
        assert!(!engine.transport_snapshot().running);
    }

    #[test]
    fn a_completed_old_decode_cannot_replace_the_newest_load() {
        let engine = EngineHandle::new().unwrap();
        let old_generation = engine.begin_load(0).unwrap();
        let new_generation = engine.begin_load(0).unwrap();
        assert!(engine
            .load_data(0, decode::demo_track(0), old_generation)
            .is_err());
        engine
            .load_data(0, decode::demo_track(1), new_generation)
            .unwrap();
        for _ in 0..100 {
            if engine.snapshot().decks[0].bpm == Some(124.0) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            engine.snapshot().decks[0].track.as_ref().unwrap().id,
            "demo:1"
        );
        assert_eq!(engine.snapshot().decks[0].bpm, Some(124.0));
    }
    #[test]
    fn continuous_keylock_accepts_target_changes_without_preparation() {
        let engine = EngineHandle::new().unwrap();
        let generation = engine.begin_load(0).unwrap();
        let mut source = decode::demo_track(0);
        let data = Arc::get_mut(&mut source).unwrap();
        data.samples.truncate(48000);
        data.analysis.duration_seconds = 1.0;
        engine.load_data(0, source, generation).unwrap();
        engine
            .command(EngineCommand::SetTempo {
                deck: 0,
                percent: 10.0,
            })
            .unwrap();
        engine
            .command(EngineCommand::SetKeylock {
                deck: 0,
                enabled: true,
            })
            .unwrap();
        engine
            .command(EngineCommand::SetTempo {
                deck: 0,
                percent: 20.0,
            })
            .unwrap();
        let mut ready = false;
        for _ in 0..600 {
            let snapshot = engine.snapshot();
            let deck = &snapshot.decks[0];
            if deck.keylock && !deck.preparation_pending && deck.tempo_percent == 20.0 {
                ready = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(ready, "Newest keylock cache did not become ready");
        engine
            .command(EngineCommand::SetKeylock {
                deck: 0,
                enabled: false,
            })
            .unwrap();
    }
}

#[cfg(test)]
mod reanalysis_tests {
    use super::*;
    #[test]
    fn reanalysis_updates_metadata_without_moving_or_resetting_transport() {
        let engine = EngineHandle::new().unwrap();
        let generation = engine.begin_load(0).unwrap();
        let mut track = decode::demo_track(0);
        let source = Arc::get_mut(&mut track).unwrap();
        source.samples.truncate(48000 * 4);
        source.analysis.duration_seconds = 4.0;
        source.analysis.recording_allowed = false;
        engine.load_data(0, track, generation).unwrap();
        for command in [
            EngineCommand::Seek {
                deck: 0,
                seconds: 1.25,
            },
            EngineCommand::SetCue { deck: 0 },
            EngineCommand::SetHotCue { deck: 0, index: 3 },
            EngineCommand::SetBpm {
                deck: 0,
                bpm: 128.0,
            },
            EngineCommand::SetBeatgrid {
                anchor_index: None,
                deck: 0,
                offset_seconds: 0.12,
            },
            EngineCommand::Play { deck: 0 },
        ] {
            engine.command(command).unwrap();
        }
        for _ in 0..100 {
            if engine.snapshot().decks[0].playing {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let before = engine.snapshot().decks[0].clone();
        let updated = engine.reanalyze_loaded(0).unwrap();
        let after = engine.snapshot().decks[0].clone();
        assert_eq!(updated.analysis_version, audio_analysis::ANALYSIS_VERSION);
        assert_eq!(updated.waveform.len(), 4096);
        assert_eq!(updated.waveform_bands.as_ref().unwrap().low.len(), 4096);
        assert!(!updated.recording_allowed);
        assert_eq!(before.position_seconds, after.position_seconds);
        assert_eq!(before.playing, after.playing);
        assert_eq!(before.cue_seconds, after.cue_seconds);
        assert_eq!(before.hot_cues, after.hot_cues);
        assert_eq!(after.bpm, Some(128.0));
        assert_eq!(after.beatgrid_offset_seconds, 0.12);
    }

    #[test]
    fn beat_model_updates_loaded_metadata_but_not_the_deck_grid() {
        let engine = EngineHandle::new().unwrap();
        engine.load_demo(0).unwrap();
        engine
            .command(EngineCommand::SetBeatgrid {
                deck: 0,
                offset_seconds: 0.07,
                anchor_index: None,
            })
            .unwrap();
        engine
            .update_loaded_analysis(0, "demo:0", |a| {
                a.neural_rhythm = Some(audio_analysis::NeuralRhythm {
                    beats_seconds: (0..64).map(|i| i as f64 * 0.5).collect(),
                    downbeats_seconds: (0..16).map(|i| i as f64 * 2.0).collect(),
                    bpm: 120.0,
                    beat_confidence: 0.8,
                    downbeat_confidence: 0.8,
                    model: "full-model-fixture".into(),
                    analysis_seconds: 0.0,
                    bar_evidence: Vec::new(),
                    beats_per_bar: Some(4),
                });
                a.bpm_source = Some("beat_this".into());
                a.beatgrid_source = Some("beat_this".into());
            })
            .unwrap();
        let revision = engine.metadata_revision();
        let updated = engine.beat_model_loaded(0).unwrap();
        let stage = updated.beat_model.as_ref().expect("stage recorded");
        let grid = stage.usable_grid().expect("demo loop has a clear pulse");
        assert!((grid.bpm - 120.0).abs() < 0.5, "{grid:?}");
        assert_eq!(updated.bpm_source.as_deref(), Some("beat_this"));
        assert!(engine.metadata_revision() > revision);
        assert!(engine.metadata_snapshot().tracks[0]
            .as_ref()
            .is_some_and(|t| t.beat_model.is_some()));
        // Applying a new grid to a loaded deck is the caller's decision.
        for _ in 0..100 {
            if engine.snapshot().decks[0].beatgrid_offset_seconds == 0.07 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(engine.snapshot().decks[0].beatgrid_offset_seconds, 0.07);
        engine.unload(0).unwrap();
        assert!(engine.beat_model_loaded(0).is_err());
    }
}

#[cfg(test)]
mod cached_source_tests {
    use super::*;
    #[test]
    fn cached_analysis_reuses_metadata_but_checks_audio_identity_and_capabilities() {
        let samples = vec![[0.0; 2]; 4000];
        let info = || AudioSourceInfo {
            id: "spotify:track:0000000000000000000000".into(),
            location: "spotify:track:0000000000000000000000".into(),
            title: "Fixture".into(),
            artist: None,
            extension: None,
            release_identity: None,
        };
        let mut cached = analyze_decoded_audio(&samples, 8000, &info(), false).unwrap();
        cached.bpm = Some(127.8);
        cached.recording_allowed = true;
        let reused =
            decode::prepare_pcm_cached(samples.clone(), 8000, info(), false, Some(cached.clone()))
                .unwrap();
        assert_eq!(reused.analysis.bpm, Some(127.8));
        assert!(!reused.analysis.recording_allowed);
        cached.duration_seconds += 1.0;
        let fresh =
            decode::prepare_pcm_cached(samples.clone(), 8000, info(), false, Some(cached.clone()))
                .unwrap();
        assert_eq!(fresh.analysis.bpm, None);
        cached.duration_seconds -= 1.0;
        cached.analysis_version = 0;
        let fresh = decode::prepare_pcm_cached(samples, 8000, info(), false, Some(cached)).unwrap();
        assert_eq!(fresh.analysis.bpm, None);
    }
}

#[cfg(test)]
mod transport_performance_tests {
    use super::*;
    use std::{hint::black_box, time::Instant};
    #[test]
    fn metadata_revision_changes_only_on_source_or_analysis_commits() {
        let engine = EngineHandle::new().unwrap();
        assert_eq!(engine.metadata_revision(), 0);
        engine.load_demo(0).unwrap();
        let loaded = engine.metadata_snapshot();
        assert_eq!(loaded.revision, 1);
        assert!(loaded.tracks[0].is_some());
        for command in [
            EngineCommand::Seek {
                deck: 0,
                seconds: 2.5,
            },
            EngineCommand::SetTempo {
                deck: 0,
                percent: 4.0,
            },
            EngineCommand::SetEq {
                deck: 0,
                low: 0.8,
                mid: 1.0,
                high: 1.1,
            },
            EngineCommand::JogTouch {
                deck: 0,
                touched: true,
            },
        ] {
            engine.command(command).unwrap();
        }
        for _ in 0..100 {
            if engine.transport_snapshot().decks[0].jog_touch {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(engine.metadata_revision(), 1);
        let slim = engine.transport_snapshot();
        assert!(slim.decks.iter().all(|deck| deck.track.is_none()));
        assert!(slim.decks[0].jog_touch);
        assert_eq!(slim.decks[0].position_seconds, 2.5);
        assert!(slim.decks[0].position_revision >= 2);
        engine.reanalyze_loaded(0).unwrap();
        assert_eq!(engine.metadata_revision(), 2);
        engine.unload(0).unwrap();
        assert_eq!(engine.metadata_revision(), 3);
        assert!(engine.metadata_snapshot().tracks[0].is_none());
    }
    #[test]
    fn slim_payload_is_small_and_snapshot_benchmark_reports_the_difference() {
        let engine = EngineHandle::new().unwrap();
        engine.load_demo(0).unwrap();
        engine.load_demo(1).unwrap();
        let slim = serde_json::to_vec(&engine.transport_snapshot()).unwrap();
        let full = serde_json::to_vec(&engine.snapshot()).unwrap();
        let metadata = serde_json::to_vec(&engine.metadata_snapshot()).unwrap();
        assert!(slim.len() < 5 * 1024, "{} bytes", slim.len());
        assert!(
            full.len() > 100 * 1024,
            "fixture must contain real waveform metadata"
        );
        assert!(slim.len() * 50 < full.len());
        let iterations = 100;
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(serde_json::to_vec(&engine.transport_snapshot()).unwrap());
        }
        let slim_time = started.elapsed();
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(serde_json::to_vec(&engine.snapshot()).unwrap());
        }
        let full_time = started.elapsed();
        println!(
            "snapshot_benchmark {}",
            serde_json::json!({"iterations":iterations,"transport_bytes":slim.len(),"full_bytes":full.len(),"metadata_bytes":metadata.len(),"transport_mean_us":slim_time.as_secs_f64()*1e6/iterations as f64,"full_mean_us":full_time.as_secs_f64()*1e6/iterations as f64,"waveform_arrays":8,"bins_per_array":4096})
        );
    }
}

#[cfg(test)]
mod instant_double_handle_tests {
    use super::*;
    #[test]
    fn load_pulse_revision_tracks_committed_sources_not_transport_edits() {
        let engine = EngineHandle::new().unwrap();
        assert_eq!(engine.load_revision(0), 0);
        assert_eq!(engine.track_duration(0), None);
        engine.load_demo(0).unwrap();
        let revision = engine.load_revision(0);
        assert_eq!(revision, 1);
        assert!(engine.track_duration(0).unwrap() > 0.);
        engine
            .command(EngineCommand::Seek {
                deck: 0,
                seconds: 1.,
            })
            .unwrap();
        engine
            .command(EngineCommand::SetBpm { deck: 0, bpm: 125. })
            .unwrap();
        assert_eq!(engine.load_revision(0), revision);
        engine.instant_double(1).unwrap();
        assert_eq!(engine.metadata_snapshot().load_revisions, [1, 1]);
        engine.load_demo(0).unwrap(); // Same path still needs another LOAD LED pulse.
        assert_eq!(engine.load_revision(0), 2);
        engine.unload(0).unwrap();
        assert_eq!(engine.load_revision(0), 3);
        assert_eq!(engine.track_duration(0), None);
    }
    #[test]
    fn instant_double_replaces_destination_source_and_preserves_source_capability() {
        let engine = EngineHandle::new().unwrap();
        assert!(engine.instant_double(0).is_err());
        assert!(engine.instant_double(2).is_err());
        let generation = engine.begin_load(0).unwrap();
        let mut source = Arc::try_unwrap(decode::sample_track(0)).ok().unwrap();
        source.analysis.recording_allowed = false;
        source.analysis.bpm_source = Some("manual".into());
        let source = Arc::new(source);
        engine.load_data(0, source.clone(), generation).unwrap();
        let outdated = engine.begin_source_load(1).unwrap();
        let revision = engine.metadata_revision();
        engine.instant_double(1).unwrap();
        assert_eq!(engine.metadata_revision(), revision + 1);
        let metadata = engine.metadata_snapshot();
        let copied = metadata.tracks[1].as_ref().unwrap();
        assert_eq!(copied.id, source.analysis.id);
        assert_eq!(copied.bpm_source.as_deref(), Some("manual"));
        assert!(!copied.recording_allowed);
        let settings = engine.shared.sources.lock().unwrap();
        assert!(Arc::ptr_eq(
            settings[0].as_ref().unwrap(),
            settings[1].as_ref().unwrap()
        ));
        drop(settings);
        assert!(engine
            .load_decoded_source(
                outdated,
                8000,
                vec![[0.1; 2]; 8000],
                AudioSourceInfo {
                    id: "outdated".into(),
                    location: "outdated".into(),
                    title: "outdated".into(),
                    artist: None,
                    extension: None,
                    release_identity: None
                },
                true
            )
            .is_err());
    }
}

#[cfg(test)]
mod playback_bpm_public_control_tests {
    use super::*;
    #[test]
    fn public_playback_bpm_errors_are_clear_and_offline_control_preserves_source() {
        let e = EngineHandle::new().unwrap();
        assert!(e
            .command(EngineCommand::SetPlaybackBpm { deck: 0, bpm: 124. })
            .unwrap_err()
            .contains("requires a valid"));
        e.load_demo(0).unwrap();
        let loaded = e.fence().unwrap();
        e.wait_for_fence(loaded, Duration::from_secs(2)).unwrap();
        let before = e.snapshot();
        let base = before.decks[0].bpm.unwrap();
        assert!(e
            .command(EngineCommand::SetSync {
                deck: 0,
                enabled: true
            })
            .unwrap_err()
            .contains("master deck cannot sync"));
        e.command(EngineCommand::SetPlaybackBpm {
            deck: 0,
            bpm: base * 1.03,
        })
        .unwrap();
        let fence = e.fence().unwrap();
        e.wait_for_fence(fence, Duration::from_secs(2)).unwrap();
        let after = e.snapshot();
        assert!((after.decks[0].playback_bpm.unwrap() - base * 1.03).abs() < 1e-10);
        assert_eq!(after.decks[0].bpm, before.decks[0].bpm);
        assert_eq!(after.decks[0].grid_revision, before.decks[0].grid_revision);
        assert_eq!(
            after.decks[0].track.as_ref().unwrap().bpm,
            before.decks[0].track.as_ref().unwrap().bpm
        );
        assert!(e
            .command(EngineCommand::SetSyncRateLimits {
                deck: 0,
                limits: Some(SyncRateLimits {
                    min_rate: 0.98,
                    max_rate: 1.02,
                    max_slew_per_second: 0.01,
                }),
            })
            .unwrap_err()
            .contains("inside the Sync limits"));
        assert!(e
            .command(EngineCommand::SetPlaybackBpm { deck: 0, bpm: 300. })
            .unwrap_err()
            .contains("outside -90..100"));
    }
}

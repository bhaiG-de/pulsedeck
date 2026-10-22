#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_skills;
mod artwork;
mod audio_preferences;
mod control;
mod controller;
mod import_scan;
mod library;
mod manual_cue_migration;
mod metadata_index;
mod next_move;
mod spotify;
mod spotify_analysis;
mod spotify_decks;
mod spotify_vault;
mod suggest;
mod track_inspection;

use deck_engine::{
    AudioDeviceInfo, AudioRouting, AudioStatus, EngineCommand, EngineHandle, EngineSnapshot,
    RecordingStatus, TrackAnalysis,
};
use flx4_midi::{MidiController, MidiPorts, MidiStatus};
use library::{AnalysisMetadata, Library, Playlist, TrackRecord};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Debug, Default, Serialize)]
struct AnalysisProgress {
    active: bool,
    total: usize,
    completed: usize,
    current_path: Option<String>,
    errors: Vec<String>,
    cancelled: bool,
    analyzing: bool,
}

struct AppState {
    engine: Arc<EngineHandle>,
    library: Mutex<Library>,
    midi: Mutex<MidiController>,
    analysis: Mutex<AnalysisProgress>,
    import_busy: AtomicBool,
    /// Set to stop an import or library analysis between (and inside) tracks.
    import_cancel: AtomicBool,
    data_dir: std::path::PathBuf,
    loading: [AtomicU64; 2],
    local_preparation: [Mutex<()>; 2],
    source_jobs: spotify_decks::SourceJobs,
    spotify_analysis: spotify_analysis::BatchService,
    messages: Mutex<Vec<String>>,
    controller: Mutex<controller::ControlState>,
    midi_autoconnect: AtomicBool,
    audio_status: Mutex<Option<AudioStatus>>,
    audio_devices: Mutex<Vec<AudioDeviceInfo>>,
    audio_discovering: AtomicBool,
    audio_operation: Mutex<()>,
    audio_generation: AtomicU64,
    audio_preferences_path: std::path::PathBuf,
    /// Tracks heard this session, left out of next-track suggestions.
    play_history: Mutex<suggest::PlayHistory>,
    /// Set while older records get their suggestion features.
    features_backfill: AtomicBool,
}

impl AppState {
    fn current_audio_status(&self) -> Result<Option<AudioStatus>, String> {
        let mut status = self.audio_status.lock().map_err(|_| poisoned())?.clone();
        if let Some(status) = &mut status {
            status.running = self.engine.transport_snapshot().running;
        }
        Ok(status)
    }

    fn message(&self, text: impl Into<String>) {
        if let Ok(mut messages) = self.messages.lock() {
            messages.push(text.into());
            if messages.len() > 20 {
                messages.remove(0);
            }
        }
    }
}

#[derive(Serialize)]
struct SmartModes {
    fader: bool,
    cfx: bool,
}

type PersistedDeckState = (String, [Option<f64>; 8], [Option<f64>; 10], f64, f64, f64);
/// A deck's hot cue pads and memory cues.
#[derive(Serialize)]
struct AppSnapshot {
    engine: EngineSnapshot,
    library: Vec<TrackRecord>,
    playlists: Vec<Playlist>,
    audio_devices: Vec<AudioDeviceInfo>,
    audio_status: Option<AudioStatus>,
    midi: MidiStatus,
    midi_ports: MidiPorts,
    analysis: AnalysisProgress,
    messages: Vec<String>,
    smart_modes: SmartModes,
    spotify_decks: [spotify_decks::DeckSourceStatus; 2],
}

fn poisoned() -> String {
    "Internal state is unavailable".into()
}

/// Whether all persisted source stages are current, including terminal failures.
pub(crate) fn source_analysis_current(analysis: &TrackAnalysis) -> bool {
    deck_engine::analysis_current(analysis)
}

fn get_app_state(state: State<'_, AppState>, include_library: bool) -> Result<AppSnapshot, String> {
    let (library, playlists) = {
        let lib = state.library.lock().map_err(|_| poisoned())?;
        (
            if include_library {
                lib.tracks()?
            } else {
                Vec::new()
            },
            lib.playlists()?,
        )
    };
    let modes = {
        let control = state.controller.lock().map_err(|_| poisoned())?;
        SmartModes {
            fader: control.smart_fader,
            cfx: control.smart_cfx,
        }
    };
    Ok(AppSnapshot {
        engine: state.engine.snapshot(),
        library,
        playlists,
        audio_devices: state.audio_devices.lock().map_err(|_| poisoned())?.clone(),
        audio_status: state.current_audio_status()?,
        midi: state.midi.lock().map_err(|_| poisoned())?.status(),
        midi_ports: flx4_midi::list_ports().unwrap_or_default(),
        analysis: state.analysis.lock().map_err(|_| poisoned())?.clone(),
        messages: state.messages.lock().map_err(|_| poisoned())?.clone(),
        smart_modes: modes,
        spotify_decks: state.source_jobs.snapshot(),
    })
}

#[derive(Serialize)]
struct ActivitySnapshot {
    midi: MidiStatus,
    analysis: AnalysisProgress,
    messages: Vec<String>,
    smart_modes: SmartModes,
    spotify_decks: [spotify_decks::DeckSourceStatus; 2],
    audio_status: Option<AudioStatus>,
    audio_devices: Vec<AudioDeviceInfo>,
}
fn get_activity_state(state: State<'_, AppState>) -> Result<ActivitySnapshot, String> {
    let smart_modes = {
        let control = state.controller.lock().map_err(|_| poisoned())?;
        SmartModes {
            fader: control.smart_fader,
            cfx: control.smart_cfx,
        }
    };
    Ok(ActivitySnapshot {
        audio_status: state.current_audio_status()?,
        audio_devices: state.audio_devices.lock().map_err(|_| poisoned())?.clone(),
        midi: state.midi.lock().map_err(|_| poisoned())?.status(),
        analysis: state.analysis.lock().map_err(|_| poisoned())?.clone(),
        messages: state.messages.lock().map_err(|_| poisoned())?.clone(),
        smart_modes,
        spotify_decks: state.source_jobs.snapshot(),
    })
}

async fn list_audio_devices(app: tauri::AppHandle) -> Result<Vec<AudioDeviceInfo>, String> {
    let state = app.state::<AppState>();
    if state.audio_discovering.swap(true, Ordering::AcqRel) {
        let cached = state.audio_devices.lock().map_err(|_| poisoned())?.clone();
        return if cached.is_empty() {
            Err(
                "Audio device discovery is still waiting for macOS; the decks remain available"
                    .into(),
            )
        } else {
            Ok(cached)
        };
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker_app = app.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("pulsedeck-audio-discovery".into())
        .spawn(move || {
            let result = deck_engine::devices();
            let state = worker_app.state::<AppState>();
            if let Ok(devices) = &result {
                if let Ok(mut cache) = state.audio_devices.lock() {
                    *cache = devices.clone();
                }
            }
            state.audio_discovering.store(false, Ordering::Release);
            let _ = sender.send(result);
        })
    {
        state.audio_discovering.store(false, Ordering::Release);
        return Err(error.to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .map_err(|_| {
                "macOS audio discovery has not responded; try Refresh devices when it is ready"
                    .to_string()
            })?
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn start_audio(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    device_name: Option<String>,
    buffer_frames: Option<u32>,
    routing: AudioRouting,
) -> Result<AudioStatus, String> {
    let generation = state.audio_generation.fetch_add(1, Ordering::AcqRel) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _operation = state.audio_operation.lock().map_err(|_| poisoned())?;
        if state.audio_generation.load(Ordering::Acquire) != generation {
            return Err("Audio change superseded by a newer request".into());
        }
        let status = state
            .engine
            .start_audio(device_name, buffer_frames, routing)?;
        if state.audio_generation.load(Ordering::Acquire) == generation {
            *state.audio_status.lock().map_err(|_| poisoned())? = Some(status.clone());
            audio_preferences::save(
                &state.audio_preferences_path,
                &audio_preferences::Preferences::from_status(&status),
            )?;
        }
        Ok(status)
    })
    .await
    .map_err(|e| e.to_string())?
}
async fn stop_audio(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let generation = state.audio_generation.fetch_add(1, Ordering::AcqRel) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _operation = state.audio_operation.lock().map_err(|_| poisoned())?;
        if state.audio_generation.load(Ordering::Acquire) != generation {
            return Ok(());
        }
        state.engine.stop_audio();
        if let Some(status) = state.audio_status.lock().map_err(|_| poisoned())?.as_mut() {
            status.running = false;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn choose_tracks(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(app
            .dialog()
            .file()
            .add_filter(
                "Audio",
                &[
                    "wav", "aiff", "aif", "mp3", "flac", "m4a", "aac", "ogg", "alac",
                ],
            )
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn choose_folder(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(folder) = app.dialog().file().blocking_pick_folder() else {
            return Ok(Vec::new());
        };
        let folder = folder.into_path().map_err(|error| error.to_string())?;
        import_scan::audio_paths(&folder)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn analysis_record(analysis: TrackAnalysis) -> TrackRecord {
    TrackRecord {
        id: 0,
        path: analysis.path,
        title: analysis.title,
        artist: analysis.artist.unwrap_or_default(),
        tags: Vec::new(),
        file_metadata: Vec::new(),
        duration_seconds: analysis.duration_seconds,
        bpm: analysis.bpm.unwrap_or(0.),
        key: analysis.key,
        sample_rate: analysis.sample_rate,
        waveform: analysis.waveform,
        added_at: String::new(),
        hot_cues: vec![None; 8],
        memory_cues: vec![None; 10],
        cue_seconds: 0.,
        camelot: analysis.camelot,
        key_confidence: analysis.key_confidence,
        beatgrid_offset_seconds: analysis.beatgrid_offset_seconds.unwrap_or(0.0),
        analysis: AnalysisMetadata {
            analysis_version: analysis.analysis_version,
            bpm_confidence: analysis.bpm_confidence,
            bpm_candidates: analysis.bpm_candidates,
            bpm_ambiguous: analysis.bpm_ambiguous,
            bpm_source: analysis.bpm_source,
            key_candidates: analysis.key_candidates,
            waveform_bands: analysis.waveform_bands,
            beatgrid_confidence: analysis.beatgrid_confidence,
            beatgrid_source: analysis.beatgrid_source,
            beat_model: analysis.beat_model,

            cue_points: analysis.cue_points,
            source_fingerprint: analysis.source_fingerprint,
            source_release: analysis.source_release,
            audio_measurements: analysis.audio_measurements,
            analysis_stages: analysis.analysis_stages,
            neural_rhythm: analysis.neural_rhythm,
            rhythm_error: analysis.rhythm_error,
            manual_grid_anchor_index: analysis.manual_grid_anchor_index,
        },
    }
}

/// A local track's stored analysis, reused when the file is loaded again. The
/// engine still checks it against the decoded audio before trusting it.
fn record_analysis(record: TrackRecord) -> TrackAnalysis {
    TrackAnalysis {
        id: record.path.clone(),
        path: record.path,
        title: record.title,
        artist: (!record.artist.is_empty()).then_some(record.artist),
        duration_seconds: record.duration_seconds,
        sample_rate: record.sample_rate,
        channels: 2,
        recording_allowed: true,
        bpm: (record.bpm.is_finite() && record.bpm > 0.0).then_some(record.bpm),
        bpm_confidence: record.analysis.bpm_confidence,
        analysis_version: record.analysis.analysis_version,
        bpm_candidates: record.analysis.bpm_candidates,
        bpm_ambiguous: record.analysis.bpm_ambiguous,
        bpm_source: record.analysis.bpm_source,
        beatgrid_offset_seconds: Some(record.beatgrid_offset_seconds),
        beatgrid_confidence: record.analysis.beatgrid_confidence,
        beatgrid_source: record.analysis.beatgrid_source,
        key_candidates: record.analysis.key_candidates,
        key: record.key,
        key_confidence: record.key_confidence,
        camelot: record.camelot,
        waveform_bands: record.analysis.waveform_bands,
        waveform: record.waveform,
        neural_rhythm: record.analysis.neural_rhythm,
        rhythm_error: record.analysis.rhythm_error,
        manual_grid_anchor_index: record.analysis.manual_grid_anchor_index,
        beat_model: record.analysis.beat_model,

        cue_points: record.analysis.cue_points,
        source_fingerprint: record.analysis.source_fingerprint,
        source_release: record.analysis.source_release,
        audio_measurements: record.analysis.audio_measurements,
        analysis_stages: record.analysis.analysis_stages,
    }
}

/// Keep manual BPM and grid edits stored in the Spotify cache entry.
fn keep_spotify_manual(
    mut analysis: TrackAnalysis,
    stored: Option<TrackAnalysis>,
) -> TrackAnalysis {
    if let Some(stored) = stored {
        if stored.bpm_source.as_deref() == Some("manual") {
            analysis.bpm = stored.bpm;
            analysis.bpm_source = stored.bpm_source;
            analysis.bpm_confidence = 1.0;
            analysis.bpm_ambiguous = false;
        }
        if stored.beatgrid_source.as_deref() == Some("manual") {
            analysis.beatgrid_offset_seconds = stored.beatgrid_offset_seconds;
            analysis.beatgrid_source = stored.beatgrid_source;
            analysis.beatgrid_confidence = None;
            analysis.manual_grid_anchor_index = analysis
                .manual_grid_anchor_index
                .or(stored.manual_grid_anchor_index);
        }
    }
    analysis
}

fn apply_grid(
    state: &AppState,
    deck: usize,
    bpm: Option<f64>,
    offset: Option<f64>,
) -> Result<(), String> {
    if let Some(bpm) = bpm.filter(|b| b.is_finite() && (30.0..=300.0).contains(b)) {
        state.engine.command(EngineCommand::SetBpm { deck, bpm })?;
    }
    if let Some(offset_seconds) = offset.filter(|o| o.is_finite()) {
        state.engine.command(EngineCommand::SetBeatgrid {
            deck,
            offset_seconds,
            anchor_index: None,
        })?;
    }
    Ok(())
}

/// Save a loaded track's analysis: the library for local files (manual BPM and
/// grid kept by `upsert`), the Spotify cache otherwise (manual edits merged).
fn persist_analysis(
    app: &tauri::AppHandle,
    state: &AppState,
    analysis: &TrackAnalysis,
) -> Result<(), String> {
    if analysis.path.starts_with("spotify:") {
        let stored = state
            .library
            .lock()
            .map_err(|_| poisoned())?
            .spotify_analysis(&analysis.path)?;
        let merged = keep_spotify_manual(analysis.clone(), stored);
        spotify_analysis::store(app, &merged)?;
        Ok(())
    } else {
        state
            .library
            .lock()
            .map_err(|_| poisoned())?
            .upsert(&analysis_record(analysis.clone()))?;
        Ok(())
    }
}

fn import_one(state: &AppState, path: &str) -> Result<TrackRecord, String> {
    let path = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    let path_string = path.to_string_lossy().into_owned();
    let existing = state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .track_by_path(&path_string)?;
    let mut record = if let Some(existing) = existing {
        existing
    } else {
        let record = analysis_record(deck_engine::probe_file(&path)?);
        state
            .library
            .lock()
            .map_err(|_| poisoned())?
            .import_metadata(&record)?
    };
    metadata_index::refresh(state, &mut record)?;
    Ok(record)
}

fn analyze_one(state: &AppState, path: &str) -> Result<TrackRecord, String> {
    let path = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    if !path.is_file() {
        return Err("Select a music file".into());
    }
    let path_string = path.to_string_lossy().into_owned();
    let existing = state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .track_by_path(&path_string)?;
    if let Some(existing) = existing.as_ref() {
        if existing.analysis.analysis_version >= audio_analysis::ANALYSIS_VERSION
            && existing.analysis.waveform_bands.is_some()
            && existing
                .analysis
                .beat_model
                .as_ref()
                .is_some_and(|stage| stage.version >= audio_analysis::BEAT_MODEL_VERSION)
        {
            return Ok(existing.clone());
        }
    }
    let cancel = || state.import_cancel.load(Ordering::Acquire);
    let (analysis, warning) =
        deck_engine::analyze_file_cached(&path, existing.map(record_analysis), &cancel)?;
    if let Some(warning) = warning {
        if let Ok(mut p) = state.analysis.lock() {
            p.errors.push(format!("{path_string}: {warning}"));
        }
    }
    let lib = state.library.lock().map_err(|_| poisoned())?;
    let record = lib.upsert(&analysis_record(analysis))?;
    Ok(record)
}

struct ImportGuard<'a>(&'a AppState);
impl Drop for ImportGuard<'_> {
    fn drop(&mut self) {
        self.0.import_busy.store(false, Ordering::Release);
        if let Ok(mut p) = self.0.analysis.lock() {
            p.active = false;
            p.current_path = None;
        }
    }
}

fn import_batch<T>(
    app: &tauri::AppHandle,
    paths: Vec<String>,
    analyzing: bool,
    project: impl Fn(TrackRecord) -> T,
) -> Result<Vec<T>, String> {
    let state = app.state::<AppState>();
    if state.import_busy.swap(true, Ordering::AcqRel) {
        return Err("A library import is already running".into());
    }
    let _guard = ImportGuard(&state);
    state.import_cancel.store(false, Ordering::Release);
    *state.analysis.lock().map_err(|_| poisoned())? = AnalysisProgress {
        active: true,
        total: paths.len(),
        analyzing,
        ..Default::default()
    };
    let mut results = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        if state.import_cancel.load(Ordering::Acquire) {
            if let Ok(mut p) = state.analysis.lock() {
                p.cancelled = true;
            }
            break;
        }
        if let Ok(mut p) = state.analysis.lock() {
            p.current_path = Some(path.clone());
        }
        let result = if analyzing {
            analyze_one(&state, &path)
        } else {
            import_one(&state, &path)
        };
        match result {
            Ok(record) => results.push(project(record)),
            Err(_) if state.import_cancel.load(Ordering::Acquire) => continue,
            Err(e) => {
                let e = format!("{path}: {e}");
                state.message(e.clone());
                if let Ok(mut p) = state.analysis.lock() {
                    p.errors.push(e);
                }
            }
        }
        if let Ok(mut p) = state.analysis.lock() {
            p.completed = index + 1;
        }
    }
    Ok(results)
}

async fn import_tracks(
    app: tauri::AppHandle,
    paths: Vec<String>,
    summary: Option<bool>,
) -> Result<serde_json::Value, String> {
    if paths.len() > 10_000 {
        return Err("Import at most 10,000 tracks in one batch".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        if summary.unwrap_or(false) {
            import_batch(
                &app,
                paths,
                false,
                |record| serde_json::json!({"id":record.id}),
            )
            .map(serde_json::Value::Array)
        } else {
            serde_json::to_value(import_batch(&app, paths, false, std::convert::identity)?)
                .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Re-analyze local tracks that lack the current beat-model stage, through the
/// same cancellable worker and progress as an import.
async fn update_library_analysis(app: tauri::AppHandle) -> Result<Vec<TrackRecord>, String> {
    let paths = app
        .state::<AppState>()
        .library
        .lock()
        .map_err(|_| poisoned())?
        .paths_needing_analysis()?;
    tauri::async_runtime::spawn_blocking(move || {
        import_batch(&app, paths, true, std::convert::identity)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn cancel_library_analysis(state: State<'_, AppState>) {
    state.import_cancel.store(true, Ordering::Release);
}

fn apply_library_names(engine: &EngineHandle, path: &str, title: &str, artist: &str) {
    let snapshot = engine.metadata_summary_snapshot();
    for (deck, track) in snapshot.tracks.iter().enumerate() {
        if let Some(track) = track.as_ref().filter(|track| {
            track.path == path
                && (track.title != title || track.artist.as_deref().unwrap_or("") != artist)
        }) {
            // A source switch after the snapshot must not rename the new deck.
            let _ = engine.update_loaded_analysis_for_revision(
                deck,
                &track.id,
                Some(snapshot.load_revisions[deck]),
                |analysis| {
                    analysis.title = title.into();
                    analysis.artist = (!artist.is_empty()).then(|| artist.to_owned());
                },
            );
        }
    }
}

fn load_into_deck(
    app: &tauri::AppHandle,
    deck: usize,
    path: &Path,
) -> Result<TrackAnalysis, String> {
    load_into_deck_owned(app, deck, path, None, None)
}
fn load_into_deck_owned(
    app: &tauri::AppHandle,
    deck: usize,
    path: &Path,
    owner: Option<&str>,
    ticket: Option<deck_engine::SourceLoadTicket>,
) -> Result<TrackAnalysis, String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    let state = app.state::<AppState>();
    if owner.is_none() {
        state.source_jobs.cancel(deck);
    }
    let check_owner = || -> Result<(), String> {
        if owner.is_some_and(|owner| control::request_owner_cancelled(app, owner)) {
            Err("Queued local source preparation was cancelled".into())
        } else {
            Ok(())
        }
    };
    check_owner()?;
    let request = state.loading[deck].fetch_add(1, Ordering::AcqRel) + 1;
    let _preparation = state.local_preparation[deck]
        .lock()
        .map_err(|_| poisoned())?;
    if state.loading[deck].load(Ordering::Acquire) != request {
        return Err("Track load was replaced by a newer request".into());
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let cached = state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .track_by_path(&path.to_string_lossy())?
        .map(record_analysis);
    // The engine's own generation checks protect late decode results. Parent
    // generation also avoids applying persisted metadata for a superseded load.
    check_owner()?;
    let analysis = if let Some(ticket) = ticket {
        state
            .engine
            .load_track_cached_for_ticket(ticket, &path, cached)?
    } else {
        state.engine.load_track_cached(deck, &path, cached)?
    };
    check_owner()?;
    let record = state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .upsert(&analysis_record(analysis.clone()))?;
    if state.loading[deck].load(Ordering::Acquire) != request {
        return Err("Track load was replaced by a newer request".into());
    }
    let analysis = if analysis.title != record.title
        || analysis.artist.as_deref().unwrap_or("") != record.artist
    {
        state
            .engine
            .update_loaded_analysis(deck, &analysis.id, |current| {
                current.title = record.title.clone();
                current.artist = (!record.artist.is_empty()).then(|| record.artist.clone());
            })?
    } else {
        analysis
    };
    if matches!(
        record.analysis.bpm_source.as_deref(),
        Some("manual" | "import")
    ) && record.bpm.is_finite()
        && (30.0..=300.0).contains(&record.bpm)
    {
        state.engine.command(EngineCommand::SetBpm {
            deck,
            bpm: record.bpm,
        })?;
    }
    if record.analysis.beatgrid_source.as_deref() == Some("manual") {
        state.engine.command(EngineCommand::SetBeatgrid {
            anchor_index: record.analysis.manual_grid_anchor_index,
            deck,
            offset_seconds: record.beatgrid_offset_seconds,
        })?;
    }
    let cues: [Option<f64>; 8] = record.hot_cues.try_into().unwrap_or([None; 8]);
    state
        .engine
        .command(EngineCommand::RestoreHotCues { deck, cues })?;
    let memory_cues: [Option<f64>; 10] = record.memory_cues.try_into().unwrap_or([None; 10]);
    state.engine.command(EngineCommand::RestoreMemoryCues {
        deck,
        cues: memory_cues,
    })?;
    state.engine.command(EngineCommand::SetCuePoint {
        deck,
        seconds: record.cue_seconds,
    })?;
    if !deck_engine::playback_analysis_ready(&analysis) {
        check_owner()?;
        let completed = state.engine.complete_loaded_analysis(deck)?;
        check_owner()?;
        if state.loading[deck].load(Ordering::Acquire) != request {
            return Err("Track load was replaced during analysis".into());
        }
        persist_analysis(app, &state, &completed)?;
        state
            .engine
            .wait_for_fence(state.engine.fence()?, std::time::Duration::from_secs(2))?;
        if !deck_engine::playback_analysis_ready(&completed) {
            return Err(
                "Analysis could not prepare a usable BPM and waveform; inspect analysis or retry"
                    .into(),
            );
        }
        return Ok(completed);
    }
    Ok(analysis)
}

async fn load_track_owned(
    app: tauri::AppHandle,
    deck: usize,
    path: String,
    owner: String,
) -> Result<TrackAnalysis, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if control::request_owner_cancelled(&app, &owner) {
            return Err("Queued local source preparation was cancelled".into());
        }
        let ticket = state.engine.begin_source_load(deck)?;
        let cancel = state
            .source_jobs
            .start(deck, path.clone(), Some(owner.clone()))?;
        state
            .source_jobs
            .attach_ticket(deck, &cancel, ticket.clone());
        let result = load_into_deck_owned(&app, deck, Path::new(&path), Some(&owner), Some(ticket));
        state
            .source_jobs
            .finish(deck, &cancel, result.as_ref().err().cloned());
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn reanalyze_loaded_track(
    app: tauri::AppHandle,
    deck: usize,
) -> Result<TrackAnalysis, String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let request = state.loading[deck].load(Ordering::Acquire);
        let analysis = state.engine.reanalyze_loaded(deck)?;
        if state.loading[deck].load(Ordering::Acquire) != request {
            return Err("A new track replaced this analysis request".into());
        }
        let chosen_bpm = if analysis.path.starts_with("spotify:") {
            let stored = state
                .library
                .lock()
                .map_err(|_| poisoned())?
                .spotify_analysis(&analysis.path)?;
            let stored_override = stored
                .as_ref()
                .filter(|a| a.bpm_source.as_deref() == Some("manual"))
                .and_then(|a| a.bpm);
            let manual_bpm = stored_override;
            let mut cached = analysis.clone();
            if let Some(bpm) = manual_bpm {
                cached.bpm = Some(bpm);
                cached.bpm_source = Some("manual".into());
                cached.bpm_confidence = 1.0;
                cached.bpm_ambiguous = false;
            }
            if let Some(grid) = stored.filter(|a| a.beatgrid_source.as_deref() == Some("manual")) {
                cached.beatgrid_offset_seconds = grid.beatgrid_offset_seconds;
                cached.beatgrid_source = grid.beatgrid_source;
                cached.beatgrid_confidence = None;
                cached.manual_grid_anchor_index = analysis
                    .manual_grid_anchor_index
                    .or(grid.manual_grid_anchor_index);
            }
            spotify_analysis::store(&app, &cached)?;
            manual_bpm
        } else {
            let record = state
                .library
                .lock()
                .map_err(|_| poisoned())?
                .upsert(&analysis_record(analysis.clone()))?;
            (record.analysis.bpm_source.as_deref() == Some("manual")
                && record.bpm.is_finite()
                && (30.0..=300.0).contains(&record.bpm))
            .then_some(record.bpm)
        };
        // Only explicit overrides issue tempo/grid commands. Model event timing
        // reaches the render clock through the engine's revision-guarded UpdateGrid.
        if let Some(bpm) = chosen_bpm {
            apply_grid(&state, deck, Some(bpm), None)?;
        }
        Ok(analysis)
    })
    .await
    .map_err(|_| "Track analysis worker failed".to_string())?
}

async fn load_demo(app: tauri::AppHandle, deck: usize) -> Result<TrackAnalysis, String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    app.state::<AppState>().source_jobs.cancel(deck);
    app.state::<AppState>().loading[deck].fetch_add(1, Ordering::AcqRel);
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().engine.load_demo(deck))
        .await
        .map_err(|e| e.to_string())?
}

fn engine_command_inner(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    command: EngineCommand,
    smart_modes: bool,
    human_control: bool,
    expected_revision: Option<u64>,
) -> Result<(), String> {
    if command
        .deck()
        .is_some_and(|d| expected_revision.is_some_and(|r| state.engine.load_revision(d) != r))
    {
        return Err("Loaded source revision conflict".into());
    }
    if let EngineCommand::SetCrossfader { position } = command {
        if !position.is_finite() || !(-1.0..=1.0).contains(&position) {
            return Err("Crossfader position must be between -1 and 1".into());
        }
        let smart = state.controller.lock().map_err(|_| poisoned())?.smart_fader;
        if smart && smart_modes {
            controller::handle(
                &app,
                flx4_midi::ControllerAction::Knob {
                    deck: None,
                    knob: flx4_midi::Knob::Crossfader,
                    raw: ((position + 1.) * 0.5 * 16383.) as u16,
                    value: (position + 1.) * 0.5,
                },
            )?;
            return Ok(());
        }
    }
    if let EngineCommand::SetFilter { deck, amount } = command {
        if deck > 1 || !amount.is_finite() || !(-1.0..=1.0).contains(&amount) {
            return Err("Choose deck 0/1 and a filter amount between -1 and 1".into());
        }
        let smart = state.controller.lock().map_err(|_| poisoned())?.smart_cfx;
        if smart && smart_modes {
            controller::handle(
                &app,
                flx4_midi::ControllerAction::Knob {
                    deck: Some(deck),
                    knob: flx4_midi::Knob::Filter,
                    raw: ((amount + 1.) * 0.5 * 16383.) as u16,
                    value: (amount + 1.) * 0.5,
                },
            )?;
            return Ok(());
        }
    }
    let manual_bpm = if let EngineCommand::SetBpm { deck, bpm } = &command {
        state
            .engine
            .snapshot()
            .decks
            .get(*deck)
            .and_then(|d| d.track.as_ref())
            .map(|t| (t.clone(), *bpm))
    } else {
        None
    };
    let manual_grid = if let EngineCommand::SetBeatgrid {
        deck,
        offset_seconds,
        ..
    } = &command
    {
        state
            .engine
            .snapshot()
            .decks
            .get(*deck)
            .and_then(|d| d.track.clone())
            .map(|t| (t, *offset_seconds))
    } else {
        None
    };
    state
        .engine
        .command_for_revision(command.clone(), expected_revision)?;
    control::mix_session_control(&app, &command, human_control);
    state
        .controller
        .lock()
        .map_err(|_| poisoned())?
        .observe(&command, smart_modes);
    if let Some((mut track, offset)) = manual_grid {
        let fence = state.engine.fence()?;
        state
            .engine
            .wait_for_fence(fence, std::time::Duration::from_secs(2))?;
        let deck = command.deck().ok_or("Grid edit requires a deck")?;
        if state.engine.snapshot().decks[deck]
            .track
            .as_ref()
            .is_none_or(|t| t.path != track.path)
        {
            return Err("Loaded source revision conflict during grid persistence".into());
        }
        let anchor = state.engine.transport_snapshot().decks[deck].beatgrid_anchor_index;
        if track.path.starts_with("spotify:") {
            let previous = state
                .library
                .lock()
                .map_err(|_| poisoned())?
                .spotify_analysis(&track.path)?;
            if let Some(cached) = previous {
                track = cached;
            }
            track.beatgrid_offset_seconds = Some(offset);
            track.beatgrid_source = Some("manual".into());
            track.beatgrid_confidence = None;
            track.manual_grid_anchor_index = anchor;
            spotify_analysis::store(&app, &track)?;
        } else {
            state.library.lock().map_err(|_| poisoned())?.manual_grid(
                &track.path,
                offset,
                anchor,
            )?;
        }
    }
    if let Some((mut track, bpm)) = manual_bpm {
        if track.path.starts_with("spotify:") {
            if let Some(cached) = state
                .library
                .lock()
                .map_err(|_| poisoned())?
                .spotify_analysis(&track.path)?
            {
                track = cached;
            }
            track.bpm = Some(bpm);
            track.bpm_source = Some("manual".into());
            track.bpm_confidence = 1.0;
            track.bpm_ambiguous = false;
            spotify_analysis::store(&app, &track)?;
        } else {
            state.library.lock().map_err(|_| poisoned())?.metadata(
                &track.path,
                Some(bpm),
                None,
                None,
            )?;
        }
    }
    Ok(())
}

fn set_controller_selection(state: State<'_, AppState>, track_id: i64) -> Result<(), String> {
    let mut control = state.controller.lock().map_err(|_| poisoned())?;
    let index = if control.browse_context.is_empty() {
        state
            .library
            .lock()
            .map_err(|_| poisoned())?
            .track_ids()?
            .iter()
            .position(|id| *id == track_id)
    } else {
        control.browse_context.iter().position(|id| *id == track_id)
    };
    control.selected = index.ok_or("Selected track is not in the current browser")?;
    Ok(())
}

fn set_controller_browse_context(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
    selected_track_id: Option<i64>,
) -> Result<(), String> {
    let mut control = state.controller.lock().map_err(|_| poisoned())?;
    control.set_browse_context(track_ids, selected_track_id)
}

fn get_controller_surface(state: State<'_, AppState>) -> controller::ControllerSurface {
    state
        .controller
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .surface()
}
fn set_controller_tempo_range(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    deck: usize,
    range: f64,
) -> Result<(), String> {
    use tauri::Emitter;
    let surface = {
        let mut control = state.controller.lock().map_err(|_| poisoned())?;
        control.set_range(deck, range)?;
        control.surface()
    };
    let _ = app.emit("controller_surface", surface);
    let _ = app.emit(
        "controller_tempo_range",
        serde_json::json!({"deck":deck,"range":range}),
    );
    Ok(())
}
fn set_controller_browser_active(state: State<'_, AppState>, active: bool) -> Result<(), String> {
    state
        .controller
        .lock()
        .map_err(|_| poisoned())?
        .browser_ui_active = active;
    Ok(())
}
async fn instant_double(app: tauri::AppHandle, deck: usize) -> Result<(), String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    let state = app.state::<AppState>();
    state.source_jobs.cancel(deck);
    state.loading[deck].fetch_add(1, Ordering::AcqRel);
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().engine.instant_double(deck)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn set_smart_modes(
    state: State<'_, AppState>,
    fader: bool,
    cfx: bool,
) -> Result<SmartModes, String> {
    let mut c = state.controller.lock().map_err(|_| poisoned())?;
    c.smart_fader = fader;
    c.smart_cfx = cfx;
    Ok(SmartModes { fader, cfx })
}

fn list_midi_ports() -> Result<MidiPorts, String> {
    flx4_midi::list_ports()
}

fn connect_midi(
    state: State<'_, AppState>,
    input_name: String,
    output_name: Option<String>,
) -> Result<MidiStatus, String> {
    let mut midi = state.midi.lock().map_err(|_| poisoned())?;
    midi.connect(input_name, output_name)?;
    state.midi_autoconnect.store(true, Ordering::Release);
    if midi
        .status()
        .input_name
        .as_ref()
        .is_some_and(|n| n.to_uppercase().contains("DDJ-FLX4"))
        && midi.status().feedback_enabled
    {
        for deck in 0..2 {
            let _ = midi.set_vinyl_mode(deck, true);
        }
    }
    Ok(midi.status())
}

fn disconnect_midi(state: State<'_, AppState>) -> Result<MidiStatus, String> {
    state.midi_autoconnect.store(false, Ordering::Release);
    let mut m = state.midi.lock().map_err(|_| poisoned())?;
    m.disconnect();
    let status = m.status();
    drop(m);
    state
        .controller
        .lock()
        .map_err(|_| poisoned())?
        .release_jogs(&state.engine);
    Ok(status)
}

fn get_midi_history(state: State<'_, AppState>) -> Result<Vec<flx4_midi::RawMidiEvent>, String> {
    Ok(state.midi.lock().map_err(|_| poisoned())?.history())
}

fn create_playlist(state: State<'_, AppState>, name: String) -> Result<Playlist, String> {
    state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .create_playlist(&name)
}

fn add_to_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_id: i64,
) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .add_to_playlist(playlist_id, track_id)
}

fn delete_track(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .delete_track(id)
}

async fn start_recording_at(
    app: tauri::AppHandle,
    path: Option<String>,
    overwrite: bool,
) -> Result<Option<RecordingStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = if let Some(path) = path {
            std::path::PathBuf::from(path)
        } else {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("WAV recording", &["wav"])
                .set_file_name("PulseDeck-mix.wav")
                .blocking_save_file()
            else {
                return Ok(None);
            };
            file.into_path().map_err(|e| e.to_string())?
        };
        app.state::<AppState>()
            .engine
            .start_recording_with_policy(&path, overwrite)
            .map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn stop_recording(app: tauri::AppHandle) -> Result<RecordingStatus, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().engine.stop_recording())
        .await
        .map_err(|e| e.to_string())?
}

async fn load_sample_at(
    app: tauri::AppHandle,
    slot: usize,
    path: Option<String>,
) -> Result<Option<TrackAnalysis>, String> {
    if slot >= 16 {
        return Err("Sample slot must be 0 through 15".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = if let Some(path) = path {
            std::path::PathBuf::from(path)
        } else {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter(
                    "Audio sample",
                    &["wav", "aiff", "aif", "mp3", "flac", "m4a", "aac", "ogg"],
                )
                .blocking_pick_file()
            else {
                return Ok(None);
            };
            file.into_path().map_err(|e| e.to_string())?
        };
        app.state::<AppState>()
            .engine
            .load_sample(slot, &path)
            .map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn write_library_metadata(
    app: tauri::AppHandle,
    id: i64,
    revision: i64,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move||{
        let state=app.state::<AppState>();
        if state.import_busy.swap(true,Ordering::AcqRel){return Err("A library import or metadata write is already running".into());}
        let _guard=ImportGuard(&state);
        let (path,edits)=state.library.lock().map_err(|_|poisoned())?.metadata_write_edits(id,revision)?;
        let backup=deck_engine::write_file_metadata(std::path::Path::new(&path),&edits)?;
        let mut record=state.library.lock().map_err(|_|poisoned())?.track_by_path(&path)?.ok_or("Track no longer exists")?;
        metadata_index::refresh(&state,&mut record)?;
        Ok(serde_json::json!({"backup":backup,"details":state.library.lock().map_err(|_|poisoned())?.track_details(id)?}))
    }).await.map_err(|e|e.to_string())?
}

async fn import_rekordbox_xml(
    app: tauri::AppHandle,
    path: Option<String>,
    summary: Option<bool>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = if let Some(path) = path {
            std::path::PathBuf::from(path)
        } else {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("rekordbox XML", &["xml"])
                .blocking_pick_file()
            else {
                return Ok(serde_json::json!([]));
            };
            file.into_path().map_err(|e| e.to_string())?
        };
        let xml = library::parse_rekordbox_xml(&path)?;
        let state = app.state::<AppState>();
        if state.import_busy.swap(true, Ordering::AcqRel) {
            return Err("An import is already running".into());
        }
        let _guard = ImportGuard(&state);
        state.import_cancel.store(false, Ordering::Release);
        *state.analysis.lock().map_err(|_| poisoned())? = AnalysisProgress {
            active: true,
            total: xml.collection.tracks.len(),
            ..Default::default()
        };
        if let Some(taxonomy) = &xml.taxonomy {
            state
                .library
                .lock()
                .map_err(|_| poisoned())?
                .import_taxonomy_json(taxonomy)?;
        }
        let mut records = Vec::new();
        let mut track_ids = HashMap::new();
        for (i, entry) in xml.collection.tracks.into_iter().enumerate() {
            if state.import_cancel.load(Ordering::Acquire) {
                if let Ok(mut p) = state.analysis.lock() {
                    p.cancelled = true;
                }
                break;
            }
            if let Ok(mut p) = state.analysis.lock() {
                p.current_path = Some(entry.location.clone());
            }
            let result = (|| {
                let path = library::xml_path(&entry.location)?;
                let mut record = import_one(&state, &path.to_string_lossy())?;
                {
                    let lib = state.library.lock().map_err(|_| poisoned())?;
                    lib.import_xml_cues(&record, &entry)?;
                    lib.import_xml_details(record.id, &entry.attributes)?;
                    lib.tags(
                        &record.path,
                        &entry.name,
                        &entry.artist,
                        entry.key.as_deref().filter(|key| !key.is_empty()),
                    )?;
                }
                if !summary.unwrap_or(false) {
                    record = state
                        .library
                        .lock()
                        .map_err(|_| poisoned())?
                        .track_by_path(&record.path)?
                        .ok_or("Imported track disappeared")?;
                }
                track_ids.insert(entry.id, record.id);
                Ok::<_, String>(record)
            })();
            match result {
                Ok(t) => records.push(if summary.unwrap_or(false) {
                    serde_json::json!({"id":t.id})
                } else {
                    serde_json::to_value(t).map_err(|e| e.to_string())?
                }),
                Err(e) => {
                    state.message(e.clone());
                    if let Ok(mut p) = state.analysis.lock() {
                        p.errors.push(e);
                    }
                }
            }
            if let Ok(mut p) = state.analysis.lock() {
                p.completed = i + 1;
            }
        }
        fn playlists(
            lib: &Library,
            nodes: &[library::XmlNode],
            ids: &HashMap<String, i64>,
            parent: &str,
        ) -> Result<(), String> {
            for node in nodes {
                let name = if parent.is_empty() {
                    node.name.clone()
                } else {
                    format!("{parent} / {}", node.name)
                };
                if !node.tracks.is_empty() {
                    let p = lib.create_playlist(&name)?;
                    for t in &node.tracks {
                        if let Some(id) = ids.get(&t.key) {
                            lib.add_to_playlist(p.id, *id)?;
                        }
                    }
                }
                let next = if node.name == "ROOT" { parent } else { &name };
                playlists(lib, &node.children, ids, next)?;
            }
            Ok(())
        }
        let lib = state.library.lock().map_err(|_| poisoned())?;
        playlists(&lib, &xml.playlists.nodes, &track_ids, "")?;
        Ok(serde_json::Value::Array(records))
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn export_rekordbox_xml_to(
    app: tauri::AppHandle,
    path: Option<String>,
    overwrite: bool,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Write;
        let path = if let Some(path) = path {
            std::path::PathBuf::from(path)
        } else {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("rekordbox XML", &["xml"])
                .set_file_name("PulseDeck.xml")
                .blocking_save_file()
            else {
                return Ok(None);
            };
            file.into_path().map_err(|e| e.to_string())?
        };
        let state = app.state::<AppState>();
        let database = state
            .library
            .lock()
            .map_err(|_| poisoned())?
            .database_path()?;
        let xml = Library::export_readonly(std::path::Path::new(&database))?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(overwrite)
            .truncate(overwrite)
            .create_new(!overwrite)
            .open(&path)
            .map_err(|e| e.to_string())?;
        file.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--spotify-client-token-probe") {
        let result = std::env::args()
            .nth(2)
            .ok_or_else(|| "Provide a public developer Client ID".to_string())
            .and_then(|id| spotify_playback::probe_client_identity(&id));
        match result {
            Ok(report) => println!(
                "{}",
                serde_json::to_string(&report).expect("Probe report serialization failed")
            ),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--spotify-worker") {
        if spotify_playback::run_worker_from_stdio().is_err() {
            std::process::exit(1);
        }
        return;
    }
    // Own the runtime before opening the library or restoring the durable journal.
    let ready = Arc::new(std::sync::OnceLock::<tauri::AppHandle>::new());
    let runtime = ready.clone();
    let server =
        control_protocol::ipc::Server::start(control_protocol::socket_path(), move |request| {
            match runtime.get() {
                Some(app) => control::execute(app, request),
                None => control_protocol::Response::failure(
                    request.request_id,
                    control_protocol::ControlError::new(
                        "busy",
                        "PulseDeck is starting; retry shortly",
                    ),
                ),
            }
        });
    if server
        .as_ref()
        .is_err_and(|error| error.code == "instance_running")
    {
        let mut request = control_protocol::Request::new("app.show", serde_json::json!({}));
        request.timeout_ms = 2000;
        let _ = control_protocol::ipc::call(&control_protocol::socket_path(), &request);
        return;
    }
    let mut context = tauri::generate_context!();
    if std::env::var("PULSEDECK_TEST_NULL_AUDIO").as_deref() == Ok("1")
        && (!cfg!(debug_assertions)
            || std::env::var("PULSEDECK_ISOLATED").as_deref() != Ok("1")
            || std::env::var_os("PULSEDECK_DATA_DIR").is_none()
            || std::env::var_os("PULSEDECK_SOCKET").is_none())
    {
        eprintln!("The null-renderer harness requires a debug build, isolated profile, explicit data directory and private socket");
        std::process::exit(2);
    }
    if std::env::var_os("PULSEDECK_ISOLATED").is_some_and(|v| v == "1") {
        for window in &mut context.config_mut().app.windows {
            // A dedicated WKWebView store keeps test UI preferences separate.
            let mut identifier = [0u8; 16];
            identifier[..8].copy_from_slice(b"PDTEST01");
            identifier[8..12].copy_from_slice(&std::process::id().to_be_bytes());
            window.data_store_identifier = Some(identifier);
        }
    }
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(
            "pulsedeck-art",
            |context, request, responder| {
                let app = context.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    responder.respond(artwork::response(&app, request.uri().path()))
                });
            },
        )
        .setup(move |app| {
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let server = server.map_err(std::io::Error::other)?;
            let isolated = std::env::var_os("PULSEDECK_ISOLATED").is_some_and(|v| v == "1");
            if std::env::var_os("PULSEDECK_BEAT_MODEL_DIR").is_none() {
                std::env::set_var(
                    "PULSEDECK_BEAT_MODEL_DIR",
                    app.path().resource_dir()?.join("beat-models"),
                );
            }
            let data_dir = match std::env::var_os("PULSEDECK_DATA_DIR") {
                Some(path) => std::path::PathBuf::from(path),
                None if isolated => {
                    return Err(
                        std::io::Error::other("Isolated mode requires PULSEDECK_DATA_DIR").into(),
                    )
                }
                None => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&data_dir)?;
            let (spotify_library, spotify_native) = spotify::SpotifyService::new_pair(
                data_dir.join("spotify.json"),
                data_dir.join("spotify-native.json"),
            )
            .map_err(std::io::Error::other)?;
            app.manage(spotify_library);
            app.manage(spotify_native);
            let engine = Arc::new(EngineHandle::new().map_err(std::io::Error::other)?);
            let library =
                Library::open(&data_dir.join("library.sqlite3")).map_err(std::io::Error::other)?;
            let handle = app.handle().clone();
            let midi = MidiController::new(move |action| {
                if let Err(e) = controller::handle(&handle, action) {
                    handle.state::<AppState>().message(e);
                }
            });
            app.manage(AppState {
                engine: engine.clone(),
                library: Mutex::new(library),
                midi: Mutex::new(midi),
                analysis: Mutex::new(AnalysisProgress::default()),
                import_busy: AtomicBool::new(false),
                import_cancel: AtomicBool::new(false),
                data_dir: data_dir.clone(),
                loading: [AtomicU64::new(0), AtomicU64::new(0)],
                local_preparation: std::array::from_fn(|_| Mutex::new(())),
                source_jobs: spotify_decks::SourceJobs::default(),
                spotify_analysis: spotify_analysis::BatchService::default(),
                messages: Mutex::new(vec![]),
                controller: Mutex::new(controller::ControlState::default()),
                midi_autoconnect: AtomicBool::new(!isolated),
                audio_status: Mutex::new(None),
                audio_devices: Mutex::new(vec![]),
                audio_discovering: AtomicBool::new(!isolated),
                audio_operation: Mutex::new(()),
                audio_generation: AtomicU64::new(0),
                audio_preferences_path: data_dir.join("audio.json"),
                play_history: Mutex::new(suggest::PlayHistory::default()),
                features_backfill: AtomicBool::new(false),
            });
            control::start(app.handle(), server).map_err(std::io::Error::other)?;
            metadata_index::start(app.handle())?;
            let audio_app = app.handle().clone();
            std::thread::Builder::new()
                .name("pulsedeck-audio-startup".into())
                .spawn(move || {
                    if isolated {
                        return;
                    }
                    let audio_devices = deck_engine::devices().unwrap_or_default();
                    let state = audio_app.state::<AppState>();
                    if let Ok(mut cache) = state.audio_devices.lock() {
                        *cache = audio_devices.clone();
                    }
                    state.audio_discovering.store(false, Ordering::Release);
                    let saved = audio_preferences::load(&state.audio_preferences_path)
                        .unwrap_or_else(|error| {
                            state.message(error);
                            None
                        });
                    let (device, buffer, routing) =
                        audio_preferences::choose(&audio_devices, saved);
                    let _operation = state
                        .audio_operation
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    if state.audio_generation.load(Ordering::Acquire) != 0 {
                        return;
                    }
                    match engine.start_audio(device, Some(buffer), routing) {
                        Ok(status) => {
                            if state.audio_generation.load(Ordering::Acquire) == 0 {
                                *state.audio_status.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(status.clone());
                                if let Err(error) = audio_preferences::save(
                                    &state.audio_preferences_path,
                                    &audio_preferences::Preferences::from_status(&status),
                                ) {
                                    state.message(error);
                                }
                            }
                        }
                        Err(e) => audio_app
                            .state::<AppState>()
                            .message(format!("Choose an audio device in Settings: {e}")),
                    }
                })?;
            let background = app.handle().clone();
            std::thread::Builder::new()
                .name("pulsedeck-feedback-persistence".into())
                .spawn(move || {
                    let mut previous: [Option<PersistedDeckState>; 2] = [None, None];
                    let mut track_revision = u64::MAX;
                    let mut track_paths: [Option<String>; 2] = [None, None];
                    let mut loaded_revisions = [0u64; 2];
                    let mut source_revisions = [0u64; 2];
                    let mut discovery_tick = 0u32;
                    let mut last_tick = std::time::Instant::now();
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        let state = background.state::<AppState>();
                        discovery_tick = discovery_tick.wrapping_add(1);
                        if discovery_tick % 15 == 1
                            && state.midi_autoconnect.load(Ordering::Acquire)
                        {
                            if let Ok(ports) = flx4_midi::list_ports() {
                                let mut disconnected = false;
                                if let Ok(mut midi) = state.midi.try_lock() {
                                    let status = midi.status();
                                    if status.connected
                                        && status
                                            .input_name
                                            .as_ref()
                                            .is_some_and(|name| !ports.inputs.contains(name))
                                    {
                                        midi.disconnect();
                                        disconnected = true;
                                    }
                                    if !midi.status().connected {
                                        if let Some((input, output)) =
                                            flx4_midi::preferred_flx4_ports(&ports)
                                        {
                                            if midi.connect(input, output).is_ok()
                                                && midi.status().feedback_enabled
                                            {
                                                for deck in 0..2 {
                                                    let _ = midi.set_vinyl_mode(deck, true);
                                                }
                                                loaded_revisions = [0u64; 2];
                                            }
                                        }
                                    }
                                }
                                if disconnected {
                                    if let Ok(mut control) = state.controller.lock() {
                                        control.release_jogs(&state.engine);
                                    }
                                }
                            }
                        }
                        let snap = state.engine.transport_snapshot();
                        if state.engine.metadata_revision() != track_revision {
                            let metadata = state.engine.metadata_snapshot();
                            track_revision = metadata.revision;
                            source_revisions = metadata.load_revisions;
                            track_paths = metadata.tracks.map(|track| track.map(|t| t.path));
                        }
                        if let Ok(mut history) = state.play_history.lock() {
                            let seconds = last_tick.elapsed().as_secs_f64().min(0.5);
                            history.observe(&snap, &track_paths, seconds);
                        }
                        last_tick = std::time::Instant::now();
                        // Copy controller state before taking MIDI: worker actions take
                        // controller state too, so never invert the two mutexes.
                        let controls = state
                            .controller
                            .lock()
                            .map(|c| (c.modes, c.smart_fader, c.smart_cfx))
                            .unwrap_or(([flx4_midi::PadMode::HotCue; 2], false, false));
                        if let Ok(midi) = state.midi.try_lock() {
                            for i in 0..2 {
                                if loaded_revisions[i] != source_revisions[i] {
                                    if track_paths[i].is_some() {
                                        let _ = midi.loaded(i);
                                    }
                                    loaded_revisions[i] = source_revisions[i];
                                }
                            }
                            let mut feedback = controller::feedback(
                                &snap,
                                controls.0,
                                std::array::from_fn(|i| track_paths[i].is_some()),
                            );
                            for deck in &mut feedback.decks {
                                deck.smart_fader = controls.1;
                                deck.smart_cfx = controls.2;
                            }
                            let _ = midi.feedback(feedback);
                        }
                        if let Ok(lib) = state.library.try_lock() {
                            let mut skip = [false; 2];
                            for (index, deck) in snap.decks.iter().enumerate() {
                                if snap.load_revisions[index] != source_revisions[index] {
                                    continue;
                                }
                                if skip[index] {
                                    continue;
                                }
                                if let Some(path) = &track_paths[index] {
                                    let value = (
                                        path.clone(),
                                        deck.hot_cues,
                                        deck.memory_cues,
                                        deck.cue_seconds,
                                        deck.bpm.unwrap_or(0.),
                                        deck.beatgrid_offset_seconds,
                                    );
                                    let same_track = previous[index]
                                        .as_ref()
                                        .is_some_and(|old| old.0 == value.0);
                                    let cue_changed = previous[index].as_ref().is_some_and(|old| {
                                        old.1 != value.1 || old.2 != value.2 || old.3 != value.3
                                    });
                                    if same_track && cue_changed {
                                        let saved =
                                            lib.source_cues(path).and_then(|(_, mut cues)| {
                                                cues.hot_cues = value.1;
                                                cues.update_memory(value.2);
                                                cues.cue_seconds = value.3;
                                                let revision =
                                                    lib.save_source_cues(path, &cues, None)?;
                                                let (_, saved) = lib.source_cues(path)?;
                                                let metadata = state.engine.metadata_snapshot();
                                                for d in 0..2 {
                                                    if metadata.tracks[d]
                                                        .as_ref()
                                                        .is_some_and(|t| t.path == *path)
                                                        && metadata.load_revisions[d]
                                                            == snap.load_revisions[d]
                                                    {
                                                        let _ = state
                                                            .engine
                                                            .update_loaded_analysis_for_revision(
                                                                d,
                                                                &metadata.tracks[d]
                                                                    .as_ref()
                                                                    .unwrap()
                                                                    .id,
                                                                Some(metadata.load_revisions[d]),
                                                                |a| {
                                                                    a.cue_points =
                                                                        saved.points.clone();
                                                                },
                                                            );
                                                    }
                                                }
                                                Ok(revision)
                                            });
                                        if let Err(error) = saved {
                                            state.message(format!(
                                                "Cue persistence failed: {error}"
                                            ));
                                        } else {
                                            let other = 1 - index;
                                            if track_paths[other].as_ref() == Some(path) {
                                                let update = state
                                                    .engine
                                                    .command_for_revision(
                                                        EngineCommand::RestoreHotCues {
                                                            deck: other,
                                                            cues: value.1,
                                                        },
                                                        Some(source_revisions[other]),
                                                    )
                                                    .and_then(|()| {
                                                        state.engine.command_for_revision(
                                                            EngineCommand::RestoreMemoryCues {
                                                                deck: other,
                                                                cues: value.2,
                                                            },
                                                            Some(source_revisions[other]),
                                                        )
                                                    })
                                                    .and_then(|()| {
                                                        state.engine.command_for_revision(
                                                            EngineCommand::SetCuePoint {
                                                                deck: other,
                                                                seconds: value.3,
                                                            },
                                                            Some(source_revisions[other]),
                                                        )
                                                    });
                                                if let Err(e) = update {
                                                    state.message(e);
                                                } else {
                                                    previous[other] = Some(value.clone());
                                                    skip[other] = true;
                                                }
                                            }
                                        }
                                    }
                                    previous[index] = Some(value);
                                }
                            }
                        };
                    }
                })?;
            suggest::warm_cached_features(app.handle());
            let _ = ready.set(app.handle().clone());
            Ok(())
            })();
            if let Err(error) = result {
                let message = format!("PulseDeck could not start: {error}\n\nYour library has not been reset. Close other copies and check the app data folder's permissions before trying again.");
                eprintln!("{message}");
                for window in app.webview_windows().values() {
                    let _ = window.hide();
                }
                let handle = app.handle().clone();
                app.dialog().message(message).title("PulseDeck startup failed")
                    .kind(tauri_plugin_dialog::MessageDialogKind::Error)
                    .show(move |_| handle.exit(1));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            control::control_request,
            control::ui_reply,
            control::ui_publish
        ])
        .build(context);
    match result {
        Ok(app) => app.run(|handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                if let Err(error) = control::show_main_window(handle) {
                    eprintln!("PulseDeck reopen: {error}");
                }
            }
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                if let Err(error) = control::prepare_exit(handle) {
                    eprintln!("PulseDeck shutdown: {error}");
                }
            }
            _ => {}
        }),
        Err(error) => {
            eprintln!("PulseDeck could not start: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod metadata_edit_tests {
    use super::*;
    #[test]
    fn name_edits_update_matching_decks_without_changing_analysis_or_cues() {
        let engine = EngineHandle::new().unwrap();
        let mut original = engine.load_demo(0).unwrap();
        let other = engine.load_demo(1).unwrap();
        apply_library_names(&engine, &original.path, "Edited title", "Edited artist");
        original.title = "Edited title".into();
        original.artist = Some("Edited artist".into());
        let snapshot = engine.metadata_snapshot();
        assert_eq!(
            serde_json::to_value(&snapshot.tracks[0]).unwrap(),
            serde_json::to_value(Some(original)).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&snapshot.tracks[1]).unwrap(),
            serde_json::to_value(Some(other)).unwrap()
        );
    }
}

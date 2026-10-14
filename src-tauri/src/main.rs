#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio_preferences;
mod control;
mod controller;
mod library;
mod spotify;
mod spotify_analysis;
mod spotify_decks;
mod spotify_vault;
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
}

struct AppState {
    engine: Arc<EngineHandle>,
    library: Mutex<Library>,
    midi: Mutex<MidiController>,
    analysis: Mutex<AnalysisProgress>,
    import_busy: AtomicBool,
    loading: [AtomicU64; 2],
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
}

impl AppState {
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

#[tauri::command]
fn get_app_state(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let (library, playlists) = {
        let lib = state.library.lock().map_err(|_| poisoned())?;
        (lib.tracks()?, lib.playlists()?)
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
        audio_status: state.audio_status.lock().map_err(|_| poisoned())?.clone(),
        midi: state.midi.lock().map_err(|_| poisoned())?.status(),
        midi_ports: flx4_midi::list_ports().unwrap_or_default(),
        analysis: state.analysis.lock().map_err(|_| poisoned())?.clone(),
        messages: state.messages.lock().map_err(|_| poisoned())?.clone(),
        smart_modes: modes,
        spotify_decks: state.source_jobs.snapshot(),
    })
}

#[tauri::command]
fn get_engine_snapshot(state: State<'_, AppState>) -> EngineSnapshot {
    state.engine.snapshot()
}

#[derive(Serialize)]
struct EngineTransport {
    engine: EngineSnapshot,
    metadata_revision: u64,
}
#[tauri::command]
fn get_engine_transport(state: State<'_, AppState>) -> EngineTransport {
    EngineTransport {
        engine: state.engine.transport_snapshot(),
        metadata_revision: state.engine.metadata_revision(),
    }
}
#[tauri::command]
fn get_engine_metadata(state: State<'_, AppState>) -> deck_engine::EngineMetadataSnapshot {
    state.engine.metadata_snapshot()
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
#[tauri::command]
fn get_activity_state(state: State<'_, AppState>) -> Result<ActivitySnapshot, String> {
    let smart_modes = {
        let control = state.controller.lock().map_err(|_| poisoned())?;
        SmartModes {
            fader: control.smart_fader,
            cfx: control.smart_cfx,
        }
    };
    Ok(ActivitySnapshot {
        audio_status: state.audio_status.lock().map_err(|_| poisoned())?.clone(),
        audio_devices: state.audio_devices.lock().map_err(|_| poisoned())?.clone(),
        midi: state.midi.lock().map_err(|_| poisoned())?.status(),
        analysis: state.analysis.lock().map_err(|_| poisoned())?.clone(),
        messages: state.messages.lock().map_err(|_| poisoned())?.clone(),
        smart_modes,
        spotify_decks: state.source_jobs.snapshot(),
    })
}

#[tauri::command]
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

#[tauri::command(rename_all = "snake_case")]
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
#[tauri::command]
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

#[tauri::command]
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

fn analysis_record(analysis: TrackAnalysis) -> TrackRecord {
    TrackRecord {
        id: 0,
        path: analysis.path,
        title: analysis.title,
        artist: analysis.artist.unwrap_or_default(),
        tags: Vec::new(),
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
            neural_rhythm: analysis.neural_rhythm,
            rhythm_error: analysis.rhythm_error,
            manual_grid_anchor_index: analysis.manual_grid_anchor_index,
        },
    }
}

fn import_one(state: &AppState, path: &str) -> Result<TrackRecord, String> {
    let path = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    if !path.is_file() {
        return Err("Select a music file".into());
    }
    let path_string = path.to_string_lossy().into_owned();
    if let Some(existing) = state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .track_by_path(&path_string)?
    {
        if existing.analysis.analysis_version >= audio_analysis::ANALYSIS_VERSION
            && existing.analysis.waveform_bands.is_some()
        {
            return Ok(existing);
        }
    }
    let record = analysis_record(deck_engine::analyze_file(&path)?);
    state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .upsert(&record)
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

fn import_batch(app: &tauri::AppHandle, paths: Vec<String>) -> Result<Vec<TrackRecord>, String> {
    let state = app.state::<AppState>();
    if state.import_busy.swap(true, Ordering::AcqRel) {
        return Err("A library import is already running".into());
    }
    let _guard = ImportGuard(&state);
    *state.analysis.lock().map_err(|_| poisoned())? = AnalysisProgress {
        active: true,
        total: paths.len(),
        ..Default::default()
    };
    let mut results = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        if let Ok(mut p) = state.analysis.lock() {
            p.current_path = Some(path.clone());
        }
        match import_one(&state, &path) {
            Ok(record) => results.push(record),
            Err(e) => {
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

#[tauri::command]
async fn import_tracks(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<TrackRecord>, String> {
    if paths.len() > 10_000 {
        return Err("Import at most 10,000 tracks in one batch".into());
    }
    tauri::async_runtime::spawn_blocking(move || import_batch(&app, paths))
        .await
        .map_err(|e| e.to_string())?
}

fn load_into_deck(
    app: &tauri::AppHandle,
    deck: usize,
    path: &Path,
) -> Result<TrackAnalysis, String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    let state = app.state::<AppState>();
    state.source_jobs.cancel(deck);
    let request = state.loading[deck].fetch_add(1, Ordering::AcqRel) + 1;
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    // The engine's own generation checks protect late decode results. Parent
    // generation also avoids applying persisted metadata for a superseded load.
    let analysis = state.engine.load_track(deck, &path)?;
    let record = state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .upsert(&analysis_record(analysis.clone()))?;
    if state.loading[deck].load(Ordering::Acquire) != request {
        return Err("Track load was replaced by a newer request".into());
    }
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
    Ok(analysis)
}

#[tauri::command]
async fn load_track(
    app: tauri::AppHandle,
    deck: usize,
    path: String,
) -> Result<TrackAnalysis, String> {
    tauri::async_runtime::spawn_blocking(move || load_into_deck(&app, deck, Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
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
        if let Some(bpm) = chosen_bpm {
            state.engine.command(EngineCommand::SetBpm { deck, bpm })?;
        }
        Ok(analysis)
    })
    .await
    .map_err(|_| "Track analysis worker failed".to_string())?
}

#[tauri::command]
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

#[tauri::command]
fn engine_command(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    command: EngineCommand,
) -> Result<(), String> {
    engine_command_inner(app, state, command, true, None)
}
fn engine_command_inner(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    command: EngineCommand,
    smart_modes: bool,
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

#[tauri::command(rename_all = "snake_case")]
fn set_controller_selection(state: State<'_, AppState>, track_id: i64) -> Result<(), String> {
    let tracks = state.library.lock().map_err(|_| poisoned())?.tracks()?;
    let mut control = state.controller.lock().map_err(|_| poisoned())?;
    let index = if control.browse_context.is_empty() {
        tracks.iter().position(|t| t.id == track_id)
    } else {
        control.browse_context.iter().position(|id| *id == track_id)
    };
    control.selected = index.ok_or("Selected track is not in the current browser")?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
fn set_controller_browse_context(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
) -> Result<(), String> {
    if track_ids.len() > 100_000 {
        return Err("Browser context exceeds 100,000 tracks".into());
    }
    let mut control = state.controller.lock().map_err(|_| poisoned())?;
    control.browse_context = track_ids;
    control.selected = control
        .selected
        .min(control.browse_context.len().saturating_sub(1));
    Ok(())
}

#[tauri::command]
fn get_controller_surface(state: State<'_, AppState>) -> controller::ControllerSurface {
    state
        .controller
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .surface()
}
#[tauri::command(rename_all = "snake_case")]
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
#[tauri::command(rename_all = "snake_case")]
fn set_controller_browser_active(state: State<'_, AppState>, active: bool) -> Result<(), String> {
    state
        .controller
        .lock()
        .map_err(|_| poisoned())?
        .browser_ui_active = active;
    Ok(())
}
#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
fn list_midi_ports() -> Result<MidiPorts, String> {
    flx4_midi::list_ports()
}

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command]
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

#[tauri::command]
fn get_midi_history(state: State<'_, AppState>) -> Result<Vec<flx4_midi::RawMidiEvent>, String> {
    Ok(state.midi.lock().map_err(|_| poisoned())?.history())
}

#[tauri::command]
fn create_playlist(state: State<'_, AppState>, name: String) -> Result<Playlist, String> {
    state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .create_playlist(&name)
}

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command]
fn delete_track(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .delete_track(id)
}

#[tauri::command]
fn get_recording_status(state: State<'_, AppState>) -> RecordingStatus {
    state.engine.recording_status()
}

#[tauri::command]
async fn start_recording(app: tauri::AppHandle) -> Result<Option<RecordingStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("WAV recording", &["wav"])
            .set_file_name("PulseDeck-mix.wav")
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        app.state::<AppState>()
            .engine
            .start_recording(&path)
            .map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_recording(app: tauri::AppHandle) -> Result<RecordingStatus, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().engine.stop_recording())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn choose_sample(
    app: tauri::AppHandle,
    slot: usize,
) -> Result<Option<TrackAnalysis>, String> {
    if slot >= 16 {
        return Err("Sample slot must be 0 through 15".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
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
        let path = file.into_path().map_err(|e| e.to_string())?;
        app.state::<AppState>()
            .engine
            .load_sample(slot, &path)
            .map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_rekordbox_xml(
    app: tauri::AppHandle,
    path: Option<String>,
) -> Result<Vec<TrackRecord>, String> {
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
                return Ok(vec![]);
            };
            file.into_path().map_err(|e| e.to_string())?
        };
        let xml = library::parse_rekordbox_xml(&path)?;
        let state = app.state::<AppState>();
        if state.import_busy.swap(true, Ordering::AcqRel) {
            return Err("An import is already running".into());
        }
        let _guard = ImportGuard(&state);
        *state.analysis.lock().map_err(|_| poisoned())? = AnalysisProgress {
            active: true,
            total: xml.collection.tracks.len(),
            ..Default::default()
        };
        let mut records = Vec::new();
        let mut track_ids = HashMap::new();
        for (i, entry) in xml.collection.tracks.into_iter().enumerate() {
            if let Ok(mut p) = state.analysis.lock() {
                p.current_path = Some(entry.location.clone());
            }
            let result = (|| {
                let path = library::xml_path(&entry.location)?;
                let mut record = import_one(&state, &path.to_string_lossy())?;
                if let Some(bpm) = entry
                    .bpm
                    .or_else(|| entry.tempos.first().map(|t| t.bpm))
                    .filter(|b| b.is_finite() && (30.0..=300.0).contains(b))
                {
                    record.bpm = bpm;
                }
                let mut cues = vec![None; 8];
                for mark in &entry.marks {
                    if mark.kind == 0
                        && (0..8).contains(&mark.number)
                        && mark.start.is_finite()
                        && mark.start >= 0.
                        && mark.start <= record.duration_seconds
                    {
                        cues[mark.number as usize] = Some(mark.start);
                    }
                }
                record.hot_cues = cues;
                let mut memory = entry
                    .marks
                    .iter()
                    .filter(|m| {
                        m.kind == 0
                            && m.number == -1
                            && m.start.is_finite()
                            && m.start >= 0.
                            && m.start <= record.duration_seconds
                    })
                    .map(|m| m.start)
                    .take(10)
                    .map(Some)
                    .collect::<Vec<_>>();
                memory.resize(10, None);
                record.memory_cues = memory;
                record.cue_seconds = record
                    .memory_cues
                    .iter()
                    .flatten()
                    .next()
                    .copied()
                    .unwrap_or(0.);
                record.beatgrid_offset_seconds = entry
                    .tempos
                    .first()
                    .filter(|t| t.start.is_finite())
                    .map(|t| t.start.max(0.))
                    .unwrap_or(0.);
                {
                    let lib = state.library.lock().map_err(|_| poisoned())?;
                    lib.metadata(
                        &record.path,
                        Some(record.bpm),
                        Some(record.hot_cues.clone()),
                        Some(record.beatgrid_offset_seconds),
                    )?;
                    lib.memory(&record.path, &record.memory_cues, record.cue_seconds)?;
                    lib.tags(
                        &record.path,
                        &entry.name,
                        &entry.artist,
                        entry.key.as_deref(),
                    )?;
                }
                if !entry.name.is_empty() {
                    record.title = entry.name;
                }
                if !entry.artist.is_empty() {
                    record.artist = entry.artist;
                }
                if entry.key.is_some() {
                    record.key = entry.key;
                }
                track_ids.insert(entry.id, record.id);
                Ok::<_, String>(record)
            })();
            match result {
                Ok(t) => records.push(t),
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
        Ok(records)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_rekordbox_xml(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("rekordbox XML", &["xml"])
            .set_file_name("PulseDeck.xml")
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        let state = app.state::<AppState>();
        let lib = state.library.lock().map_err(|_| poisoned())?;
        let xml = library::export_xml(&lib.tracks()?, &lib.playlists()?)?;
        std::fs::write(&path, xml).map_err(|e| e.to_string())?;
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
    let mut context = tauri::generate_context!();
    if std::env::var_os("PULSEDECK_ISOLATED").is_some_and(|v| v == "1") {
        for window in &mut context.config_mut().app.windows {
            // A dedicated WKWebView store keeps test UI preferences separate.
            let mut identifier = [0u8; 16];
            identifier[..8].copy_from_slice(b"PDTEST01");
            identifier[8..12].copy_from_slice(&std::process::id().to_be_bytes());
            window.data_store_identifier = Some(identifier);
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let isolated = std::env::var_os("PULSEDECK_ISOLATED").is_some_and(|v| v == "1");
            if std::env::var_os("PULSEDECK_BEAT_MODEL_DIR").is_none() {
                std::env::set_var(
                    "PULSEDECK_BEAT_MODEL_DIR",
                    app.path().resource_dir()?.join("beat-models"),
                );
            }
            std::thread::spawn(|| {
                let _ = audio_analysis::initialize_neural_rhythm();
            });
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
                loading: [AtomicU64::new(0), AtomicU64::new(0)],
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
            });
            control::start(app.handle()).map_err(std::io::Error::other)?;
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
                                                lib.save_source_cues(path, &cues, None)
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
                                                    .command(EngineCommand::RestoreHotCues {
                                                        deck: other,
                                                        cues: value.1,
                                                    })
                                                    .and_then(|()| {
                                                        state.engine.command(
                                                            EngineCommand::RestoreMemoryCues {
                                                                deck: other,
                                                                cues: value.2,
                                                            },
                                                        )
                                                    })
                                                    .and_then(|()| {
                                                        state.engine.command(
                                                            EngineCommand::SetCuePoint {
                                                                deck: other,
                                                                seconds: value.3,
                                                            },
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            control::control_request,
            control::ui_reply,
            control::ui_publish,
            get_app_state,
            get_engine_snapshot,
            get_engine_transport,
            get_engine_metadata,
            get_activity_state,
            list_audio_devices,
            start_audio,
            stop_audio,
            choose_tracks,
            import_tracks,
            load_track,
            load_demo,
            reanalyze_loaded_track,
            engine_command,
            list_midi_ports,
            connect_midi,
            disconnect_midi,
            get_midi_history,
            create_playlist,
            add_to_playlist,
            delete_track,
            import_rekordbox_xml,
            export_rekordbox_xml,
            get_recording_status,
            start_recording,
            stop_recording,
            choose_sample,
            set_controller_selection,
            set_controller_browse_context,
            get_controller_surface,
            set_controller_tempo_range,
            set_controller_browser_active,
            instant_double,
            set_smart_modes,
            spotify::spotify_status,
            spotify::spotify_configure,
            spotify::spotify_begin_login,
            spotify::spotify_disconnect,
            spotify::spotify_playlists,
            spotify::spotify_liked_tracks,
            spotify::spotify_playlist_tracks,
            spotify::spotify_album_tracks,
            spotify::spotify_devices,
            spotify::spotify_play,
            spotify::spotify_pause,
            spotify_analysis::spotify_cached_analysis,
            spotify_analysis::analyze_spotify_page,
            spotify_analysis::spotify_analysis_status,
            spotify_analysis::cancel_spotify_analysis,
            spotify_decks::load_spotify_track,
            spotify_decks::cancel_spotify_load,
            spotify::spotify_native_status,
            spotify::spotify_native_begin_login,
            spotify::spotify_native_disconnect
        ])
        .run(context)
        .expect("PulseDeck could not start");
}

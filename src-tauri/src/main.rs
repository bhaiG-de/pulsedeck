#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analysis_preferences;
mod audio_preferences;
mod control;
mod controller;
mod library;
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
}

struct AppState {
    engine: Arc<EngineHandle>,
    library: Mutex<Library>,
    midi: Mutex<MidiController>,
    analysis: Mutex<AnalysisProgress>,
    import_busy: AtomicBool,
    /// Set to stop an import or library analysis between (and inside) tracks.
    import_cancel: AtomicBool,
    /// Whether the beat model also runs on Spotify sources.
    beat_model_spotify: AtomicBool,
    /// Whether song sections fill empty hot cue pads and memory cues.
    auto_hot_cues: AtomicBool,
    auto_memory_cues: AtomicBool,
    /// Debounces section refreshes after manual grid edits, per deck.
    section_refresh: [AtomicU64; 2],
    analysis_settings_path: std::path::PathBuf,
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
    /// Tracks heard this session, left out of next-track suggestions.
    play_history: Mutex<suggest::PlayHistory>,
    /// Set while older records get their suggestion features.
    features_backfill: AtomicBool,
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
/// A deck's hot cue pads and memory cues.
type DeckCues = ([Option<f64>; 8], [Option<f64>; 10]);
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
    analysis_settings: analysis_preferences::AnalysisSettings,
}

fn poisoned() -> String {
    "Internal state is unavailable".into()
}

fn analysis_settings(state: &AppState) -> analysis_preferences::AnalysisSettings {
    analysis_preferences::AnalysisSettings {
        beat_model_spotify: state.beat_model_spotify.load(Ordering::Acquire),
        auto_hot_cues: state.auto_hot_cues.load(Ordering::Acquire),
        auto_memory_cues: state.auto_memory_cues.load(Ordering::Acquire),
    }
}

/// Whether an analysis already has the current beat and section stage.
pub(crate) fn beat_stage_current(analysis: &TrackAnalysis) -> bool {
    analysis
        .beat_model
        .as_ref()
        .is_some_and(|stage| stage.version >= audio_analysis::BEAT_MODEL_VERSION)
        && analysis.neural_rhythm.is_some()
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
        analysis_settings: analysis_settings(&state),
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
            beat_model: analysis.beat_model,
            sections: analysis.sections,
            auto_cues: analysis.auto_cues,
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
        sections: record.analysis.sections,
        auto_cues: record.analysis.auto_cues,
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
/// Returns the grid the track should use and whether it places beat 1.
fn persist_analysis(
    app: &tauri::AppHandle,
    state: &AppState,
    analysis: &TrackAnalysis,
) -> Result<(Option<f64>, Option<f64>, bool), String> {
    let knows_beat_one =
        |source: Option<&str>| matches!(source, Some("beat_this" | "model" | "manual"));
    if analysis.path.starts_with("spotify:") {
        let stored = state
            .library
            .lock()
            .map_err(|_| poisoned())?
            .spotify_analysis(&analysis.path)?;
        let merged = keep_spotify_manual(analysis.clone(), stored);
        spotify_analysis::store(app, &merged)?;
        Ok((
            merged.bpm,
            merged.beatgrid_offset_seconds,
            knows_beat_one(merged.beatgrid_source.as_deref()),
        ))
    } else {
        let record = state
            .library
            .lock()
            .map_err(|_| poisoned())?
            .upsert(&analysis_record(analysis.clone()))?;
        Ok((
            Some(record.bpm),
            Some(record.beatgrid_offset_seconds),
            knows_beat_one(record.analysis.beatgrid_source.as_deref()),
        ))
    }
}

/// Re-cut the loaded deck's sections when they were cut from a different grid
/// than the one the track uses.
fn sections_for_grid(
    state: &AppState,
    deck: usize,
    analysis: TrackAnalysis,
    grid: (Option<f64>, Option<f64>, bool),
) -> Result<TrackAnalysis, String> {
    match grid {
        (Some(bpm), Some(offset), true)
            if (30.0..=300.0).contains(&bpm)
                && analysis
                    .sections
                    .as_ref()
                    .is_none_or(|s| !s.matches_grid(bpm, offset)) =>
        {
            state.engine.sections_loaded(deck, bpm, offset)
        }
        _ => Ok(analysis),
    }
}

/// Fill the loaded deck's empty cue slots from its sections and save which cues
/// were placed automatically. `cues` overrides the deck's current cues when the
/// engine has not applied a just-restored set yet. Spotify cues are not stored
/// between loads, so an older auto-cue record from the cache is ignored.
pub(crate) fn place_auto_cues(
    app: &tauri::AppHandle,
    state: &AppState,
    deck: usize,
    analysis: TrackAnalysis,
    cues: Option<DeckCues>,
    fresh_load: bool,
) -> Result<TrackAnalysis, String> {
    let Some(stage) = analysis.sections.as_ref() else {
        return Ok(analysis);
    };
    let (hot, memory) = cues.unwrap_or_else(|| {
        let deck = &state.engine.transport_snapshot().decks[deck];
        (deck.hot_cues, deck.memory_cues)
    });
    let previous = if fresh_load {
        None
    } else {
        analysis.auto_cues.as_ref()
    };
    let plan = audio_analysis::plan_auto_cues(
        stage,
        hot,
        memory,
        previous,
        state.auto_hot_cues.load(Ordering::Acquire),
        state.auto_memory_cues.load(Ordering::Acquire),
    );
    let metadata = state.engine.metadata_snapshot();
    if metadata.tracks[deck]
        .as_ref()
        .is_none_or(|t| t.id != analysis.id)
    {
        return Err("Track changed during cue analysis".into());
    }
    if plan.hot != hot || plan.memory != memory {
        state.engine.merge_auto_cues(
            deck,
            metadata.load_revisions[deck],
            (hot, plan.hot),
            (memory, plan.memory),
        )?;
    }
    let record = plan.record;
    let updated = state
        .engine
        .update_loaded_analysis(deck, &analysis.id, |a| a.auto_cues = Some(record))?;
    persist_analysis(app, state, &updated)?;
    Ok(updated)
}

/// Run the beat model and sections on a freshly loaded deck in the background
/// and persist the result. The deck's grid moves only when that cannot disturb
/// a synced mix; otherwise the new grid applies on the track's next load. Empty
/// cue slots are filled from the sections either way.
pub(crate) fn spawn_beat_model(app: &tauri::AppHandle, deck: usize, request: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let current = || state.loading[deck].load(Ordering::Acquire) == request;
        if !current() {
            return;
        }
        let analysis = match state.engine.beat_model_loaded(deck) {
            Ok(analysis) if current() => analysis,
            Ok(_) => return,
            Err(e) => {
                if current() {
                    state.message(format!("Beat analysis failed: {e}"));
                }
                return;
            }
        };
        let result = (|| -> Result<(), String> {
            persist_analysis(&app, &state, &analysis)?;
            if !current() {
                return Ok(());
            }
            let transport = state.engine.transport_snapshot();
            let d = &transport.decks[deck];
            let grid = (d.bpm, Some(d.beatgrid_offset_seconds), d.beatgrid_downbeat);
            let analysis = sections_for_grid(&state, deck, analysis.clone(), grid)?;
            let spotify = analysis.path.starts_with("spotify:");
            place_auto_cues(&app, &state, deck, analysis, None, spotify).map(|_| ())
        })();
        if let Err(e) = result {
            if current() {
                state.message(format!("Beat analysis could not be saved: {e}"));
            }
        }
    });
}

/// After a manual grid edit, re-cut the loaded deck's sections on the new grid
/// and move its untouched auto cues. Rapid edits collapse into one refresh.
fn spawn_sections_refresh(app: &tauri::AppHandle, deck: usize) {
    let app = app.clone();
    let state = app.state::<AppState>();
    let ticket = state.section_refresh[deck].fetch_add(1, Ordering::AcqRel) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let state = app.state::<AppState>();
        if state.section_refresh[deck].load(Ordering::Acquire) != ticket {
            return;
        }
        let deck_state = state.engine.transport_snapshot().decks[deck].clone();
        let Some(bpm) = deck_state.bpm.filter(|b| (30.0..=300.0).contains(b)) else {
            return;
        };
        let result = state
            .engine
            .sections_loaded(deck, bpm, deck_state.beatgrid_offset_seconds)
            .and_then(|analysis| {
                let spotify = analysis.path.starts_with("spotify:");
                place_auto_cues(&app, &state, deck, analysis, None, spotify)
            });
        // A track loaded meanwhile makes the refresh moot, not an error.
        if let Err(Some(e)) = result.map_err(|e| (!e.starts_with("Track changed")).then_some(e)) {
            state.message(format!("Sections could not be updated: {e}"));
        }
    });
}

/// Choose the vocabulary for the loaded track's section labels (`None` returns
/// to the detected one) and re-place untouched auto cues by the new labels.
#[tauri::command(rename_all = "snake_case")]
async fn set_section_vocabulary(
    app: tauri::AppHandle,
    deck: usize,
    vocabulary: Option<audio_analysis::Vocabulary>,
) -> Result<TrackAnalysis, String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let id = state.engine.metadata_snapshot().tracks[deck]
            .as_ref()
            .map(|t| t.id.clone())
            .ok_or("Deck has no loaded track")?;
        let analysis = state.engine.update_loaded_analysis(deck, &id, |a| {
            if let Some(stage) = a.sections.as_mut() {
                stage.vocabulary_override = vocabulary;
                stage.relabel();
            }
        })?;
        if analysis.sections.is_none() {
            return Err("This track has no sections yet".into());
        }
        let spotify = analysis.path.starts_with("spotify:");
        place_auto_cues(&app, &state, deck, analysis, None, spotify)
    })
    .await
    .map_err(|_| "Section worker failed".to_string())?
}

fn import_one(state: &AppState, path: &str) -> Result<TrackRecord, String> {
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
    let (mut analysis, warning) = deck_engine::analyze_file_cached_with_sections(
        &path,
        existing.map(record_analysis),
        &cancel,
    )?;
    if let Some(warning) = warning {
        if let Ok(mut p) = state.analysis.lock() {
            p.errors.push(format!("{path_string}: {warning}"));
        }
    }
    let lib = state.library.lock().map_err(|_| poisoned())?;
    let existing = lib.track_by_path(&path_string)?;
    // A loaded deck owns its cues; they are filled when it next loads.
    let loaded = state
        .engine
        .metadata_snapshot()
        .tracks
        .iter()
        .flatten()
        .any(|t| t.path == path_string);
    let plan = analysis.sections.as_ref().filter(|_| !loaded).map(|stage| {
        let (hot, memory) = existing.as_ref().map_or(([None; 8], [None; 10]), |r| {
            (
                r.hot_cues.clone().try_into().unwrap_or([None; 8]),
                r.memory_cues.clone().try_into().unwrap_or([None; 10]),
            )
        });
        let previous = existing
            .as_ref()
            .and_then(|r| r.analysis.auto_cues.as_ref());
        let plan = audio_analysis::plan_auto_cues(
            stage,
            hot,
            memory,
            previous,
            state.auto_hot_cues.load(Ordering::Acquire),
            state.auto_memory_cues.load(Ordering::Acquire),
        );
        (plan, hot, memory)
    });
    if let Some((plan, _, _)) = &plan {
        analysis.auto_cues = Some(plan.record.clone());
    }
    let record = lib.upsert(&analysis_record(analysis))?;
    if let Some((plan, hot, memory)) = plan {
        if plan.hot != hot {
            lib.metadata(&record.path, None, Some(plan.hot.to_vec()), None)?;
        }
        if plan.memory != memory {
            lib.memory(&record.path, &plan.memory, record.cue_seconds)?;
        }
        return lib
            .track_by_path(&record.path)?
            .ok_or_else(|| "Imported track was not found".to_string());
    }
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

fn import_batch(app: &tauri::AppHandle, paths: Vec<String>) -> Result<Vec<TrackRecord>, String> {
    let state = app.state::<AppState>();
    if state.import_busy.swap(true, Ordering::AcqRel) {
        return Err("A library import is already running".into());
    }
    let _guard = ImportGuard(&state);
    state.import_cancel.store(false, Ordering::Release);
    *state.analysis.lock().map_err(|_| poisoned())? = AnalysisProgress {
        active: true,
        total: paths.len(),
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
        match import_one(&state, &path) {
            Ok(record) => results.push(record),
            Err(_) if state.import_cancel.load(Ordering::Acquire) => continue,
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

/// Re-analyze local tracks that lack the current beat-model stage, through the
/// same cancellable worker and progress as an import.
#[tauri::command]
async fn update_library_analysis(app: tauri::AppHandle) -> Result<Vec<TrackRecord>, String> {
    let paths = app
        .state::<AppState>()
        .library
        .lock()
        .map_err(|_| poisoned())?
        .paths_needing_beat_model()?;
    tauri::async_runtime::spawn_blocking(move || import_batch(&app, paths))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_library_analysis(state: State<'_, AppState>) {
    state.import_cancel.store(true, Ordering::Release);
}

#[tauri::command]
fn set_analysis_settings(
    state: State<'_, AppState>,
    settings: analysis_preferences::AnalysisSettings,
) -> Result<analysis_preferences::AnalysisSettings, String> {
    analysis_preferences::save(&state.analysis_settings_path, &settings)?;
    state
        .beat_model_spotify
        .store(settings.beat_model_spotify, Ordering::Release);
    state
        .auto_hot_cues
        .store(settings.auto_hot_cues, Ordering::Release);
    state
        .auto_memory_cues
        .store(settings.auto_memory_cues, Ordering::Release);
    Ok(settings)
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
    let cached = state
        .library
        .lock()
        .map_err(|_| poisoned())?
        .track_by_path(&path.to_string_lossy())?
        .map(record_analysis);
    // The engine's own generation checks protect late decode results. Parent
    // generation also avoids applying persisted metadata for a superseded load.
    let analysis = state.engine.load_track_cached(deck, &path, cached)?;
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
    if !beat_stage_current(&analysis) {
        spawn_beat_model(app, deck, request);
        return Ok(analysis);
    }
    // Cheap: fill any cue slots left empty since the sections were found. The
    // just-restored cues are passed in because the engine may not show them yet.
    match place_auto_cues(
        app,
        &state,
        deck,
        analysis.clone(),
        Some((cues, memory_cues)),
        false,
    ) {
        Ok(updated) => Ok(updated),
        Err(e) => {
            state.message(format!("Auto cues could not be placed: {e}"));
            Ok(analysis)
        }
    }
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
        // Only explicit overrides issue tempo/grid commands. Model event timing
        // reaches the render clock through the engine's revision-guarded UpdateGrid.
        if let Some(bpm) = chosen_bpm {
            apply_grid(&state, deck, Some(bpm), None)?;
        }
        let transport = state.engine.transport_snapshot();
        let d = &transport.decks[deck];
        let analysis = sections_for_grid(
            &state,
            deck,
            analysis,
            (d.bpm, Some(d.beatgrid_offset_seconds), d.beatgrid_downbeat),
        )?;
        let spotify = analysis.path.starts_with("spotify:");
        place_auto_cues(&app, &state, deck, analysis, None, spotify)
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
    // A manual grid edit on a loaded track re-cuts its sections afterwards.
    let edited_deck = match &command {
        EngineCommand::SetBpm { deck, .. } | EngineCommand::SetBeatgrid { deck, .. }
            if manual_bpm.is_some() || manual_grid.is_some() =>
        {
            Some(*deck)
        }
        _ => None,
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
    if let Some(deck) = edited_deck {
        spawn_sections_refresh(&app, deck);
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
            let settings = analysis_preferences::load(&data_dir.join("analysis.json"));
            app.manage(AppState {
                engine: engine.clone(),
                library: Mutex::new(library),
                midi: Mutex::new(midi),
                analysis: Mutex::new(AnalysisProgress::default()),
                import_busy: AtomicBool::new(false),
                import_cancel: AtomicBool::new(false),
                beat_model_spotify: AtomicBool::new(settings.beat_model_spotify),
                auto_hot_cues: AtomicBool::new(settings.auto_hot_cues),
                auto_memory_cues: AtomicBool::new(settings.auto_memory_cues),
                section_refresh: [AtomicU64::new(0), AtomicU64::new(0)],
                analysis_settings_path: data_dir.join("analysis.json"),
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
                play_history: Mutex::new(suggest::PlayHistory::default()),
                features_backfill: AtomicBool::new(false),
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
            update_library_analysis,
            cancel_library_analysis,
            set_analysis_settings,
            set_section_vocabulary,
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
            spotify::spotify_native_disconnect,
            suggest::suggest_next,
            suggest::clear_play_history
        ])
        .run(context)
        .expect("PulseDeck could not start");
}

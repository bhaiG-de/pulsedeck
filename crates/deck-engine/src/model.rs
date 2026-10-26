use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrackAnalysis {
    #[serde(default)]
    pub audio_measurements: Option<audio_analysis::AudioMeasurements>,
    pub id: String,
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
    /// Service sources can prohibit recording even though native transport is available.
    #[serde(default = "recording_allowed_default")]
    pub recording_allowed: bool,
    pub bpm: Option<f64>,
    pub bpm_confidence: f32,
    #[serde(default)]
    pub analysis_version: u32,
    #[serde(default)]
    pub bpm_candidates: Vec<audio_analysis::TempoCandidate>,
    #[serde(default)]
    pub bpm_ambiguous: bool,
    #[serde(default)]
    pub bpm_source: Option<String>,
    #[serde(default)]
    pub beatgrid_offset_seconds: Option<f64>,
    #[serde(default)]
    pub beatgrid_confidence: Option<f32>,
    #[serde(default)]
    pub beatgrid_source: Option<String>,
    #[serde(default)]
    pub neural_rhythm: Option<audio_analysis::NeuralRhythm>,
    #[serde(default)]
    pub rhythm_error: Option<String>,
    #[serde(default)]
    pub manual_grid_anchor_index: Option<usize>,
    #[serde(default)]
    pub key_candidates: Vec<audio_analysis::KeyCandidate>,
    /// Statistical major/minor estimate; ambiguous content retains alternatives.
    pub key: Option<String>,
    pub key_confidence: Option<f32>,
    pub camelot: Option<String>,
    #[serde(default)]
    pub waveform_bands: Option<audio_analysis::WaveformBands>,
    /// 4096 normalized stereo-maximum waveform peaks, generated off the audio thread.
    pub waveform: Vec<f32>,
    /// Neural beat/downbeat stage, versioned separately from the DSP analysis.
    /// `None` means the model has not run on this source yet.
    #[serde(default)]
    pub beat_model: Option<audio_analysis::BeatModelStage>,
    #[serde(default)]
    pub cue_points: Vec<audio_analysis::CuePoint>,
    #[serde(default)]
    pub source_fingerprint: Option<String>,
    #[serde(default)]
    pub source_release: Option<String>,
    #[serde(default)]
    pub analysis_stages: audio_analysis::AnalysisStages,
}
fn recording_allowed_default() -> bool {
    true
}

/// Large immutable-ish display data, emitted only when the metadata revision changes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineMetadataSnapshot {
    pub revision: u64,
    pub load_revisions: [u64; 2],
    pub tracks: [Option<TrackAnalysis>; 2],
    pub samples: [Option<TrackAnalysis>; 16],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub channels: u16,
    pub sample_rates: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioRouting {
    pub master_left: u16,
    pub master_right: u16,
    pub cue_left: Option<u16>,
    pub cue_right: Option<u16>,
}
impl Default for AudioRouting {
    fn default() -> Self {
        Self {
            master_left: 0,
            master_right: 1,
            cue_left: None,
            cue_right: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioStatus {
    pub running: bool,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: u32,
    pub routing: AudioRouting,
}

pub use control_protocol::commands::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeckSnapshot {
    #[serde(default)]
    pub grid_revision: u64,
    pub track: Option<TrackAnalysis>,
    pub playing: bool,
    pub position_seconds: f64,
    #[serde(default)]
    pub jog_touch: bool,
    /// Changes only on position discontinuities, never normal sample-clock advance.
    #[serde(default)]
    pub position_revision: u64,
    pub playback_rate: f64,
    pub actual_playback_rate: f64,
    pub playback_bpm: Option<f64>,
    pub local_bpm: Option<f64>,
    pub actual_playback_bpm: Option<f64>,
    pub target_playback_bpm: Option<f64>,
    pub tempo_percent: f64,
    pub gain: f32,
    #[serde(default = "recording_allowed_default")]
    pub normalization_enabled: bool,
    #[serde(default)]
    pub normalization_gain_db: f32,
    #[serde(default)]
    pub fx_tail_active: bool,
    #[serde(default)]
    pub fx_tail_peak: [f32; 2],
    #[serde(default)]
    pub restricted_tail_active: bool,
    pub channel_fader: f32,
    #[serde(default)]
    pub channel_fader_actual: f32,
    pub sync_enabled: bool,
    #[serde(default)]
    pub sync_rate_limits: Option<SyncRateLimits>,
    #[serde(default)]
    pub sync_rate_limited: bool,
    pub is_master: bool,
    pub eq_low: f32,
    pub eq_mid: f32,
    pub eq_high: f32,
    pub filter: f32,
    pub cue_seconds: f64,
    pub hot_cues: [Option<f64>; 8],
    pub memory_cues: [Option<f64>; 10],
    pub peak: [f32; 2],
    pub loop_start_seconds: Option<f64>,
    pub loop_end_seconds: Option<f64>,
    pub loop_edit: Option<LoopBoundary>,
    pub fx_beats: f64,
    pub cue_enabled: bool,
    pub echo: f32,
    pub reverb: f32,
    pub bpm: Option<f64>,
    pub beatgrid_offset_seconds: f64,
    /// A zero offset alone does not establish that a grid was detected.
    #[serde(default)]
    pub beatgrid_valid: bool,
    /// Only a manually established origin identifies musical bar one.
    #[serde(default)]
    pub beatgrid_downbeat: bool,
    #[serde(default)]
    pub beatgrid_manual: bool,
    #[serde(default)]
    pub beatgrid_uniform: bool,
    #[serde(default)]
    pub beatgrid_anchor_index: Option<usize>,
    #[serde(default)]
    pub beat_position: Option<f64>,
    #[serde(default)]
    pub launch_pending: bool,
    #[serde(default)]
    pub launch_clock: Option<usize>,
    #[serde(default)]
    pub launch_at_seconds: Option<f64>,
    pub pitch_semitones: f64,
    pub keylock: bool,
    pub preparation_pending: bool,
}

/// Native output and continuous processor diagnostics. Peak protection is a
/// sample-peak limiter with a conservative -1 dBFS ceiling, not a promise that
/// pathological intersample peaks or upstream source clipping can be repaired.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AudioQualitySnapshot {
    pub headroom_db: f32,
    pub output_latency_frames: u32,
    pub master_input_peak: [f32; 2],
    pub cue_input_peak: [f32; 2],
    pub master_gain_reduction_db: f32,
    pub cue_gain_reduction_db: f32,
    pub master_limited_frames: u64,
    pub cue_limited_frames: u64,
    pub invalid_signal_frames: u64,
    pub processor_resets: [u64; 2],
    pub processor_blocks: [u64; 2],
    pub callback_count: u64,
    pub callback_max_ns: u64,
    pub callback_deadline_exceeded: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineSnapshot {
    #[serde(default)]
    pub control_revision: u64,
    #[serde(default)]
    pub deck_control_revisions: [u64; 2],
    #[serde(default)]
    #[serde(deserialize_with = "read_control_revisions")]
    pub target_control_revisions: [u64; COMMAND_TARGET_COUNT],
    #[serde(default)]
    pub engine_epoch: u64,
    #[serde(default)]
    pub scheduled_batch: crate::scheduled::ScheduledBatchSnapshot,
    #[serde(default)]
    pub audio_quality: AudioQualitySnapshot,
    pub load_revisions: [u64; 2],
    pub sample_rate: u32,
    pub output_channels: u16,
    pub buffer_frames: u32,
    pub running: bool,
    pub rendered_frames: u64,
    pub master_peak: [f32; 2],
    pub cue_peak: [f32; 2],
    /// Count of CPAL device/stream errors. It is not a hardware underrun measurement.
    pub underruns: u64,
    pub master_gain: f32,
    pub crossfader: f32,
    pub cue_gain: f32,
    pub cue_mix: f32,
    pub sampler_gain: f32,
    pub sampler_positions_seconds: [Option<f64>; 16],
    #[serde(deserialize_with = "read_active_ramps")]
    pub active_ramps: [bool; CONTROL_TARGET_COUNT],
    pub decks: [DeckSnapshot; 2],
}

fn read_active_ramps<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<[bool; CONTROL_TARGET_COUNT], D::Error> {
    let values = Vec::<bool>::deserialize(d)?;
    if values.len() != 19 && values.len() != CONTROL_TARGET_COUNT {
        return Err(serde::de::Error::custom("Invalid ramp target count"));
    }
    let mut out = [false; CONTROL_TARGET_COUNT];
    out[..values.len()].copy_from_slice(&values);
    Ok(out)
}
fn read_control_revisions<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<[u64; COMMAND_TARGET_COUNT], D::Error> {
    let values = Vec::<u64>::deserialize(d)?;
    let mut out = [0; COMMAND_TARGET_COUNT];
    if values.len() == 21 {
        out[..19].copy_from_slice(&values[..19]);
        out[CONTROL_TARGET_COUNT..].copy_from_slice(&values[19..]);
    } else if values.len() == COMMAND_TARGET_COUNT {
        out.copy_from_slice(&values);
    } else {
        return Err(serde::de::Error::custom("Invalid command target count"));
    }
    Ok(out)
}

//! Advisory source-envelope/control forecast, never an audio-quality verdict.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TransitionForecast {
    pub origin_rendered_frame: u64,
    pub sample_rate: u32,
    /// All window times are renderer seconds from origin_rendered_frame.
    pub horizon_seconds: f64,
    pub method: String,
    pub sources: [ForecastSource; 2],
    pub windows: Vec<ForecastWindow>,
    pub omitted_ranges: Vec<[f64; 2]>,
    pub findings: Vec<ForecastFinding>,
    pub findings_omitted: usize,
    /// Declared native monitoring scopes; Sync source trajectories are not simulated.
    #[serde(default)]
    pub rhythm_contracts: Vec<ForecastRhythmContract>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForecastSource {
    pub deck: usize,
    pub source: Option<String>,
    pub load_revision: u64,
    pub grid_revision: u64,
    pub source_fingerprint: Option<String>,
    pub analysis_version: Option<u32>,
    pub bin_seconds: Option<f64>,
    pub key: Option<String>,
    pub camelot: Option<String>,
    pub key_confidence: Option<f32>,
    pub bpm_source: Option<String>,
    pub beats_per_bar: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ForecastCoverage {
    Available,
    Partial,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BandDominance {
    A,
    B,
    Shared,
    Low,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForecastWindow {
    pub seconds: [f64; 2],
    pub actions: Vec<String>,
    pub actions_omitted: usize,
    pub decks: [ForecastContribution; 2],
    /// Relative coarse-band envelope cues, not instrument or musical-role labels.
    pub band_dominance: [BandDominance; 3],
    pub fractional_phase_error_beats: Option<[f64; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForecastContribution {
    pub coverage: ForecastCoverage,
    pub uncertainty: Vec<String>,
    /// Maximum current control/clock start-time bound retained from the compiler.
    pub timing_uncertainty_seconds: f64,
    /// Persistent source-clock start bound; positions are nominal when nonzero.
    pub source_timing_uncertainty_seconds: f64,
    pub source_seconds: Option<[f64; 2]>,
    pub effective_beats: Option<[f64; 2]>,
    pub playing: bool,
    pub rate: Option<f64>,
    pub playing_bpm: Option<f64>,
    pub pitch_shift_semitones: Option<f64>,
    pub normalization_gain_db: Option<f64>,
    pub fader: [f32; 2],
    pub eq: [[f32; 3]; 2],
    /// Cached low/mid/high RMS after the observed playback normalization only.
    pub source_rms: Option<[f64; 3]>,
    /// Nominal source RMS * fader * gain * matching EQ-band weight. Analysis
    /// and EQ crossovers differ: this is an envelope cue, not rendered/summed audio.
    pub contribution_cue: Option<[f64; 3]>,
    /// Same nominal low-band cue with only EQ low set to unity, when modelled.
    pub low_eq_open_cue: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForecastFinding {
    pub code: String,
    pub seconds: [f64; 2],
    pub deck: Option<usize>,
    pub source_seconds: Option<[f64; 2]>,
    pub evidence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForecastRhythmContract {
    pub action: String,
    pub deck: usize,
    pub seconds: [f64; 2],
    pub phase_tolerance_beats: f64,
    pub bar_beats: Option<u8>,
    pub sync_rate_limits: Option<crate::commands::SyncRateLimits>,
}

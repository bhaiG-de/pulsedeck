//! Independently implemented, bounded offline musical analysis. Confidence scores
//! are heuristics, not probabilities. Public DSP references and limits: README.md.
mod beats;
mod key;
mod sections;
mod spectral;
mod tempo;
mod waveform;
pub use beats::{
    fit_grid, spectrogram, track_beats, track_beats_with_spectrogram, BeatModelStage, ModelGrid,
    Spectrogram, BEAT_MODEL_VERSION,
};
pub use key::{analyze_key, estimate_key, KeyAnalysis, KeyCandidate, KeyEstimate};
pub use sections::{
    detect_sections, plan_auto_cues, AutoCues, CuePlan, Section, SectionLabel, SectionStage,
    Vocabulary, SECTIONS_VERSION,
};
pub use tempo::{
    analyze_rhythm, estimate_phase, estimate_tempo, BeatPhaseEstimate, TempoCandidate,
    TempoEstimate,
};
pub use waveform::{analyze_waveform, WaveformAnalysis, WaveformBands};
pub const ANALYSIS_VERSION: u32 = 3;

//! Independently implemented, bounded offline musical analysis. Confidence scores
//! are heuristics, not probabilities. Public DSP references and limits: README.md.
mod key;
mod spectral;
mod tempo;
mod waveform;
pub use key::{analyze_key, estimate_key, KeyAnalysis, KeyCandidate, KeyEstimate};
pub use tempo::{
    analyze_rhythm, estimate_phase, estimate_tempo, BeatPhaseEstimate, TempoCandidate,
    TempoEstimate,
};
pub use waveform::{analyze_waveform, WaveformAnalysis, WaveformBands};
pub const ANALYSIS_VERSION: u32 = 3;

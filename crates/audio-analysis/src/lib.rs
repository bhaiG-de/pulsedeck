//! Independently implemented, bounded offline musical analysis. Confidence scores
//! are heuristics, not probabilities. Public DSP references and limits: README.md.
mod beats;
mod cache;
pub use cache::{
    encoded_file_fingerprint, source_fingerprint, AnalysisStages, StageMemo, StageOutcome,
    StageStamp,
};
mod cues;
mod dbn;
mod key;
mod measurements;
mod neural;
mod spectral;
mod tempo;
mod waveform;
pub use beats::{
    fit_grid, spectrogram, stage_from_rhythm, track_beats, track_beats_with_spectrogram,
    BeatModelStage, ModelGrid, Spectrogram, BEAT_MODEL_VERSION,
};
pub use cues::{CueBindings, CueCollection, CueOrigin, CuePoint, CueRole};
pub use dbn::{
    bar_recurrence_for_validation, decode_independent_beats_for_validation,
    decode_neural_logits_for_app_validation, decode_neural_logits_for_validation,
    decode_neural_logits_with_meters_for_validation, decode_separated_for_validation,
    decode_with_acoustic_meter_for_validation, BarEvidence, PulseRepair,
};
pub use key::{analyze_key, estimate_key, KeyAnalysis, KeyCandidate, KeyEstimate};
pub use measurements::{measure_audio, AudioMeasurements, MEASUREMENTS_VERSION};
pub use neural::{
    analyze_neural_rhythm, analyze_neural_rhythm_with_logit_observer, initialize_neural_rhythm,
    legacy_neural_provenance_current, neural_cache_identity,
    neural_logits_from_spectrogram_for_validation, NeuralRhythm,
};

pub use tempo::{
    analyze_rhythm, estimate_phase, estimate_tempo, BeatPhaseEstimate, TempoCandidate,
    TempoEstimate,
};
pub use waveform::{analyze_waveform, WaveformAnalysis, WaveformBands};
pub const ANALYSIS_VERSION: u32 = 11;

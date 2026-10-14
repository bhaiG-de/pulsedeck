use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackAnalysis {
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
    /// Song sections cut on the track's grid; `None` until analysed or when
    /// the grid has no known beat 1.
    #[serde(default)]
    pub sections: Option<audio_analysis::SectionStage>,
    /// Cues PulseDeck placed from the sections, so they stay distinguishable.
    #[serde(default)]
    pub auto_cues: Option<audio_analysis::AutoCues>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoopBoundary {
    In,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlTarget {
    Crossfader,
    MasterGain,
    CueGain,
    CueMix,
    SamplerGain,
    ChannelFader { deck: usize },
    DeckGain { deck: usize },
    Filter { deck: usize },
    EqLow { deck: usize },
    EqMid { deck: usize },
    EqHigh { deck: usize },
}
impl ControlTarget {
    pub fn deck(self) -> Option<usize> {
        match self {
            Self::ChannelFader { deck }
            | Self::DeckGain { deck }
            | Self::Filter { deck }
            | Self::EqLow { deck }
            | Self::EqMid { deck }
            | Self::EqHigh { deck } => Some(deck),
            _ => None,
        }
    }
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Crossfader => 0,
            Self::MasterGain => 1,
            Self::CueGain => 2,
            Self::CueMix => 3,
            Self::SamplerGain => 4,
            Self::ChannelFader { deck } => 5 + deck * 6,
            Self::DeckGain { deck } => 6 + deck * 6,
            Self::Filter { deck } => 7 + deck * 6,
            Self::EqLow { deck } => 8 + deck * 6,
            Self::EqMid { deck } => 9 + deck * 6,
            Self::EqHigh { deck } => 10 + deck * 6,
        }
    }
    fn valid_value(self, value: f32) -> bool {
        value.is_finite()
            && match self {
                Self::Crossfader | Self::Filter { .. } => (-1.0..=1.0).contains(&value),
                Self::ChannelFader { .. } | Self::CueMix => (0.0..=1.0).contains(&value),
                _ => (0.0..=2.0).contains(&value),
            }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeckSnapshot {
    pub track: Option<TrackAnalysis>,
    pub playing: bool,
    pub position_seconds: f64,
    #[serde(default)]
    pub jog_touch: bool,
    /// Changes only on position discontinuities, never normal sample-clock advance.
    #[serde(default)]
    pub position_revision: u64,
    pub playback_rate: f64,
    pub tempo_percent: f64,
    pub gain: f32,
    pub channel_fader: f32,
    pub sync_enabled: bool,
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
    pub pitch_semitones: f64,
    pub keylock: bool,
    pub preparation_pending: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineSnapshot {
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
    pub active_ramps: [bool; 17],
    pub decks: [DeckSnapshot; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EngineCommand {
    Ramp {
        target: ControlTarget,
        value: f32,
        seconds: f64,
        clock: Option<usize>,
        beats: Option<f64>,
        quantize: Option<f64>,
    },
    CancelRamps,
    Play {
        deck: usize,
    },
    Pause {
        deck: usize,
    },
    TogglePlay {
        deck: usize,
    },
    Cue {
        deck: usize,
    },
    SetCue {
        deck: usize,
    },
    SetCuePoint {
        deck: usize,
        seconds: f64,
    },
    RestoreHotCues {
        deck: usize,
        cues: [Option<f64>; 8],
    },
    CuePress {
        deck: usize,
        pressed: bool,
    },
    Seek {
        deck: usize,
        seconds: f64,
    },
    /// Traverse source PCM while the cursor holds the platter. No seek jump.
    ScrubTo {
        deck: usize,
        seconds: f64,
        #[schemars(range(min = 8, max = 100))]
        duration_ms: f64,
    },
    ScrubEnd {
        deck: usize,
        seconds: f64,
    },
    SetHotCue {
        deck: usize,
        index: usize,
    },
    SetHotCuePoint {
        deck: usize,
        index: usize,
        seconds: f64,
    },
    TriggerHotCue {
        deck: usize,
        index: usize,
    },
    ToggleActiveLoop {
        deck: usize,
    },
    SetLoop {
        deck: usize,
        beats: Option<f64>,
    },
    SetLoopEdit {
        deck: usize,
        boundary: Option<LoopBoundary>,
    },
    SetFxBeat {
        deck: usize,
        beats: f64,
    },
    LoopIn {
        deck: usize,
    },
    LoopOut {
        deck: usize,
    },
    ClearHotCue {
        deck: usize,
        index: usize,
    },
    SetPitch {
        deck: usize,
        #[schemars(range(min = -12, max = 12))]
        semitones: f64,
    },
    JogTouch {
        deck: usize,
        touched: bool,
    },
    MemoryCueSave {
        deck: usize,
    },
    MemoryCueDelete {
        deck: usize,
    },
    MemoryCueNext {
        deck: usize,
    },
    MemoryCuePrevious {
        deck: usize,
    },
    RestoreMemoryCues {
        deck: usize,
        cues: [Option<f64>; 10],
    },
    TriggerSample {
        slot: usize,
    },
    StopSample {
        slot: usize,
    },
    SetSamplerGain {
        #[schemars(range(min = 0, max = 2))]
        gain: f32,
    },
    SetKeylock {
        deck: usize,
        enabled: bool,
    },
    BeatJump {
        deck: usize,
        beats: f64,
    },
    SetTempo {
        deck: usize,
        #[schemars(range(min = -90, max = 100))]
        percent: f64,
    },
    Jog {
        deck: usize,
        delta_seconds: f64,
        scratch: bool,
    },
    Sync {
        deck: usize,
    },
    SetSync {
        deck: usize,
        enabled: bool,
    },
    SetMaster {
        deck: usize,
    },
    SetChannelFader {
        deck: usize,
        value: f32,
    },
    SetDeckGain {
        deck: usize,
        #[schemars(range(min = 0, max = 2))]
        gain: f32,
    },
    SetEq {
        deck: usize,
        #[schemars(range(min = 0, max = 2))]
        low: f32,
        #[schemars(range(min = 0, max = 2))]
        mid: f32,
        #[schemars(range(min = 0, max = 2))]
        high: f32,
    },
    SetFilter {
        deck: usize,
        #[schemars(range(min = -1, max = 1))]
        amount: f32,
    },
    SetCrossfader {
        #[schemars(range(min = -1, max = 1))]
        position: f32,
    },
    SetMasterGain {
        #[schemars(range(min = 0, max = 2))]
        gain: f32,
    },
    SetCueMix {
        #[schemars(range(min = 0, max = 1))]
        mix: f32,
    },
    SetCueGain {
        #[schemars(range(min = 0, max = 2))]
        gain: f32,
    },
    SetHeadphoneCue {
        deck: usize,
        enabled: bool,
    },
    SetFx {
        deck: usize,
        #[schemars(range(min = 0, max = 1))]
        echo: f32,
        #[schemars(range(min = 0, max = 1))]
        reverb: f32,
    },
    SetBpm {
        deck: usize,
        #[schemars(range(min = 30, max = 300))]
        bpm: f64,
    },
    SetBeatgrid {
        deck: usize,
        offset_seconds: f64,
    },
}
impl EngineCommand {
    pub fn deck(&self) -> Option<usize> {
        use EngineCommand::*;
        match self {
            Ramp { target, clock, .. } => target.deck().or(*clock),
            Play { deck }
            | Pause { deck }
            | TogglePlay { deck }
            | Cue { deck }
            | SetCue { deck }
            | Seek { deck, .. }
            | ScrubTo { deck, .. }
            | ScrubEnd { deck, .. }
            | SetHotCue { deck, .. }
            | SetHotCuePoint { deck, .. }
            | TriggerHotCue { deck, .. }
            | ToggleActiveLoop { deck }
            | SetLoop { deck, .. }
            | BeatJump { deck, .. }
            | SetTempo { deck, .. }
            | Jog { deck, .. }
            | Sync { deck }
            | SetDeckGain { deck, .. }
            | SetEq { deck, .. }
            | SetFilter { deck, .. }
            | SetHeadphoneCue { deck, .. }
            | SetFx { deck, .. }
            | SetBpm { deck, .. }
            | SetBeatgrid { deck, .. }
            | SetCuePoint { deck, .. }
            | RestoreHotCues { deck, .. }
            | CuePress { deck, .. }
            | SetSync { deck, .. }
            | SetMaster { deck }
            | SetChannelFader { deck, .. }
            | SetLoopEdit { deck, .. }
            | SetFxBeat { deck, .. }
            | LoopIn { deck }
            | LoopOut { deck }
            | ClearHotCue { deck, .. }
            | SetPitch { deck, .. }
            | SetKeylock { deck, .. }
            | JogTouch { deck, .. }
            | MemoryCueSave { deck }
            | MemoryCueDelete { deck }
            | MemoryCueNext { deck }
            | MemoryCuePrevious { deck }
            | RestoreMemoryCues { deck, .. } => Some(*deck),
            _ => None,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        use EngineCommand::*;
        let deck = self.deck();
        if deck.is_some_and(|d| d > 1) {
            return Err("Deck must be 0 or 1".into());
        }
        match self {
            Ramp {
                target,
                value,
                seconds,
                clock,
                beats,
                quantize,
            } if target.deck().is_some_and(|d| d > 1)
                || clock.is_some_and(|d| d > 1)
                || !target.valid_value(*value)
                || !seconds.is_finite()
                || !(0.001..=3600.0).contains(seconds)
                || beats.is_some_and(|b| !b.is_finite() || !(0.001..=256.0).contains(&b))
                || quantize.is_some_and(|b| !b.is_finite() || !(0.001..=256.0).contains(&b))
                || (beats.is_some() || quantize.is_some()) && clock.is_none() =>
            {
                return Err(
                    "Ramp requires a valid target/value, duration and optional deck beat clock"
                        .into(),
                );
            }
            TriggerSample { slot } | StopSample { slot } if *slot >= 16 => {
                return Err("Sample slot must be 0..15".into())
            }
            RestoreMemoryCues { cues, .. }
                if cues.iter().flatten().any(|v| !v.is_finite() || *v < 0.0) =>
            {
                return Err("Memory cue times must be finite and positive".into())
            }
            SetHotCue { index, .. }
            | SetHotCuePoint { index, .. }
            | TriggerHotCue { index, .. }
            | ClearHotCue { index, .. }
                if *index >= 8 =>
            {
                return Err("Hot cue index must be 0..7".into())
            }
            ScrubTo {
                seconds,
                duration_ms,
                ..
            } if !seconds.is_finite()
                || !duration_ms.is_finite()
                || !(8.0..=100.0).contains(duration_ms) =>
            {
                return Err("Scrub target must be finite and duration must be 8..100 ms".into())
            }
            Seek { seconds, .. }
            | SetHotCuePoint { seconds, .. }
            | ScrubEnd { seconds, .. }
            | Jog {
                delta_seconds: seconds,
                ..
            }
            | SetBeatgrid {
                offset_seconds: seconds,
                ..
            }
            | SetCuePoint { seconds, .. }
                if !seconds.is_finite() =>
            {
                return Err("Time must be finite".into())
            }
            SetFxBeat { beats, .. }
                if !beats.is_finite() || !(1.0 / 32.0..=16.0).contains(beats) =>
            {
                return Err("FX beat length must be 1/32..16 beats".into())
            }
            SetLoop {
                beats: Some(beats), ..
            } if !beats.is_finite() || *beats <= 0.0 || *beats > 256.0 => {
                return Err("Loop length must be 0..256 beats".into())
            }
            BeatJump { beats, .. } if !beats.is_finite() || beats.abs() > 256.0 => {
                return Err("Beat jump must be within 256 beats".into())
            }
            SetTempo { percent, .. }
                if !percent.is_finite() || !(-90.0..=100.0).contains(percent) =>
            {
                return Err("Tempo must be -90..100 percent".into())
            }
            SetPitch { semitones, .. }
                if !semitones.is_finite() || !(-12.0..=12.0).contains(semitones) =>
            {
                return Err("Pitch must be -12..12 semitones".into())
            }
            SetBpm { bpm, .. } if !bpm.is_finite() || !(30.0..=300.0).contains(bpm) => {
                return Err("BPM must be 30..300".into())
            }
            SetChannelFader { value, .. } if !value.is_finite() || !(0.0..=1.0).contains(value) => {
                return Err("Channel fader must be 0..1".into())
            }
            RestoreHotCues { cues, .. }
                if cues.iter().flatten().any(|v| !v.is_finite() || *v < 0.0) =>
            {
                return Err("Cue times must be finite and positive".into())
            }
            SetDeckGain { gain, .. }
            | SetMasterGain { gain }
            | SetCueGain { gain }
            | SetSamplerGain { gain }
                if !gain.is_finite() || !(0.0..=2.0).contains(gain) =>
            {
                return Err("Gain must be 0..2".into())
            }
            SetEq { low, mid, high, .. }
                if [*low, *mid, *high]
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=2.0).contains(v)) =>
            {
                return Err("EQ gains must be 0..2".into())
            }
            SetFilter { amount, .. } | SetCrossfader { position: amount }
                if !amount.is_finite() || !(-1.0..=1.0).contains(amount) =>
            {
                return Err("Control must be -1..1".into())
            }
            SetCueMix { mix } if !mix.is_finite() || !(0.0..=1.0).contains(mix) => {
                return Err("Cue mix must be 0..1".into())
            }
            SetFx { echo, reverb, .. }
                if [*echo, *reverb]
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) =>
            {
                return Err("FX amounts must be 0..1".into())
            }
            _ => {}
        }
        Ok(())
    }
}

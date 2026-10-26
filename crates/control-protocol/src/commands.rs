//! Shared renderer vocabulary. No device, DSP or source services live here.
use serde::{Deserialize, Serialize};
pub fn deck_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    serde_json::from_value(serde_json::json!({"oneOf":[{"type":"integer","enum":[0,1]},{"type":"string","enum":["A","B"]}]})).unwrap()
}
pub fn optional_deck_schema(g: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    serde_json::from_value(serde_json::json!({"anyOf":[deck_schema(g),{"type":"null"}]})).unwrap()
}
pub fn deserialize_deck<'de, D: serde::Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("a") => Ok(0),
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("b") => Ok(1),
        serde_json::Value::Number(n) if n.as_u64().is_some_and(|i| i <= 1) => {
            Ok(n.as_u64().unwrap() as usize)
        }
        _ => Err(serde::de::Error::custom("Deck must be A/B or 0/1")),
    }
}
pub fn deserialize_optional_deck<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<usize>, D::Error> {
    let v = Option::<serde_json::Value>::deserialize(d)?;
    v.map(deserialize_deck)
        .transpose()
        .map_err(serde::de::Error::custom)
}

/// Optional expectations for a direct deck control. Omit for intentional live
/// seeking/juggling; preparation can require its source to remain paused/isolated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectStateGuard {
    #[serde(default)]
    pub playing: Option<bool>,
    #[serde(default)]
    pub launch_pending: Option<bool>,
    /// Maximum target AND settled channel fader, in native linear 0..1 units.
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub channel_fader_max: Option<f32>,
}
impl DirectStateGuard {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .channel_fader_max
            .is_some_and(|n| !n.is_finite() || !(0.0..=1.0).contains(&n))
        {
            return Err("channel_fader_max must be finite in 0..1");
        }
        Ok(())
    }
    pub fn failure(&self, state: &DirectDeckState) -> Option<CommandFailureCause> {
        if self.playing.is_some_and(|v| v != state.playing) {
            Some(CommandFailureCause::PlayingMismatch)
        } else if self
            .launch_pending
            .is_some_and(|v| v != state.launch_pending)
        {
            Some(CommandFailureCause::LaunchPendingMismatch)
        } else if self
            .channel_fader_max
            .is_some_and(|v| state.channel_fader > v || state.channel_fader_actual > v)
        {
            Some(CommandFailureCause::FaderNotIsolated)
        } else {
            None
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DirectDeckState {
    pub load_revision: u64,
    pub position_revision: u64,
    pub playing: bool,
    pub position_seconds: f64,
    pub channel_fader: f32,
    pub channel_fader_actual: f32,
    pub launch_pending: bool,
    #[schemars(schema_with = "optional_deck_schema")]
    pub launch_clock: Option<usize>,
    pub launch_at_seconds: Option<f64>,
    pub channel_fader_ramp_active: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DirectControlOutcome {
    Applied,
    CancelledPending,
    NoPendingLaunch,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DirectControlReceipt {
    pub ticket: u64,
    pub frame: u64,
    #[schemars(schema_with = "deck_schema")]
    pub deck: usize,
    pub outcome: DirectControlOutcome,
    pub failure: Option<CommandFailureCause>,
    pub before: DirectDeckState,
    pub after: DirectDeckState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoopBoundary {
    In,
    Out,
}

/// Bounds for automatic Sync adaptation; explicit tempo/jog ownership is separate.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncRateLimits {
    pub min_rate: f64,
    pub max_rate: f64,
    /// Playback-rate units per second (0.01 means one percentage point/second).
    pub max_slew_per_second: f64,
}
impl SyncRateLimits {
    pub fn validate(self) -> Result<(), String> {
        if !self.min_rate.is_finite()
            || !self.max_rate.is_finite()
            || !(0.1..=2.0).contains(&self.min_rate)
            || !(self.min_rate..=2.0).contains(&self.max_rate)
            || !self.max_slew_per_second.is_finite()
            || !(0.000_001..=2.0).contains(&self.max_slew_per_second)
        {
            return Err("Sync rate limits require 0.1 <= min_rate <= max_rate <= 2 and a positive max_slew_per_second <= 2".into());
        }
        Ok(())
    }
    pub fn contains(self, rate: f64) -> bool {
        rate.is_finite() && rate >= self.min_rate - 1e-9 && rate <= self.max_rate + 1e-9
    }
}

/// Convert an absolute playback target without changing the source beat grid.
pub fn playback_tempo_percent(reference: Option<f64>, bpm: f64) -> Result<f64, &'static str> {
    if !bpm.is_finite() || !(30.0..=300.0).contains(&bpm) {
        return Err("Playback BPM must be 30..300");
    }
    let reference = reference
        .filter(|b| b.is_finite() && *b > 0.)
        .ok_or("Playback BPM requires a valid loaded beat-grid BPM")?;
    let percent = (bpm / reference - 1.) * 100.;
    if !(-90.0..=100.0).contains(&percent) {
        return Err("Playback BPM requires a tempo outside -90..100 percent");
    }
    Ok(percent)
}

pub const CONTROL_TARGET_COUNT: usize = 23;
pub const COMMAND_TARGET_COUNT: usize = CONTROL_TARGET_COUNT + 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlTarget {
    Crossfader,
    MasterGain,
    CueGain,
    CueMix,
    SamplerGain,
    ChannelFader {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    DeckGain {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    Filter {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    EqLow {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    EqMid {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    EqHigh {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    TempoPercent {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    Echo {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    Reverb {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
}
impl ControlTarget {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Crossfader,
            1 => Self::MasterGain,
            2 => Self::CueGain,
            3 => Self::CueMix,
            4 => Self::SamplerGain,
            17 | 18 => Self::TempoPercent { deck: i - 17 },
            19 | 20 => Self::Echo { deck: i - 19 },
            21 | 22 => Self::Reverb { deck: i - 21 },
            _ => {
                let deck = (i - 5) / 6;
                match (i - 5) % 6 {
                    0 => Self::ChannelFader { deck },
                    1 => Self::DeckGain { deck },
                    2 => Self::Filter { deck },
                    3 => Self::EqLow { deck },
                    4 => Self::EqMid { deck },
                    _ => Self::EqHigh { deck },
                }
            }
        }
    }

    pub fn deck(self) -> Option<usize> {
        match self {
            Self::ChannelFader { deck }
            | Self::DeckGain { deck }
            | Self::Filter { deck }
            | Self::EqLow { deck }
            | Self::EqMid { deck }
            | Self::EqHigh { deck }
            | Self::TempoPercent { deck }
            | Self::Echo { deck }
            | Self::Reverb { deck } => Some(deck),
            _ => None,
        }
    }
    pub fn index(self) -> usize {
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
            Self::TempoPercent { deck } => 17 + deck,
            Self::Echo { deck } => 19 + deck,
            Self::Reverb { deck } => 21 + deck,
        }
    }
    fn valid_value(self, value: f32) -> bool {
        value.is_finite()
            && match self {
                Self::TempoPercent { .. } => (-90.0..=100.0).contains(&value),
                Self::Crossfader | Self::Filter { .. } => (-1.0..=1.0).contains(&value),
                Self::Echo { .. }
                | Self::Reverb { .. }
                | Self::ChannelFader { .. }
                | Self::CueMix => (0.0..=1.0).contains(&value),
                _ => (0.0..=2.0).contains(&value),
            }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EngineCommand {
    SetSyncRateLimits {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        limits: Option<SyncRateLimits>,
    },
    Ramp {
        target: ControlTarget,
        /// Target units: filter/crossfader -1..1; channel/effects 0..1;
        /// gain/EQ 0..2; source-relative tempo percent -90..100.
        value: f32,
        #[schemars(range(min = 0.001, max = 3600))]
        seconds: f64,
        #[serde(default, deserialize_with = "deserialize_optional_deck")]
        #[schemars(schema_with = "optional_deck_schema")]
        clock: Option<usize>,
        beats: Option<f64>,
        quantize: Option<f64>,
    },
    CancelRamps,
    Play {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    /// Start a cued deck on the clock deck's next grid boundary or explicit
    /// future source timestamp. Both sources remain guarded until launch.
    PlayQuantized {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        clock: usize,
        clock_revision: u64,
        #[serde(default)]
        at_seconds: Option<f64>,
        quantize: f64,
    },
    CancelLaunch {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    Pause {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    TogglePlay {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    Cue {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    SetCue {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    SetCuePoint {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        seconds: f64,
    },
    RestoreHotCues {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        cues: [Option<f64>; 8],
    },
    CuePress {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        pressed: bool,
    },
    Seek {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        seconds: f64,
    },
    /// Traverse source PCM while the cursor holds the platter. No seek jump.
    ScrubTo {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        seconds: f64,
        #[schemars(range(min = 8, max = 100))]
        duration_ms: f64,
    },
    ScrubEnd {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        seconds: f64,
    },
    SetHotCue {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        index: usize,
    },
    SetHotCuePoint {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        index: usize,
        seconds: f64,
    },
    TriggerHotCue {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        index: usize,
    },
    ToggleActiveLoop {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    SetLoop {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        beats: Option<f64>,
    },
    SetLoopEdit {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        boundary: Option<LoopBoundary>,
    },
    SetFxBeat {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        beats: f64,
    },
    LoopIn {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    LoopOut {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    ClearHotCue {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        index: usize,
    },
    SetPitch {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = -12, max = 12))]
        semitones: f64,
    },
    JogTouch {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        touched: bool,
    },
    MemoryCueSave {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    MemoryCueDelete {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    MemoryCueNext {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    MemoryCuePrevious {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    RestoreMemoryCues {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
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
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        enabled: bool,
    },
    BeatJump {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        beats: f64,
    },
    SetTempo {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = -90, max = 100))]
        percent: f64,
    },
    SetPlaybackBpm {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = 30, max = 300))]
        bpm: f64,
    },
    Jog {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        delta_seconds: f64,
        scratch: bool,
    },
    Sync {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    SetSync {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        enabled: bool,
    },
    SetMaster {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
    },
    /// Linear channel gain: 0 isolates the deck and 1 opens it fully.
    SetChannelFader {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = 0, max = 1))]
        value: f32,
    },
    SetDeckGain {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = 0, max = 2))]
        gain: f32,
    },
    /// Three independent linear amplitude gains (not dB); 1 is unity.
    SetEq {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = 0, max = 2))]
        low: f32,
        #[schemars(range(min = 0, max = 2))]
        mid: f32,
        #[schemars(range(min = 0, max = 2))]
        high: f32,
    },
    /// Signed sweep: negative low-pass, zero neutral, positive high-pass.
    SetFilter {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
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
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        enabled: bool,
    },
    SetNormalization {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        enabled: bool,
    },
    SetFx {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = 0, max = 1))]
        echo: f32,
        #[schemars(range(min = 0, max = 1))]
        reverb: f32,
    },
    SetBpm {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        #[schemars(range(min = 30, max = 300))]
        bpm: f64,
    },
    SetBeatgrid {
        #[serde(deserialize_with = "deserialize_deck")]
        #[schemars(schema_with = "deck_schema")]
        deck: usize,
        offset_seconds: f64,
        #[serde(default)]
        anchor_index: Option<usize>,
    },
}
impl EngineCommand {
    pub fn deck(&self) -> Option<usize> {
        use EngineCommand::*;
        match self {
            Ramp { target, clock, .. } => target.deck().or(*clock),
            Play { deck }
            | PlayQuantized { deck, .. }
            | CancelLaunch { deck }
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
            | SetPlaybackBpm { deck, .. }
            | Jog { deck, .. }
            | Sync { deck }
            | SetDeckGain { deck, .. }
            | SetEq { deck, .. }
            | SetFilter { deck, .. }
            | SetHeadphoneCue { deck, .. }
            | SetNormalization { deck, .. }
            | SetFx { deck, .. }
            | SetBpm { deck, .. }
            | SetBeatgrid { deck, .. }
            | SetCuePoint { deck, .. }
            | RestoreHotCues { deck, .. }
            | CuePress { deck, .. }
            | SetSync { deck, .. }
            | SetSyncRateLimits { deck, .. }
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
            SetSyncRateLimits {
                limits: Some(limits),
                ..
            } => limits.validate()?,
            PlayQuantized {
                deck,
                clock,
                at_seconds,
                quantize,
                ..
            } if *clock > 1
                || deck == clock
                || !quantize.is_finite()
                || quantize.fract() != 0.0
                || !(1.0..=256.0).contains(quantize)
                || at_seconds.is_some_and(|s| !s.is_finite() || s < 0.0) =>
            {
                return Err("Quantized play requires distinct decks, a finite future timestamp and 1..256 grid beats".into());
            }
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
            SetPlaybackBpm { bpm, .. } | SetBpm { bpm, .. }
                if !bpm.is_finite() || !(30.0..=300.0).contains(bpm) =>
            {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[repr(u64)]
pub enum CommandFailureCause {
    SourceMissing = 1,
    ClockSourceChanged,
    ClockNotPlaying,
    ClockEof,
    IncomingAlreadyPlaying,
    GridMissing,
    JogHeld,
    LoopActive,
    PreparationPending,
    SyncMasterSelf,
    HotCueMissing,
    PlaybackBpmReferenceMissing,
    PlaybackBpmUnreachable,
    DependencyFailed,
    LaunchAborted,
    DeadlineMissed,
    SyncRateOutsideLimits,
    AlignmentFailed,
    SourceChanged,
    ControlChanged,
    PlayingMismatch,
    LaunchPendingMismatch,
    FaderNotIsolated,
    AudioNotRunning,
    ControlEndpointUnverified,
}
impl CommandFailureCause {
    pub fn from_code(code: u64) -> Option<Self> {
        Some(match code {
            1 => Self::SourceMissing,
            2 => Self::ClockSourceChanged,
            3 => Self::ClockNotPlaying,
            4 => Self::ClockEof,
            5 => Self::IncomingAlreadyPlaying,
            6 => Self::GridMissing,
            7 => Self::JogHeld,
            8 => Self::LoopActive,
            9 => Self::PreparationPending,
            10 => Self::SyncMasterSelf,
            11 => Self::HotCueMissing,
            12 => Self::PlaybackBpmReferenceMissing,
            13 => Self::PlaybackBpmUnreachable,
            14 => Self::DependencyFailed,
            15 => Self::LaunchAborted,
            16 => Self::DeadlineMissed,
            17 => Self::SyncRateOutsideLimits,
            18 => Self::AlignmentFailed,
            19 => Self::SourceChanged,
            20 => Self::ControlChanged,
            21 => Self::PlayingMismatch,
            22 => Self::LaunchPendingMismatch,
            23 => Self::FaderNotIsolated,
            24 => Self::AudioNotRunning,
            25 => Self::ControlEndpointUnverified,
            _ => return None,
        })
    }
}

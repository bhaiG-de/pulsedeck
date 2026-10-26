//! Bounded scheduled batches. Plans compile off the callback; execution reuses
//! EngineCommand and native ramps, with no allocation, locks or IO per frame.
use crate::{ControlTarget, EngineCommand, EngineSnapshot};
use serde::{Deserialize, Serialize};

fn deck_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    serde_json::from_value(serde_json::json!({"oneOf":[{"type":"integer","enum":[0,1]},{"type":"string","enum":["A","B"]}]})).unwrap()
}
fn optional_deck_schema(g: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    serde_json::from_value(serde_json::json!({"anyOf":[deck_schema(g),{"type":"null"}]})).unwrap()
}
fn deserialize_deck<'de, D: serde::Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("A") => Ok(0),
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("B") => Ok(1),
        serde_json::Value::Number(n) if n.as_u64().is_some_and(|i| i <= 1) => {
            Ok(n.as_u64().unwrap() as usize)
        }
        _ => Err(serde::de::Error::custom("Deck must be A/B or 0/1")),
    }
}
fn deserialize_optional_deck<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<usize>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(d)?;
    value
        .map(deserialize_deck)
        .transpose()
        .map_err(serde::de::Error::custom)
}
fn deserialize_controls<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<EngineCommand>, D::Error> {
    fn normalize(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(o) => {
                for (k, v) in o {
                    if matches!(k.as_str(), "deck" | "clock") {
                        if let Some(s) = v.as_str() {
                            if s.eq_ignore_ascii_case("A") {
                                *v = serde_json::json!(0);
                            } else if s.eq_ignore_ascii_case("B") {
                                *v = serde_json::json!(1);
                            }
                        }
                    } else {
                        normalize(v);
                    }
                }
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    normalize(v);
                }
            }
            _ => {}
        }
    }
    let mut v = serde_json::Value::deserialize(d)?;
    normalize(&mut v);
    serde_json::from_value(v).map_err(serde::de::Error::custom)
}

pub const MAX_SCHEDULED_STEPS: usize = 64;
pub const MAX_RECOVERY_CONTROLS: usize = 8;
pub const MAX_QUEUE_UPDATES: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchSchedule {
    pub operation_id: String,
    /// Open queues accept append/invalidate patches until sealed; exhaustion waits.
    #[serde(default)]
    pub open: bool,
    /// Mandatory renderer-clock lease for an open queue; EOF remains a hard stop.
    #[serde(default)]
    pub expiry_seconds: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional_deck")]
    #[schemars(schema_with = "optional_deck_schema")]
    pub completion_carrier: Option<usize>,
    #[serde(deserialize_with = "deserialize_deck")]
    #[schemars(schema_with = "deck_schema")]
    pub clock: usize,
    pub expected_load_revisions: [u64; 2],
    pub expected_position_revisions: [u64; 2],
    pub expected_grid_revisions: [u64; 2],
    pub expected_engine_epoch: u64,
    pub expected_control_revision: u64,
    pub arm_before_event: i64,
    pub completion_event: i64,
    #[serde(default)]
    pub pause_outgoing: bool,
    /// Persisted user preferences/plan notes; never interpreted as commands.
    #[serde(default)]
    pub intent: serde_json::Value,
    pub recovery: BatchRecovery,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchRecovery {
    #[serde(deserialize_with = "deserialize_deck")]
    #[schemars(schema_with = "deck_schema")]
    pub carrier: usize,
    pub seconds: f64,
    #[serde(deserialize_with = "deserialize_controls")]
    pub controls: Vec<EngineCommand>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchPrerequisite {
    #[serde(deserialize_with = "deserialize_deck")]
    #[schemars(schema_with = "deck_schema")]
    pub deck: usize,
    #[serde(default)]
    pub playing: Option<bool>,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub advanced_from_seconds: Option<f64>,
    #[serde(default)]
    pub min_advanced_beats: f64,
    /// Beat phase tolerance against the other deck; bar phase is checked when
    /// bar_beats is supplied. This is telemetry proof, not arrangement proof.
    #[serde(default)]
    pub phase_tolerance_beats: Option<f64>,
    #[serde(default)]
    pub bar_beats: Option<u8>,
    #[serde(default)]
    pub arrival_before_event: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduledCommand {
    pub at_event: i64,
    pub deadline_event: i64,
    pub command: EngineCommand,
    pub require: Option<BatchPrerequisite>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchLifecycle {
    #[default]
    Idle,
    Armed,
    Running,
    Recovering,
    Completed,
    Recovered,
    Failed,
    Cancelled,
    Yielded,
    Interrupted,
}
impl BatchLifecycle {
    pub fn active(self) -> bool {
        matches!(self, Self::Armed | Self::Running | Self::Recovering)
    }
    pub fn code(self) -> u64 {
        self as u64
    }
    pub fn from_code(v: u64) -> Self {
        match v {
            1 => Self::Armed,
            2 => Self::Running,
            3 => Self::Recovering,
            4 => Self::Completed,
            5 => Self::Recovered,
            6 => Self::Failed,
            7 => Self::Cancelled,
            8 => Self::Yielded,
            9 => Self::Interrupted,
            _ => Self::Idle,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchReason {
    #[default]
    None,
    DeadlineExpired,
    SourceChanged,
    PositionChanged,
    GridChanged,
    TempoChanged,
    ManualTakeover,
    ClockStopped,
    AudioStopped,
    ResourceConflict,
    PrerequisiteFailed,
    EndpointFailed,
    RecoveryUnavailable,
    CancelRequested,
    RevisionConflict,
    InvalidPatch,
    CapacityExceeded,
    IdConflict,
    QueueSealed,
    QueueInactive,
    RhythmLost,
}
impl BatchReason {
    pub fn from_code(v: u64) -> Self {
        match v {
            1 => Self::DeadlineExpired,
            2 => Self::SourceChanged,
            3 => Self::PositionChanged,
            4 => Self::GridChanged,
            5 => Self::TempoChanged,
            6 => Self::ManualTakeover,
            7 => Self::ClockStopped,
            8 => Self::AudioStopped,
            9 => Self::ResourceConflict,
            10 => Self::PrerequisiteFailed,
            11 => Self::EndpointFailed,
            12 => Self::RecoveryUnavailable,
            13 => Self::CancelRequested,
            14 => Self::RevisionConflict,
            15 => Self::InvalidPatch,
            16 => Self::CapacityExceeded,
            17 => Self::IdConflict,
            18 => Self::QueueSealed,
            19 => Self::QueueInactive,
            20 => Self::RhythmLost,
            _ => Self::None,
        }
    }
}
pub use control_protocol::commands::CommandFailureCause;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandFailure {
    pub cause: CommandFailureCause,
    pub deck: Option<usize>,
    pub action_id: u64,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BatchEventReceipt {
    pub step: usize,
    pub action_id: u64,
    /// Render frame at which completion/cancellation was verified (zero while pending/in progress).
    pub completed_frame: u64,
    pub completed_clock_seconds: [f64; 2],
    pub completed_load_revisions: [u64; 2],
    pub completed_grid_revisions: [u64; 2],
    pub completed_position_revisions: [u64; 2],
    pub state: u64, // 0=pending, 1=applied, 2=verified, 3=cancelled, 4=failed
    pub frame: u64,
    pub clock_seconds: f64,
    pub failure: Option<CommandFailure>,
    pub resolved_clock: Option<usize>,
    pub resolved_clock_seconds: Option<f64>,
    pub resolved_frame: Option<u64>,
    /// Source-media seconds remaining at the resolved trigger, not wall time.
    pub clock_source_headroom_seconds: Option<f64>,
    /// Actual native ramp endpoint; absent until the ramp has started.
    pub ramp_end_frame: Option<u64>,
    /// An intermediate curve point verified its envelope and rendered tracking;
    /// the final segment still requires the ordinary settled endpoint check.
    pub curve_boundary_verified: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduledBatchSnapshot {
    pub command_queue: bool,
    pub recovery_applied: bool,
    pub revision: u64,
    pub open: bool,
    pub clock: usize,
    pub completion_carrier: usize,
    pub completion_event: i64,
    pub expires_frame: u64,
    pub updates: Vec<BatchPatchReceipt>,
    pub id: u64,
    pub state: BatchLifecycle,
    pub reason: BatchReason,
    pub completed: usize,
    pub total: usize,
    pub frame: u64,
    pub owned_targets: u32,
    pub owned_decks: u8,
    pub events: Vec<BatchEventReceipt>,
}
#[derive(Clone, Debug)]
pub struct ScheduledQueueAction {
    pub action_id: u64,
    pub step: ScheduledCommand,
}
#[derive(Clone, Debug)]
pub struct BatchQueuePatch {
    pub patch_id: u64,
    pub expected_revision: u64,
    pub append: Vec<ScheduledQueueAction>,
    pub invalidate: Vec<u64>,
    /// Seal at this loaded clock event; endpoints must form a complete handover.
    pub seal: Option<i64>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct BatchPatchReceipt {
    pub patch_id: u64,
    pub revision: u64,
    pub accepted: bool,
    pub reason: BatchReason,
    pub frame: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct CompiledStep {
    pub action_id: u64,
    pub at: f64,
    pub end: f64,
    pub deadline: f64,
    pub command: EngineCommand,
    pub require: Option<BatchPrerequisite>,
    pub clock: Option<usize>,
    pub at_frame: Option<u64>,
    pub dependency: Option<QueueTiming>,
    pub dependency_id: u64,
    pub completion_requirements: u64,
    /// Earlier segment of this artist-authored curve, not an arbitrary override.
    pub curve_predecessor: Option<usize>,
    pub grouped: bool,
    pub group_id: Option<u64>,
    pub strict_time: bool,
    pub guard_decks: u8,
    pub source_revisions: [u64; 2],
    pub grid_revisions: [u64; 2],
    pub position_revisions: [u64; 2],
    pub deck_control_revisions: [u64; 2],
    pub target_control_revisions: [u64; crate::COMMAND_TARGET_COUNT],
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct MaintainedAlignment {
    pub require: BatchPrerequisite,
    pub limits: crate::SyncRateLimits,
    pub members: u64,
}
#[derive(Debug)]
pub(crate) struct NativeBatch {
    pub command_queue: bool,
    pub id: u64,
    pub revision: u64,
    pub open: bool,
    pub completion_carrier: usize,
    pub completion_event: i64,
    pub expires_frame: u64,
    pub updates: [BatchPatchReceipt; MAX_QUEUE_UPDATES],
    pub update_count: usize,
    pub last_patch_id: u64,
    pub patch_identities: [Option<Box<str>>; MAX_QUEUE_UPDATES],
    pub clock: usize,
    pub sources: [u64; 2],
    pub positions: [u64; 2],
    pub grids: [u64; 2],
    pub rates: [f64; 2],
    pub arm_before: f64,
    pub complete_at: f64,
    pub sample_rate: u32,
    pub engine_epoch: u64,
    pub control_revision: u64,
    pub pause_outgoing: bool,
    pub len: usize,
    pub steps: [Option<CompiledStep>; MAX_SCHEDULED_STEPS],
    pub carrier: usize,
    pub recovery_seconds: f64,
    pub recovery: [Option<EngineCommand>; MAX_RECOVERY_CONTROLS],
    pub recovery_targets: u32,
    pub targets: u32,
    pub expected_endpoints: [Option<f32>; crate::CONTROL_TARGET_COUNT],
    pub launched: [bool; 2],
    pub authored_loops: u8,
    pub rhythm: [Option<MaintainedAlignment>; MAX_SCHEDULED_STEPS],
    pub state: BatchLifecycle,
    pub reason: BatchReason,
    pub completed: usize,
    pub frame: u64,
    pub recovery_end: u64,
    pub receipts: [BatchEventReceipt; MAX_SCHEDULED_STEPS],
}

pub fn command_targets(c: &EngineCommand) -> u32 {
    use EngineCommand::*;
    let bit = |t: ControlTarget| 1u32 << t.index();
    match c {
        Ramp { target, .. } => bit(*target),
        CancelRamps => (1 << crate::CONTROL_TARGET_COUNT) - 1,
        SetCrossfader { .. } => bit(ControlTarget::Crossfader),
        SetMasterGain { .. } => bit(ControlTarget::MasterGain),
        SetCueMix { .. } => bit(ControlTarget::CueMix),
        SetCueGain { .. } => bit(ControlTarget::CueGain),
        SetSamplerGain { .. } => bit(ControlTarget::SamplerGain),
        SetHeadphoneCue { deck, .. } => 1 << (crate::CONTROL_TARGET_COUNT + deck),
        SetChannelFader { deck, .. } => bit(ControlTarget::ChannelFader { deck: *deck }),
        SetDeckGain { deck, .. } => bit(ControlTarget::DeckGain { deck: *deck }),
        SetEq { deck, .. } => {
            bit(ControlTarget::EqLow { deck: *deck })
                | bit(ControlTarget::EqMid { deck: *deck })
                | bit(ControlTarget::EqHigh { deck: *deck })
        }
        SetFilter { deck, .. } => bit(ControlTarget::Filter { deck: *deck }),
        SetFx { deck, .. } => {
            bit(ControlTarget::Echo { deck: *deck }) | bit(ControlTarget::Reverb { deck: *deck })
        }
        SetTempo { deck, .. } | SetPlaybackBpm { deck, .. } => {
            bit(ControlTarget::TempoPercent { deck: *deck })
        }
        _ => 0,
    }
}
pub fn command_decks(command: &EngineCommand) -> u8 {
    let mut mask = command.deck().map_or(0, |deck| 1 << deck);
    match command {
        EngineCommand::PlayQuantized { clock, .. }
        | EngineCommand::Ramp {
            clock: Some(clock), ..
        } => mask |= 1 << clock,
        EngineCommand::Sync { .. }
        | EngineCommand::SetSync { enabled: true, .. }
        | EngineCommand::SetMaster { .. } => mask = 3,
        _ => {}
    }
    mask
}
/// Validate a parallel renderer patch without consulting live deck state.
/// Runtime prerequisites are preflighted together on the render thread.
pub fn validate_command_group(commands: &[EngineCommand]) -> Result<(), String> {
    if !(2..=MAX_SCHEDULED_STEPS).contains(&commands.len()) {
        return Err("A parallel group requires 2 to 64 renderer commands".into());
    }
    let mut occupied = 0;
    for command in commands {
        command.validate()?;
        if matches!(
            command,
            EngineCommand::Ramp {
                quantize: Some(_),
                ..
            }
        ) {
            return Err("Parallel ramp quantization belongs on the shared queue timing".into());
        }
        let targets = group_command_resources(command);
        if occupied & targets != 0 {
            return Err("Parallel commands have conflicting control or transport targets".into());
        }
        occupied |= targets;
    }
    Ok(())
}
fn group_command_resources(command: &EngineCommand) -> u32 {
    let targets = command_targets(command);
    // Absolute BPM reads the current source position. Transport or grid edits
    // on that deck must complete first, rather than depending on member order.
    if let EngineCommand::SetPlaybackBpm { deck, .. } = command {
        return targets | (1 << (crate::COMMAND_TARGET_COUNT + deck));
    }
    if targets == 0 {
        targets | ((command_decks(command) as u32) << crate::COMMAND_TARGET_COUNT)
    } else {
        targets
    }
}
pub(crate) fn active_ownership(b: &NativeBatch) -> (u8, u32) {
    if !b.command_queue || b.state == BatchLifecycle::Recovering {
        return (if b.state.active() { 3 } else { 0 }, b.targets);
    }
    let mut decks = 0;
    let mut targets = 0;
    for i in 0..b.len {
        if b.receipts[i].state <= 1 {
            let step = b.steps[i].as_ref().unwrap();
            decks |= step.guard_decks;
            targets |= command_targets(&step.command);
        }
    }
    if b.state.active() && b.recovery_targets != 0 {
        (decks | 3, targets | b.recovery_targets)
    } else {
        (decks, targets)
    }
}
pub(crate) fn endpoints(c: &EngineCommand, out: &mut [Option<f32>; crate::CONTROL_TARGET_COUNT]) {
    use EngineCommand::*;
    let mut set = |target: ControlTarget, value: f32| out[target.index()] = Some(value);
    match c {
        Ramp { target, value, .. } => set(*target, *value),
        SetCrossfader { position } => set(ControlTarget::Crossfader, *position),
        SetMasterGain { gain } => set(ControlTarget::MasterGain, *gain),
        SetCueMix { mix } => set(ControlTarget::CueMix, *mix),
        SetCueGain { gain } => set(ControlTarget::CueGain, *gain),
        SetSamplerGain { gain } => set(ControlTarget::SamplerGain, *gain),
        SetChannelFader { deck, value } => set(ControlTarget::ChannelFader { deck: *deck }, *value),
        SetDeckGain { deck, gain } => set(ControlTarget::DeckGain { deck: *deck }, *gain),
        SetEq {
            deck,
            low,
            mid,
            high,
        } => {
            set(ControlTarget::EqLow { deck: *deck }, *low);
            set(ControlTarget::EqMid { deck: *deck }, *mid);
            set(ControlTarget::EqHigh { deck: *deck }, *high);
        }
        SetFilter { deck, amount } => set(ControlTarget::Filter { deck: *deck }, *amount),
        SetFx { deck, echo, reverb } => {
            set(ControlTarget::Echo { deck: *deck }, *echo);
            set(ControlTarget::Reverb { deck: *deck }, *reverb);
        }
        SetTempo { deck, percent } => {
            set(ControlTarget::TempoPercent { deck: *deck }, *percent as f32)
        }
        _ => {}
    }
}
pub fn event_seconds(s: &EngineSnapshot, deck: usize, index: i64) -> Result<f64, String> {
    let d = s.decks.get(deck).ok_or("Invalid schedule deck")?;
    let anchor = if d.beatgrid_uniform {
        0
    } else {
        d.beatgrid_anchor_index.unwrap_or(0) as i64
    };
    if !d.beatgrid_uniform {
        let events = d
            .track
            .as_ref()
            .and_then(|t| t.neural_rhythm.as_ref())
            .ok_or("Schedule requires loaded beat events")?;
        if index < 0 || index as usize >= events.beats_seconds.len() {
            return Err("Schedule event index is outside loaded beat events".into());
        }
    }
    let seconds = d
        .grid_time((index - anchor) as f64)
        .ok_or("Schedule grid unavailable")?;
    let duration = d
        .track
        .as_ref()
        .ok_or("Schedule deck empty")?
        .duration_seconds;
    if !seconds.is_finite() || seconds < 0. || seconds >= duration {
        return Err("Schedule event lies outside loaded track".into());
    }
    Ok(seconds)
}
pub(crate) fn compile(
    id: u64,
    schedule: &BatchSchedule,
    commands: Vec<ScheduledCommand>,
    s: &EngineSnapshot,
) -> Result<Box<NativeBatch>, String> {
    if schedule.operation_id.is_empty()
        || schedule.operation_id.len() > 128
        || schedule.intent.to_string().len() > 16384
    {
        return Err("Invalid schedule operation ID or intent size".into());
    }
    if (!schedule.open && commands.is_empty())
        || commands.len() > MAX_SCHEDULED_STEPS
        || schedule.clock > 1
        || schedule.recovery.carrier > 1
    {
        return Err("Scheduled batch needs 1..64 steps and decks 0 or 1".into());
    }
    if schedule.expected_engine_epoch != s.engine_epoch {
        return Err("Engine epoch conflict".into());
    }
    if schedule.expected_control_revision != s.control_revision {
        return Err("Control revision conflict".into());
    }
    if schedule.expected_load_revisions != s.load_revisions {
        return Err("Loaded source revision conflict".into());
    }
    if schedule.expected_position_revisions != s.decks.each_ref().map(|d| d.position_revision) {
        return Err("Position revision conflict".into());
    }
    if schedule.expected_grid_revisions != s.decks.each_ref().map(|d| d.grid_revision) {
        return Err("Grid revision conflict".into());
    }
    if !s.running || !s.decks[schedule.clock].playing {
        return Err("Schedule requires running audio and playing clock".into());
    }
    if s.scheduled_batch.state.active()
        || s.active_ramps.iter().any(|r| *r)
        || s.decks.iter().any(|d| {
            d.launch_pending
                || d.jog_touch
                || d.loop_start_seconds.is_some()
                || d.preparation_pending
        })
    {
        return Err(
            "Schedule resource conflict; settle launches, ramps, jog holds, loops and preparation"
                .into(),
        );
    }
    let arm_before = event_seconds(s, schedule.clock, schedule.arm_before_event)?;
    let complete_at = event_seconds(s, schedule.clock, schedule.completion_event)?;
    if complete_at <= arm_before {
        return Err("Queue completion/expiry must follow arming deadline".into());
    }
    if arm_before <= s.decks[schedule.clock].position_seconds {
        return Err("Schedule arming deadline expired".into());
    }
    if !schedule.recovery.seconds.is_finite()
        || !(0.02..=5.).contains(&schedule.recovery.seconds)
        || schedule.recovery.controls.is_empty()
        || schedule.recovery.controls.len() > MAX_RECOVERY_CONTROLS
    {
        return Err("Recovery needs 1..8 controls and duration 0.02..5 seconds".into());
    }
    if schedule.completion_carrier.is_some_and(|d| d > 1)
        || schedule.open
            && !schedule
                .expiry_seconds
                .is_some_and(|n| n.is_finite() && (0.1..=600.).contains(&n))
    {
        return Err(
            "Open queue requires expiry_seconds within 0.1..600 and a valid completion carrier"
                .into(),
        );
    }
    let mut b = Box::new(NativeBatch {
        id,
        command_queue: false,
        revision: 1,
        open: schedule.open,
        completion_carrier: schedule
            .completion_carrier
            .unwrap_or(schedule.recovery.carrier),
        completion_event: schedule.completion_event,
        expires_frame: if schedule.open {
            s.rendered_frames
                + (schedule.expiry_seconds.unwrap() * s.sample_rate as f64).ceil() as u64
        } else {
            u64::MAX
        },
        updates: [BatchPatchReceipt::default(); MAX_QUEUE_UPDATES],
        update_count: 0,
        last_patch_id: 0,
        patch_identities: std::array::from_fn(|_| None),
        clock: schedule.clock,
        sources: s.load_revisions,
        positions: s.decks.each_ref().map(|d| d.position_revision),
        grids: s.decks.each_ref().map(|d| d.grid_revision),
        rates: s.decks.each_ref().map(|d| d.playback_rate),
        arm_before,
        complete_at,
        sample_rate: s.sample_rate,
        engine_epoch: s.engine_epoch,
        control_revision: s.control_revision,
        pause_outgoing: schedule.pause_outgoing,
        len: commands.len(),
        steps: std::array::from_fn(|_| None),
        carrier: schedule.recovery.carrier,
        recovery_seconds: schedule.recovery.seconds,
        recovery: std::array::from_fn(|_| None),
        recovery_targets: 0,
        targets: 0,
        expected_endpoints: [None; crate::CONTROL_TARGET_COUNT],
        launched: [false; 2],
        authored_loops: 0,
        rhythm: [None; MAX_SCHEDULED_STEPS],
        state: BatchLifecycle::Armed,
        reason: BatchReason::None,
        completed: 0,
        frame: s.rendered_frames,
        recovery_end: 0,
        receipts: [BatchEventReceipt::default(); MAX_SCHEDULED_STEPS],
    });
    let a = compile_actions(
        commands,
        s,
        b.clock,
        arm_before,
        complete_at,
        b.completion_carrier,
    )?;
    b.steps = a.steps;
    b.targets = a.targets;
    b.expected_endpoints = a.endpoints;
    b.launched = a.launched;
    for i in 0..b.len {
        b.receipts[i].step = i;
        b.receipts[i].action_id = i as u64 + 1;
    }
    let fader = ControlTarget::ChannelFader {
        deck: b.completion_carrier,
    }
    .index();
    let other_fader = ControlTarget::ChannelFader {
        deck: 1 - b.completion_carrier,
    }
    .index();
    if !b.open
        && (b.expected_endpoints[fader] != Some(1.)
            || b.expected_endpoints[other_fader] != Some(0.))
    {
        return Err(
            "Handover final endpoints must fully open the carrier and isolate the other deck"
                .into(),
        );
    }
    if !b.open {
        let d = &s.decks[b.completion_carrier];
        for (target, current, desired) in [
            (
                ControlTarget::EqLow {
                    deck: b.completion_carrier,
                },
                d.eq_low,
                1.,
            ),
            (
                ControlTarget::EqMid {
                    deck: b.completion_carrier,
                },
                d.eq_mid,
                1.,
            ),
            (
                ControlTarget::EqHigh {
                    deck: b.completion_carrier,
                },
                d.eq_high,
                1.,
            ),
            (
                ControlTarget::Filter {
                    deck: b.completion_carrier,
                },
                d.filter,
                0.,
            ),
            (
                ControlTarget::Echo {
                    deck: b.completion_carrier,
                },
                d.echo,
                0.,
            ),
            (
                ControlTarget::Reverb {
                    deck: b.completion_carrier,
                },
                d.reverb,
                0.,
            ),
        ] {
            if (b.expected_endpoints[target.index()].unwrap_or(current) - desired).abs() > 0.001 {
                return Err("Handover must restore incoming EQ to unity, filter to neutral and effect sends to zero".into());
            }
            b.expected_endpoints[target.index()] = Some(desired);
        }
    }
    let mut recovery_endpoints = [None; crate::CONTROL_TARGET_COUNT];
    for (i, c) in schedule.recovery.controls.iter().enumerate() {
        c.validate()?;
        if command_targets(c) == 0
            || command_targets(c) & 31 != 0
            || c.deck().is_some_and(|d| {
                d != schedule.recovery.carrier && d != 1 - schedule.recovery.carrier
            })
        {
            return Err("Recovery supports deck fader/EQ/filter/gain/FX controls only".into());
        }
        if let EngineCommand::Ramp {
            clock,
            beats,
            quantize,
            seconds,
            ..
        } = c
        {
            if clock.is_some()
                || beats.is_some()
                || quantize.is_some()
                || *seconds > schedule.recovery.seconds
            {
                return Err("Recovery ramps use renderer seconds within recovery duration".into());
            }
        }
        endpoints(c, &mut recovery_endpoints);
        b.targets |= command_targets(c);
        b.recovery_targets |= command_targets(c);
        b.recovery[i] = Some(c.clone());
    }
    let fader = ControlTarget::ChannelFader { deck: b.carrier }.index();
    let other_fader = ControlTarget::ChannelFader {
        deck: 1 - b.carrier,
    }
    .index();
    if recovery_endpoints[fader] != Some(1.) || recovery_endpoints[other_fader] != Some(0.) {
        return Err(
            "Recovery must fully open its healthy carrier and isolate the other deck".into(),
        );
    }
    for (target, current) in [
        (
            ControlTarget::EqLow { deck: b.carrier },
            s.decks[b.carrier].eq_low,
        ),
        (
            ControlTarget::EqMid { deck: b.carrier },
            s.decks[b.carrier].eq_mid,
        ),
        (
            ControlTarget::EqHigh { deck: b.carrier },
            s.decks[b.carrier].eq_high,
        ),
    ] {
        if ((current - 1.).abs() > 0.001 || b.targets & (1 << target.index()) != 0)
            && recovery_endpoints[target.index()] != Some(1.)
        {
            return Err("Recovery must settle every affected carrier EQ band to full tone".into());
        }
    }
    if b.open && a.targets & !b.recovery_targets != 0 {
        return Err("Open queue actions must have predeclared recovery coverage".into());
    }
    Ok(b)
}

struct CompiledActions {
    steps: [Option<CompiledStep>; MAX_SCHEDULED_STEPS],
    len: usize,
    targets: u32,
    endpoints: [Option<f32>; crate::CONTROL_TARGET_COUNT],
    launched: [bool; 2],
}
fn compile_actions(
    commands: Vec<ScheduledCommand>,
    s: &EngineSnapshot,
    queue_clock: usize,
    min_at: f64,
    complete_at: f64,
    carrier: usize,
) -> Result<CompiledActions, String> {
    let mut last = min_at;
    let mut a = CompiledActions {
        steps: std::array::from_fn(|_| None),
        len: commands.len(),
        targets: 0,
        endpoints: [None; crate::CONTROL_TARGET_COUNT],
        launched: [false; 2],
    };
    let mut occupied = [f64::NEG_INFINITY; crate::CONTROL_TARGET_COUNT];
    for (i, step) in commands.into_iter().enumerate() {
        step.command.validate()?;
        let at = event_seconds(s, queue_clock, step.at_event)?;
        let deadline = event_seconds(s, queue_clock, step.deadline_event)?;
        if at < last || at < min_at || deadline < at || complete_at <= deadline {
            return Err("Schedule event order/deadline/completion invalid".into());
        }
        let mut end = at;
        match &step.command {
            EngineCommand::Ramp{target,seconds,clock,beats,quantize,..}=>{
                if target.deck().is_none() || quantize.is_some() || clock.is_some_and(|c|c!=queue_clock) {return Err("Scheduled ramps require deck targets, declared clock and explicit event (no quantize)".into());}
                let duration=if let Some(beats)=beats {let d=&s.decks[queue_clock]; let pos=d.grid_position(at).ok_or("Grid unavailable")?;(d.grid_time(pos+beats).ok_or("Grid unavailable")?-at)/d.playback_rate} else {*seconds};
                end=at+duration*s.decks[queue_clock].playback_rate;
            },
            EngineCommand::PlayQuantized{deck,clock,clock_revision,..}=>{
                if *clock!=queue_clock || *clock_revision!=s.load_revisions[*clock] || s.decks[*deck].playing || !s.decks[*deck].beatgrid_valid || !s.decks[*clock].beatgrid_valid {return Err("Invalid scheduled launch guards/readiness".into());}
                let d=&s.decks[*deck];
                if (d.snapped_grid_time(d.position_seconds).ok_or("Incoming event unavailable")?-d.position_seconds).abs()>0.001 {return Err("Seek the paused incoming deck to its selected effective beat event before arming".into());}
                a.launched[*deck]=true;
            },
            EngineCommand::SetMaster{deck} if *deck == queue_clock => {},
            EngineCommand::SetSync{deck,enabled:true} if *deck != queue_clock && s.decks[*deck].beatgrid_valid && s.decks[queue_clock].beatgrid_valid => {},
            EngineCommand::SetSyncRateLimits{deck,..} if *deck != queue_clock => {},
            EngineCommand::SetChannelFader{..}|EngineCommand::SetDeckGain{..}|EngineCommand::SetEq{..}|EngineCommand::SetFilter{..}|EngineCommand::SetFx{..}=>{},
            _=>return Err("Command unsupported inside a scheduled batch; prepare sources/tempo/grid beforehand".into()),
        }
        if end >= complete_at {
            return Err(
                "Scheduled phase needs endpoint verification margin before completion".into(),
            );
        }
        let targets = command_targets(&step.command);
        for (j, until) in occupied.iter_mut().enumerate() {
            if targets & (1 << j) != 0 {
                if at + 1e-8 < *until {
                    return Err("Overlapping scheduled control segments".into());
                }
                *until = end;
            }
        }
        a.targets |= targets;
        endpoints(&step.command, &mut a.endpoints);
        if matches!(
            &step.command,
            EngineCommand::SetChannelFader { .. }
                | EngineCommand::Ramp {
                    target: ControlTarget::ChannelFader { .. },
                    ..
                }
        ) && !step.require.is_some_and(|r| {
            r.deck == carrier
                && r.playing == Some(true)
                && r.ready
                && r.advanced_from_seconds.is_some()
                && r.min_advanced_beats > 0.
        }) {
            return Err(
                "Audible fader phases require a ready, playing carrier with verified advancement"
                    .into(),
            );
        }
        if let Some(r) = step.require {
            if r.deck > 1
                || !r.min_advanced_beats.is_finite()
                || r.min_advanced_beats < 0.
                || r.phase_tolerance_beats
                    .is_some_and(|n| !n.is_finite() || !(0.001..=0.5).contains(&n))
                || r.bar_beats.is_some_and(|n| !(2..=16).contains(&n))
                || r.advanced_from_seconds
                    .is_some_and(|n| !n.is_finite() || n < 0.)
            {
                return Err("Invalid phase prerequisite".into());
            }
            if r.bar_beats.is_some()
                && (!s.decks[r.deck].beatgrid_downbeat || !s.decks[1 - r.deck].beatgrid_downbeat)
            {
                return Err(
                    "Bar-phase prerequisite requires credible origins on both grids".into(),
                );
            }
            if let Some(n) = r.arrival_before_event {
                event_seconds(s, r.deck, n)?;
            }
        }
        last = at;
        a.steps[i] = Some(CompiledStep {
            action_id: i as u64 + 1,
            clock: Some(queue_clock),
            at_frame: None,
            dependency: None,
            dependency_id: 0,
            completion_requirements: 0,
            curve_predecessor: None,
            group_id: None,
            grouped: true,
            strict_time: true,
            guard_decks: 3,
            source_revisions: s.load_revisions,
            grid_revisions: s.decks.each_ref().map(|d| d.grid_revision),
            position_revisions: s.decks.each_ref().map(|d| d.position_revision),
            deck_control_revisions: s.deck_control_revisions,
            target_control_revisions: s.target_control_revisions,
            at,
            end,
            deadline,
            command: step.command,
            require: step.require,
        });
    }
    Ok(a)
}

#[derive(Debug)]
pub(crate) struct CompiledQueuePatch {
    pub identity: Option<Box<str>>,
    pub id: u64,
    pub patch_id: u64,
    pub expected_revision: u64,
    pub engine_epoch: u64,
    pub control_revision: u64,
    pub sources: [u64; 2],
    pub positions: [u64; 2],
    pub grids: [u64; 2],
    pub sample_rate: u32,
    pub len: usize,
    pub steps: [Option<CompiledStep>; MAX_SCHEDULED_STEPS],
    pub invalidate: [u64; MAX_SCHEDULED_STEPS],
    pub invalidate_len: usize,
    pub rebind_dependencies: [(u64, u64); MAX_SCHEDULED_STEPS],
    pub rebind_len: usize,
    pub seal: Option<(i64, f64)>,
}
pub(crate) fn compile_patch(
    id: u64,
    patch: BatchQueuePatch,
    s: &EngineSnapshot,
) -> Result<Box<CompiledQueuePatch>, String> {
    let b = &s.scheduled_batch;
    if b.id != id || !matches!(b.state, BatchLifecycle::Armed | BatchLifecycle::Running) {
        return Err("Queue is not armed/running".into());
    }
    if !b.open {
        return Err("Queue is sealed".into());
    }
    if b.revision != patch.expected_revision {
        return Err("Queue revision conflict".into());
    }
    if patch.patch_id == 0
        || patch.append.len() + b.total > MAX_SCHEDULED_STEPS
        || patch.invalidate.len() > MAX_SCHEDULED_STEPS
        || patch.append.is_empty() && patch.invalidate.is_empty() && patch.seal.is_none()
    {
        return Err("Invalid queue patch or lifetime action capacity exceeded".into());
    }
    let identity = Some(format!("{patch:?}").into_boxed_str());
    let mut last_id = b.events.iter().map(|r| r.action_id).max().unwrap_or(0);
    let mut commands = Vec::with_capacity(patch.append.len());
    let mut ids = Vec::with_capacity(patch.append.len());
    for action in patch.append {
        if action.action_id <= last_id {
            return Err("Queue action IDs must increase and must never be reused".into());
        }
        last_id = action.action_id;
        ids.push(action.action_id);
        commands.push(action.step);
    }
    let seal = patch
        .seal
        .map(|event| event_seconds(s, b.clock, event).map(|at| (event, at)))
        .transpose()?;
    let complete = event_seconds(s, b.clock, b.completion_event)?;
    if seal.is_some_and(|(_, at)| at <= s.decks[b.clock].position_seconds || at > complete) {
        return Err("Seal completion must be future and within declared queue expiry".into());
    }
    let mut a = compile_actions(
        commands,
        s,
        b.clock,
        s.decks[b.clock].position_seconds,
        seal.map_or(complete, |(_, at)| at),
        b.completion_carrier,
    )?;
    for (step, id) in a.steps.iter_mut().flatten().zip(ids) {
        step.action_id = id;
    }
    let mut invalid = [0; MAX_SCHEDULED_STEPS];
    for (i, id) in patch.invalidate.iter().copied().enumerate() {
        if invalid[..i].contains(&id) || !b.events.iter().any(|r| r.action_id == id && r.state == 0)
        {
            return Err("Only distinct pending queue action IDs may be invalidated".into());
        }
        invalid[i] = id;
    }
    Ok(Box::new(CompiledQueuePatch {
        identity,
        id,
        patch_id: patch.patch_id,
        expected_revision: patch.expected_revision,
        engine_epoch: s.engine_epoch,
        control_revision: s.control_revision,
        sources: s.load_revisions,
        positions: s.decks.each_ref().map(|d| d.position_revision),
        grids: s.decks.each_ref().map(|d| d.grid_revision),
        sample_rate: s.sample_rate,
        len: a.len,
        steps: a.steps,
        invalidate: invalid,
        invalidate_len: patch.invalidate.len(),
        rebind_dependencies: [(0, 0); MAX_SCHEDULED_STEPS],
        rebind_len: 0,
        seal,
    }))
}

/// Mechanical command timing. Relative offsets are resolved from one native
/// snapshot, or from an earlier action's actual verified completion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum QueueTiming {
    #[default]
    Now,
    AtEvent {
        clock: usize,
        event: i64,
    },
    AtSeconds {
        clock: usize,
        seconds: f64,
    },
    AfterBeats {
        clock: usize,
        beats: f64,
        #[serde(default)]
        quantize: Option<f64>,
    },
    AfterSeconds {
        seconds: f64,
    },
    AfterAction {
        action_id: u64,
        #[serde(default)]
        seconds: Option<f64>,
        #[serde(default)]
        clock: Option<usize>,
        #[serde(default)]
        beats: Option<f64>,
        #[serde(default)]
        quantize: Option<f64>,
    },
}
#[derive(Clone, Debug)]
pub struct QueuedCommand {
    pub action_id: u64,
    pub command: EngineCommand,
    pub when: QueueTiming,
    /// Explicit atomic parallel group, complete within one patch.
    pub group_id: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct CommandQueuePatch {
    /// Atomically retarget retained pending dependencies during replacement.
    pub rebind_dependencies: Vec<(u64, u64)>,
    pub patch_id: u64,
    pub expected_revision: u64,
    pub append: Vec<QueuedCommand>,
    pub invalidate: Vec<u64>,
}
/// Every EngineCommand is a bounded, typed renderer primitive. Provider, audio
/// device, library and persistence services remain in the shared Control owner.
pub fn command_queue_supported(_: &EngineCommand) -> bool {
    true
}
pub(crate) fn compile_command_queue(
    id: u64,
    s: &EngineSnapshot,
) -> Result<Box<NativeBatch>, String> {
    if !s.running {
        return Err("Command queue requires a running audio renderer".into());
    }
    if s.scheduled_batch.state.active()
        && !(s.scheduled_batch.command_queue
            && s.scheduled_batch.events.iter().all(|r| r.state >= 2))
        || s.decks.iter().any(|d| d.launch_pending)
    {
        return Err("Another native operation is still active".into());
    }
    Ok(Box::new(NativeBatch {
        id,
        command_queue: true,
        revision: 1,
        open: true,
        completion_carrier: 0,
        completion_event: i64::MAX,
        expires_frame: u64::MAX,
        updates: [BatchPatchReceipt::default(); MAX_QUEUE_UPDATES],
        update_count: 0,
        last_patch_id: 0,
        patch_identities: std::array::from_fn(|_| None),
        clock: 0,
        sources: s.load_revisions,
        positions: s.decks.each_ref().map(|d| d.position_revision),
        grids: s.decks.each_ref().map(|d| d.grid_revision),
        rates: s.decks.each_ref().map(|d| d.playback_rate),
        arm_before: f64::INFINITY,
        complete_at: f64::INFINITY,
        sample_rate: s.sample_rate,
        engine_epoch: s.engine_epoch,
        control_revision: s.control_revision,
        pause_outgoing: false,
        len: 0,
        steps: std::array::from_fn(|_| None),
        carrier: 0,
        recovery_seconds: 0.,
        recovery: std::array::from_fn(|_| None),
        recovery_targets: 0,
        targets: 0,
        expected_endpoints: [None; crate::CONTROL_TARGET_COUNT],
        launched: [false; 2],
        authored_loops: 0,
        rhythm: [None; MAX_SCHEDULED_STEPS],
        state: BatchLifecycle::Armed,
        reason: BatchReason::None,
        completed: 0,
        frame: s.rendered_frames,
        recovery_end: 0,
        receipts: [BatchEventReceipt::default(); MAX_SCHEDULED_STEPS],
    }))
}
pub fn validate_queue_timing(when: QueueTiming) -> Result<(), String> {
    let clock_valid = |clock: usize| {
        if clock <= 1 {
            Ok(())
        } else {
            Err(format!(
                "Timing clock must be deck 0/A or 1/B; received {clock}"
            ))
        }
    };
    let beats_valid = |beats: f64| {
        if beats.is_finite() && (0.0..=256.).contains(&beats) {
            Ok(())
        } else {
            Err(format!(
                "Timing beats must be finite and between 0 and 256; received {beats}"
            ))
        }
    };
    let seconds_valid = |seconds: f64| {
        if seconds.is_finite() && (0.0..=3600.).contains(&seconds) {
            Ok(())
        } else {
            Err(format!(
                "Relative timing seconds must be finite and between 0 and 3600; received {seconds}"
            ))
        }
    };
    let quantize_valid = |quantize: Option<f64>| {
        if quantize.is_none_or(|q| q.is_finite() && q > 0. && q <= 256.) {
            Ok(())
        } else {
            Err("Timing quantize must be finite, greater than 0 and at most 256 beats".to_string())
        }
    };
    match when {
        QueueTiming::Now => {}
        QueueTiming::AtEvent { clock, .. } => clock_valid(clock)?,
        QueueTiming::AtSeconds { clock, seconds } => {
            clock_valid(clock)?;
            if !seconds.is_finite() || seconds < 0. {
                return Err(format!(
                    "Absolute source seconds must be finite and nonnegative; received {seconds}"
                ));
            }
        }
        QueueTiming::AfterBeats {
            clock,
            beats,
            quantize,
        } => {
            clock_valid(clock)?;
            beats_valid(beats)?;
            quantize_valid(quantize)?;
        }
        QueueTiming::AfterSeconds { seconds } => seconds_valid(seconds)?,
        QueueTiming::AfterAction {
            action_id,
            seconds,
            clock,
            beats,
            quantize,
        } => {
            if action_id == 0 {
                return Err("Completion dependency action_id must be nonzero".into());
            }
            if seconds.is_some() && beats.is_some() {
                return Err("Completion timing accepts seconds or beats, not both".into());
            }
            if let Some(seconds) = seconds {
                seconds_valid(seconds)?;
            }
            if let Some(clock) = clock {
                clock_valid(clock)?;
            }
            if let Some(beats) = beats {
                beats_valid(beats)?;
                if clock.is_none() {
                    return Err("Completion beat timing requires a clock deck".into());
                }
            }
            if quantize.is_some() && beats.is_none() {
                return Err("Completion quantize requires a beats offset".into());
            }
            quantize_valid(quantize)?;
        }
    }
    Ok(())
}
pub(crate) fn compile_command_patch(
    id: u64,
    p: CommandQueuePatch,
    s: &EngineSnapshot,
) -> Result<Box<CompiledQueuePatch>, String> {
    let b = &s.scheduled_batch;
    if b.id != id
        || !b.command_queue
        || !matches!(
            b.state,
            BatchLifecycle::Armed | BatchLifecycle::Running | BatchLifecycle::Completed
        )
    {
        return Err("Command queue is not available".into());
    }
    if b.revision != p.expected_revision {
        return Err("Queue revision conflict".into());
    }
    if p.patch_id == 0
        || p.append.len() + b.total > MAX_SCHEDULED_STEPS
        || p.invalidate.len() > MAX_SCHEDULED_STEPS
        || p.rebind_dependencies.len() > MAX_SCHEDULED_STEPS
        || p.append.is_empty() && p.invalidate.is_empty()
    {
        return Err("Invalid command queue patch or lifetime capacity exceeded".into());
    }
    for (i, &(old, new)) in p.rebind_dependencies.iter().enumerate() {
        if old == new
            || !p.invalidate.contains(&old)
            || !b.events.iter().any(|r| r.action_id == old && r.state == 0)
            || !p.append.iter().any(|item| item.action_id == new)
            || p.rebind_dependencies[..i]
                .iter()
                .any(|&(prior, _)| prior == old)
        {
            return Err(
                "Dependency replacement requires distinct pending removed and appended action IDs"
                    .into(),
            );
        }
    }
    for (i, item) in p.append.iter().enumerate() {
        let Some(group_id) = item.group_id else {
            continue;
        };
        if group_id == 0 {
            return Err("Parallel group ID must be nonzero".into());
        }
        if p.append[..i]
            .iter()
            .any(|other| other.group_id == Some(group_id))
        {
            continue;
        }
        let members: Vec<_> = p
            .append
            .iter()
            .filter(|other| other.group_id == Some(group_id))
            .collect();
        if members.iter().any(|other| other.when != item.when) {
            return Err("Parallel commands must share one timing trigger".into());
        }
        validate_command_group(
            &members
                .iter()
                .map(|member| member.command.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    let identity = Some(format!("{p:?}").into_boxed_str());
    let mut steps: [Option<CompiledStep>; MAX_SCHEDULED_STEPS] = std::array::from_fn(|_| None);
    let mut available: Vec<u64> = b
        .events
        .iter()
        .filter(|r| r.state != 3 && r.state != 4)
        .map(|r| r.action_id)
        .collect();
    for (i, item) in p.append.iter().enumerate() {
        // Readiness can submit an earlier registered item after a later one.
        // Every lifetime receipt remains retained until this bounded native
        // segment rotates, including cancelled/failed receipts.
        if item.action_id == 0
            || b.events
                .iter()
                .any(|receipt| receipt.action_id == item.action_id)
            || p.append[..i]
                .iter()
                .any(|prior| prior.action_id == item.action_id)
        {
            return Err(
                "Queue action IDs must be nonzero and unique for the native segment lifetime"
                    .into(),
            );
        }
        item.command.validate()?;
        validate_queue_timing(item.when)?;
        if let QueueTiming::AfterAction {
            action_id,
            clock: Some(clock),
            beats: Some(_),
            ..
        } = item.when
        {
            if b.events
                .iter()
                .find(|r| r.action_id == action_id && r.state == 2)
                .is_some_and(|r| {
                    r.completed_load_revisions[clock] != s.load_revisions[clock]
                        || r.completed_grid_revisions[clock] != s.decks[clock].grid_revision
                        || r.completed_position_revisions[clock] != s.decks[clock].position_revision
                })
            {
                return Err("Completion dependency clock source/grid/position changed".into());
            }
        }
        if let QueueTiming::AfterAction { action_id, .. } = item.when {
            if !available.contains(&action_id) || p.invalidate.contains(&action_id) {
                return Err("Action dependency is unavailable or cancelled".into());
            }
        }
        available.push(item.action_id);
        let mut step = CompiledStep {
            action_id: item.action_id,
            at: 0.,
            end: 0.,
            deadline: f64::INFINITY,
            clock: None,
            at_frame: None,
            dependency: None,
            dependency_id: 0,
            completion_requirements: 0,
            curve_predecessor: None,
            group_id: item.group_id,
            grouped: !matches!(
                item.when,
                QueueTiming::Now | QueueTiming::AfterSeconds { seconds: 0. }
            ),
            strict_time: !matches!(
                item.when,
                QueueTiming::Now
                    | QueueTiming::AfterSeconds { seconds: 0. }
                    | QueueTiming::AfterAction {
                        seconds: None,
                        beats: None,
                        ..
                    }
                    | QueueTiming::AfterAction {
                        seconds: Some(0.),
                        ..
                    }
                    | QueueTiming::AfterAction {
                        beats: Some(0.),
                        quantize: None,
                        ..
                    }
                    | QueueTiming::AfterBeats {
                        beats: 0.,
                        quantize: None,
                        ..
                    }
            ),
            guard_decks: command_decks(&item.command),
            source_revisions: s.load_revisions,
            grid_revisions: s.decks.each_ref().map(|d| d.grid_revision),
            position_revisions: s.decks.each_ref().map(|d| d.position_revision),
            deck_control_revisions: s.deck_control_revisions,
            target_control_revisions: s.target_control_revisions,
            command: item.command.clone(),
            require: None,
        };
        match item.when {
            QueueTiming::Now => step.at_frame = Some(s.rendered_frames),
            QueueTiming::AfterSeconds { seconds } => {
                step.at_frame =
                    Some(s.rendered_frames + (seconds * s.sample_rate as f64).ceil() as u64)
            }
            QueueTiming::AtSeconds { clock, seconds } => {
                step.clock = Some(clock);
                step.at = seconds;
            }
            QueueTiming::AtEvent { clock, event } => {
                step.clock = Some(clock);
                step.at = event_seconds(s, clock, event)?;
            }
            QueueTiming::AfterBeats {
                clock,
                beats,
                quantize,
            } => {
                let d = &s.decks[clock];
                let mut at = d
                    .grid_position(d.position_seconds)
                    .ok_or("Clock grid unavailable")?
                    + beats;
                if let Some(q) = quantize {
                    at = (at / q).ceil() * q;
                }
                if beats == 0. && quantize.is_none() {
                    // Zero unquantized offsets mean readiness, rather than an
                    // absolute deadline already behind the control transport.
                    step.at_frame = Some(s.rendered_frames);
                    step.guard_decks |= 1 << clock;
                } else {
                    step.clock = Some(clock);
                    step.at = d.grid_time(at).ok_or("Clock grid unavailable")?;
                }
            }
            QueueTiming::AfterAction { action_id, .. } => {
                step.dependency = Some(item.when);
                step.dependency_id = action_id;
            }
        }
        if let Some(clock) = step.clock {
            let launch = match &step.command {
                EngineCommand::Play { deck } if *deck != clock => {
                    Some((*deck, s.load_revisions[clock]))
                }
                EngineCommand::PlayQuantized {
                    deck,
                    clock: launch_clock,
                    clock_revision,
                    ..
                } => {
                    if *launch_clock != clock {
                        return Err(
                            "Quantized launch clock must match its queue timing clock".into()
                        );
                    }
                    Some((*deck, *clock_revision))
                }
                _ => None,
            };
            if let Some((deck, clock_revision)) = launch {
                // This is the existing quantized launch primitive. Snap the
                // clock boundary for explicit launches too: otherwise an
                // off-beat outer trigger is accepted now and silently rejected
                // by the renderer only when the outgoing passage is ending.
                let d = &s.decks[clock];
                let beat = launch_beat(d.grid_position(step.at).ok_or("Clock grid unavailable")?);
                step.at = d.grid_time(beat).ok_or("Clock grid unavailable")?;
                step.command = EngineCommand::PlayQuantized {
                    deck,
                    clock,
                    clock_revision,
                    at_seconds: Some(step.at),
                    quantize: 1.,
                };
            }
        }
        if step.clock.is_some_and(|clock| {
            s.decks[clock]
                .track
                .as_ref()
                .is_some_and(|t| step.at >= t.duration_seconds)
        }) {
            return Err("Command source timestamp lies beyond its track".into());
        }
        if step
            .clock
            .is_some_and(|clock| step.at <= s.decks[clock].position_seconds)
        {
            return Err("Command event is not in the future".into());
        }
        step.guard_decks |= step.clock.map_or(0, |clock| 1 << clock);
        if let Some(QueueTiming::AfterAction {
            clock: Some(clock),
            beats: Some(_),
            ..
        }) = step.dependency
        {
            step.guard_decks |= 1 << clock;
        }
        step.end = step.at;
        steps[i] = Some(step);
    }
    // Quantized launches may snap their boundary to the next beat. Every
    // explicit member shares that resolved trigger, including control ramps.
    for i in 0..p.append.len() {
        let Some(group_id) = steps[i].as_ref().unwrap().group_id else {
            continue;
        };
        let anchor = steps[..p.append.len()]
            .iter()
            .flatten()
            .filter(|step| step.group_id == Some(group_id))
            .max_by(|a, b| a.at.total_cmp(&b.at))
            .unwrap();
        let (clock, at, at_frame, dependency) =
            (anchor.clock, anchor.at, anchor.at_frame, anchor.dependency);
        let step = steps[i].as_mut().unwrap();
        step.clock = clock;
        step.at = at;
        step.at_frame = at_frame;
        step.dependency = dependency;
    }
    let mut invalidate = [0; MAX_SCHEDULED_STEPS];
    for (i, action) in p.invalidate.iter().copied().enumerate() {
        if invalidate[..i].contains(&action)
            || !b
                .events
                .iter()
                .any(|r| r.action_id == action && r.state == 0)
        {
            return Err("Only pending command queue actions can be removed".into());
        }
        invalidate[i] = action;
    }
    let mut rebind_dependencies = [(0, 0); MAX_SCHEDULED_STEPS];
    rebind_dependencies[..p.rebind_dependencies.len()].copy_from_slice(&p.rebind_dependencies);
    Ok(Box::new(CompiledQueuePatch {
        identity,
        id,
        patch_id: p.patch_id,
        expected_revision: p.expected_revision,
        engine_epoch: s.engine_epoch,
        control_revision: s.control_revision,
        sources: s.load_revisions,
        positions: s.decks.each_ref().map(|d| d.position_revision),
        grids: s.decks.each_ref().map(|d| d.grid_revision),
        sample_rate: s.sample_rate,
        len: p.append.len(),
        steps,
        invalidate,
        invalidate_len: p.invalidate.len(),
        rebind_dependencies,
        rebind_len: p.rebind_dependencies.len(),
        seal: None,
    }))
}

// An exact event can round-trip a few ulps above its integer beat. Avoid
// moving that event an entire beat later, while genuinely off-beat targets
// still resolve forward. Shared by snapshot compilation and native dependencies.
pub(crate) fn launch_beat(position: f64) -> f64 {
    let nearest = position.round();
    if (position - nearest).abs() <= 1e-9 {
        nearest
    } else {
        position.ceil()
    }
}

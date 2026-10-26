//! Artist-authored performance contract shared by CLI, MCP and the native owner.
use crate::commands::{ControlTarget, EngineCommand, SyncRateLimits};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub engine_epoch: u64,
    pub load_revisions: [u64; 2],
    pub grid_revisions: [u64; 2],
    pub position_revisions: [u64; 2],
    pub control_revision: u64,
    pub tempo_percent: [f64; 2],
    pub observed_rendered_frames: u64,
    pub sample_rate: u32,
    #[serde(default)]
    pub sync_enabled: [bool; 2],
    #[serde(default, deserialize_with = "crate::commands::deserialize_deck")]
    #[schemars(schema_with = "crate::commands::deck_schema")]
    pub master_deck: usize,
    #[serde(default)]
    pub sync_rate_limits: [Option<SyncRateLimits>; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventHandle {
    #[serde(deserialize_with = "crate::commands::deserialize_deck")]
    #[schemars(schema_with = "crate::commands::deck_schema")]
    pub deck: usize,
    pub source: String,
    pub load_revision: u64,
    pub grid_revision: u64,
    /// Raw detector event index. Never reconstruct this from rounded seconds.
    pub raw_index: i64,
    pub effective_beat: f64,
    pub source_seconds: f64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum Duration {
    Seconds {
        value: f64,
    },
    Beats {
        #[serde(deserialize_with = "crate::commands::deserialize_deck")]
        #[schemars(schema_with = "crate::commands::deck_schema")]
        clock: usize,
        value: f64,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Timing {
    /// Resolve the exact loaded source event against the plan generation.
    SourceEvent {
        #[serde(deserialize_with = "crate::commands::deserialize_deck")]
        #[schemars(schema_with = "crate::commands::deck_schema")]
        deck: usize,
        raw_index: i64,
    },
    /// Finish this action, including native settling, before the original
    /// session-bound closing endpoint. Uses second durations, not guessed beats.
    Finish {
        #[serde(default)]
        before_endpoint_seconds: f64,
    },
    Event {
        event: EventHandle,
    },
    In {
        duration: Duration,
    },
    After {
        action: String,
        #[serde(default)]
        offset: Option<Duration>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub at: Duration,
    pub value: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Curve {
    pub target: ControlTarget,
    /// Strictly positive increasing endpoints; the initial target is inherited.
    pub points: Vec<CurvePoint>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    #[serde(deserialize_with = "crate::commands::deserialize_deck")]
    #[schemars(schema_with = "crate::commands::deck_schema")]
    pub deck: usize,
    pub phase_tolerance_beats: f64,
    /// Requires credible origins on both grids. Omit when bar origin is unknown.
    #[serde(default)]
    pub bar_beats: Option<u8>,
    /// Maintain prepared bounded Sync and phase through this entire action,
    /// including final control verification; failure runs authored recovery.
    #[serde(default)]
    pub maintain: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub id: String,
    pub when: Timing,
    /// Completion constraints for a fixed event; impossible deadlines fail preflight.
    #[serde(default)]
    pub after: Vec<String>,
    /// Members start atomically. Curves expand into native linear ramp segments.
    #[serde(default)]
    pub commands: Vec<EngineCommand>,
    #[serde(default)]
    pub curves: Vec<Curve>,
    #[serde(default)]
    /// Gates the initial atomic gesture; maintain also guards prepared Sync and
    /// phase through every curve segment and final native completion.
    pub alignment: Option<Alignment>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    #[serde(deserialize_with = "crate::commands::deserialize_deck")]
    #[schemars(schema_with = "crate::commands::deck_schema")]
    pub carrier: usize,
    pub seconds: f64,
    pub commands: Vec<EngineCommand>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Closing {
    /// Required audible coverage from this observation, in renderer seconds.
    pub endpoint_seconds: f64,
    #[serde(deserialize_with = "crate::commands::deserialize_deck")]
    #[schemars(schema_with = "crate::commands::deck_schema")]
    pub carrier: usize,
    /// Intentional silence starts here; absent means music must cover the endpoint.
    #[serde(default)]
    pub silence_from_seconds: Option<f64>,
    #[serde(default)]
    pub session_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub operation_id: String,
    pub generation: Generation,
    pub actions: Vec<Action>,
    pub recovery: Recovery,
    #[serde(default)]
    pub closing: Option<Closing>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObserveRequest {
    #[serde(default)]
    #[serde(deserialize_with = "crate::commands::deserialize_optional_deck")]
    #[schemars(schema_with = "crate::commands::optional_deck_schema")]
    pub deck: Option<usize>,
    #[serde(default)]
    pub start_event: Option<i64>,
    #[serde(default)]
    pub count: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub plan: Plan,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReceiptRequest {
    pub id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    pub id: String,
    #[serde(default = "yes")]
    pub stabilize: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedAction {
    pub id: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub native_action_ids: Vec<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Validation {
    pub valid: bool,
    pub operation_id: String,
    pub actions: Vec<ResolvedAction>,
    pub native_steps: usize,
    pub coverage_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arm_within_seconds: Option<f64>,
    /// Advisory only; absent in receipts written before contribution forecasting.
    #[serde(default)]
    pub forecast: Option<crate::forecast::TransitionForecast>,
}

pub fn descriptors() -> Vec<serde_json::Value> {
    use serde_json::json;
    fn schema<T: JsonSchema>() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(T)).unwrap()
    }
    [
        ("performance.observe", "Capture coherent controls, source generations, exact event handles and separate beat/bar telemetry.", schema::<ObserveRequest>()),
        ("performance.validate", "Preflight an artist-authored timeline, curves, atomic gestures, source guards, dependencies and closing coverage; return an advisory timed cached-band contribution, source-arrival and clock forecast without playback changes. Review the returned ticket, then commit that ticket without repeating the plan.", schema::<PlanRequest>()),
        ("performance.commit", "Commit a reviewed ticket (or legacy full plan), revalidate and atomically arm on the native audio clock. No track or passage selection.", schema::<CommitRequest>()),
        ("performance.receipts", "Read actual native member receipts and current controls, including failed/cancelled work and recovery state.", schema::<ReceiptRequest>()),
        ("performance.cancel", "Cancel pending performance work; optionally execute its explicitly authored native stabilization.", schema::<CancelRequest>()),
    ].into_iter().map(|(command,description,input_schema)| json!({"command":command,"description":description,"input_schema":input_schema,"example_args":match command {"performance.observe"=>json!({}),"performance.validate"=>json!({"plan":{}}),"performance.commit"=>json!({"ticket":"reviewed-plan-ticket"}),_=>json!({"id":"job-example"})},"output_schema":match command {"performance.observe"=>schema::<Observation>(),"performance.validate"=>schema::<Validation>(),"performance.commit"=>schema::<CommitReceipt>(),"performance.receipts"|"performance.cancel"=>schema::<Receipts>(),_=>unreachable!()},"example_cli":format!("pulsedeckctl call {command} --args '{{}}' --json")})).collect()
}

/// Exactly one reviewed ticket or legacy full plan; enforced at dispatch too.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
}
impl JsonSchema for CommitRequest {
    fn schema_name() -> String {
        "CommitRequest".into()
    }
    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        serde_json::from_value(serde_json::json!({
            "type":"object", "additionalProperties":false,
            "properties":{"plan":generator.subschema_for::<Plan>(),"ticket":{"type":"string","minLength":1}},
            "oneOf":[{"required":["plan"],"not":{"required":["ticket"]}},{"required":["ticket"],"not":{"required":["plan"]}}]
        })).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeckControls {
    pub channel_fader: f32,
    pub channel_fader_actual: f32,
    pub channel_fader_ramp_active: bool,
    pub gain: f32,
    pub eq: [f32; 3],
    pub filter: f32,
    pub tempo_percent: f64,
    pub playback_rate: f64,
    pub actual_playback_rate: f64,
    pub pitch_semitones: f64,
    pub keylock: bool,
    pub sync_enabled: bool,
    pub is_master: bool,
    pub echo: f32,
    pub reverb: f32,
    pub loop_start_seconds: Option<f64>,
    pub loop_end_seconds: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeckObservation {
    #[serde(deserialize_with = "crate::commands::deserialize_deck")]
    #[schemars(schema_with = "crate::commands::deck_schema")]
    pub deck: usize,
    pub source: Option<String>,
    pub duration_seconds: Option<f64>,
    pub playing: bool,
    pub launch_pending: bool,
    #[schemars(schema_with = "crate::commands::optional_deck_schema")]
    pub launch_clock: Option<usize>,
    pub launch_at_seconds: Option<f64>,
    pub ready: bool,
    pub position_seconds: f64,
    pub effective_beat: Option<f64>,
    pub raw_event_position: Option<f64>,
    pub bar_origin_known: bool,
    pub controls: DeckControls,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Observation {
    pub generation: Generation,
    pub running: bool,
    pub decks: [DeckObservation; 2],
    pub fractional_phase_error_beats: Option<f64>,
    pub bar_offset_beats: Option<f64>,
    pub bar_beats: Option<u8>,
    pub events: Vec<EventHandle>,
    pub native_owner: Option<u64>,
    pub session: Option<SessionEndpoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptState {
    Pending,
    Applied,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemberReceipt {
    pub action_id: String,
    pub native_action_id: u64,
    pub state: ReceiptState,
    pub verified: bool,
    pub curve_boundary_verified: bool,
    pub frame: u64,
    pub completed_frame: u64,
    pub failure: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Receipts {
    pub id: String,
    pub operation_id: String,
    pub lifecycle: String,
    pub reason: String,
    pub recovery_applied: bool,
    pub members: Vec<MemberReceipt>,
    pub observation: Observation,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CommitReceipt {
    pub job_id: String,
    pub operation_id: String,
    pub lifecycle: String,
    pub validation: Validation,
    pub duplicate: bool,
    pub replayed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionEndpoint {
    pub session_id: String,
    pub finish_unix_ms: u64,
    /// Remaining time from generation.observed_rendered_frames. This is a
    /// projection of the existing application monotonic session, not a new timer.
    pub endpoint_seconds: f64,
}

#[cfg(test)]
mod commit_schema_tests {
    use super::*;
    #[test]
    fn typed_ticket_serialization_roundtrips_and_satisfies_exactly_one_schema_branch() {
        let value = serde_json::to_value(CommitRequest {
            plan: None,
            ticket: Some("reviewed-ticket".into()),
        })
        .unwrap();
        assert_eq!(value, serde_json::json!({"ticket":"reviewed-ticket"}));
        let restored: CommitRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(restored.ticket.as_deref(), Some("reviewed-ticket"));
        assert!(restored.plan.is_none());
        let descriptor = descriptors()
            .into_iter()
            .find(|v| v["command"] == "performance.commit")
            .unwrap();
        let schema = &descriptor["input_schema"];
        let matching = |object: &serde_json::Value| {
            schema["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|branch| {
                    branch["required"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|key| object.get(key.as_str().unwrap()).is_some())
                        && !branch["not"]["required"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|key| object.get(key.as_str().unwrap()).is_some())
                })
                .count()
        };
        assert_eq!(matching(&value), 1);
        assert_eq!(matching(&serde_json::json!({})), 0);
        assert_eq!(matching(&serde_json::json!({"ticket":"x","plan":{}})), 0);
        assert_eq!(matching(&serde_json::json!({"plan":{}})), 1);
        assert_eq!(schema["properties"]["ticket"]["type"], "string");
    }
}

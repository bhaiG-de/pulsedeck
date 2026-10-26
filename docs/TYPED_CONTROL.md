# Typed artist control

The PulseDeck MCP tools use the shared protocol, native command definitions, compiler and renderer. MCP exposes the DJ/source/session registry; its `deck_control` tool contains the complete typed native command vocabulary. It chooses no music, passages, arrangement or timbre.

Start with `performance_observe` and `{"deck":"A","count":16}`. The response contains one coherent source/control generation, deck controls, exact source-bound event handles, fractional phase and (when both origins and meter are known) separate bar offset. `sync_enabled` is not evidence of bar alignment. A credible grid origin still is not acoustic proof of the intended phrase.

Save a score as a JSON object containing `operation_id`, the returned `generation`, `actions`, `recovery` and optional `closing`. Call `performance_validate` with `{"plan":SCORE}`, then `performance_commit` with `{"ticket":"RETURNED_TICKET"}`. Validation makes no playback changes. Commit recompiles against a fresh coherent snapshot and the callback checks ownership and generations before applying any control. Validation returns an immutable bounded-cache score ticket and `arm_within_seconds` at review time. Commit resolves that exact score, then revalidates its original cue and guards; it never retargets an expired cue. Full-plan commits remain compatible.

Use the same ticket or `operation_id` and identical score to resolve an uncertain commit acknowledgement. A duplicate returns the same typed job receipt without replay. Read `performance_receipts` with `{"id":JOB}`; it reports actual per-member pending/applied/failed/cancelled state, a separate `verified` flag, actual recovery status and current controls. Acceptance is not completion. `performance_cancel` with `{"id":JOB}` requests the authored stabilization; adding `"stabilize":false` cancels pending work without restoring controls. Ordinary human/source/position/grid takeover yields ownership and never restores stale controls over the new owner.

## Direct deck controls and recovery

`deck_control` takes the typed command fields directly, plus optional `expected_load_revision`, `request_id` and `expected_state`. Preparation can require `{"playing":false,"launch_pending":false,"channel_fader_max":0.002}`; the callback checks these expectations before applying the command. Intentional live seek/juggling remains available without isolation expectations. Direct receipts expose callback before/after state, actual disposition and any failed predicate. `cancel_launch` reports `cancelled_pending` or `no_pending_launch`; neither pauses a source that is already playing nor cancels an independent ramp. An uncertain callback timeout exposes a `receipt_ticket`; query `engine_transport` with that ticket to resolve actual execution without replaying the gesture.

## Score fields

Deck/clock fields accept `A`/`B` or `0`/`1`.

- `actions[].when`: `{"type":"source_event","deck":"A","raw_index":N}` (server-resolved against the generation), legacy `{"type":"event","event":EXACT_RETURNED_HANDLE}`, `{"type":"in","duration":{"unit":"seconds","value":2}}`, `{"type":"after","action":"launch","offset":{"unit":"seconds","value":1}}`, or `{"type":"finish","before_endpoint_seconds":0}`. `finish` subtracts the action’s curves and verification margin from its declared closing endpoint.
- `in` and `closing.endpoint_seconds` are relative to `generation.observed_rendered_frames`, so waiting between observe and commit never moves a closing endpoint. `after` waits for actual verified native completion. Optional `actions[].after` also constrains a fixed event; an event preceding its dependency's completion fails preflight and the native guard checks actual completion again.
- Validation `end_seconds` bounds actual verified completion, including the native control smoothing time. A fixed dependency at a ramp’s nominal endpoint is rejected when it leaves no settling margin; `after` waits for actual verification.
- `commands` contains the actual typed `EngineCommand` objects. `commands_describe` with `{"name":"deck_control","action":"set_loop"}` discovers one action; omit `action` for the complete shared native schema. Three-band EQ uses linear gain `0..2` (unity `1`), filter is `-1..1` (negative low-pass, positive high-pass), faders are `0..1`, tempo is source-relative percent, and pitch is semitones.
- `curves` contains `{target,points:[{at:{unit,value},value}]}`. Points are increasing endpoints relative to the action start and compile into native linear ramp segments. Commands and first curve segments in one action start atomically; resource conflicts are rejected.
- `alignment` optionally requires `{deck,phase_tolerance_beats,bar_beats}` before any member of an atomic gesture. Omit `bar_beats` when only fractional phase is required. Known deterministic misalignment fails preflight; the callback checks the actual phase before applying the gesture. Opt-in `maintain:true` requires an already prepared bounded Sync follower and opposite master, and monitors that relationship and phase through the entire action’s native completion. The authored recovery handles a lost contract; manual takeover still yields.
- `recovery` requires `{carrier,seconds,commands}`: explicit full carrier fader, isolation of the other deck, and explicit three-band carrier EQ baselines. EQ baselines may preserve deliberate coloration. Source preparation and completed valid work are retained. If that authored carrier is unavailable, recovery is reported as failed; no hidden replacement track is chosen.
- `closing` requires `{endpoint_seconds,carrier}`; optional `silence_from_seconds` declares intentional silence, and `session_id` binds an active session. Observation exposes `session.endpoint_seconds`, projected from the existing application monotonic session clock into this observation. Loop release uses the conservative loop-end source position for EOF coverage. Carrier rate/position edits with uncertain event arrival require a new observation. The compiler checks source coverage and verified action/fade endpoints; native execution cannot continue pending score controls beyond the declared renderer endpoint.

Adjacent curve segments carry an explicit native continuation link. Intermediate receipts can report `curve_boundary_verified:true`: the authored ramp envelope reached the node before continuing, rather than holding the smoothed control stationary there. Final settled completion and `after` dependencies keep their separate proof; unrelated replacement cancels the old ramp.

Native source-event triggers remain bound to exact source seconds and generations. A paused incoming source event is supported after an earlier explicitly linked authored launch. Integer `source_event` anchors avoid copying source-time doubles. Legacy event handle identities must retain full precision; confusing raw detector indices with effective beats is rejected.

Beat durations use `{"unit":"beats","clock":"A","value":16}`. They currently require an already playing, unlooped stable clock without Sync or planned rate/position edits. Unproven projections are rejected with preparation hints. A source event on a Sync follower requires explicit native Sync rate bounds for conservative dependency timing, while its actual trigger stays on that source clock. Closing coverage with Sync also requires declared native rate bounds. Unsupported future position/loop combinations fail explicitly; validate does not guess reachability.

## Small score example

After observing and choosing the passage, build this object with the returned generation. This demonstrates a fixed renderer-time gesture; choose musical events and curves for the actual performance.

```json
{
  "operation_id":"artist-passage-1",
  "generation":"REPLACE_WITH_RETURNED_GENERATION_OBJECT",
  "actions":[{
    "id":"filter-gesture",
    "when":{"type":"in","duration":{"unit":"seconds","value":2}},
    "commands":[],
    "curves":[{"target":{"type":"filter","deck":"A"},"points":[
      {"at":{"unit":"seconds","value":2},"value":-0.3},
      {"at":{"unit":"seconds","value":4},"value":0}
    ]}]
  }],
  "recovery":{"carrier":"A","seconds":0.3,"commands":[
    {"type":"set_channel_fader","deck":"A","value":1},
    {"type":"set_channel_fader","deck":"B","value":0},
    {"type":"set_eq","deck":"A","low":1,"mid":0.5,"high":0.5},
    {"type":"set_filter","deck":"A","amount":0}
  ]}
}
```

MCP uses NDJSON JSON-RPC over stdin/stdout, protocol `2025-11-25`: `initialize`, `notifications/initialized`, `tools/list`, `tools/call`. Logs/errors never contaminate stdout. Common names include `performance_observe`, `performance_validate`, `performance_commit`, `performance_receipts`, `performance_cancel`, and `deck_control`. Schemas and structured outputs come from the shared Rust protocol. Tool errors carry structured control-error text with `isError:true`. See the MCP tests for a client exchange.

Validation checks deterministic feasibility and bounded execution; it cannot certify perceived musical quality or the artist's phrase choice. A score without `closing` makes no assertion about coverage to a set endpoint.

Validation also returns an advisory transition `forecast`, retained in the committed receipt. It projects cached normalized source envelopes through the scored source progression and nominal fader/gain/EQ weights, with timed windows, source provenance, pulse/key/rate context, explicit uncertainty and omitted ranges. These coarse band contribution cues locate changes and held-back support; they are not rendered audio or summed loudness because analysis and EQ crossovers differ. Musical findings do not reject a feasible arrangement: the artist compares them with the intended course and can preserve an intentional withdrawal. Unsupported transformations remain explicit instead of receiving invented predictions.

The installed plugin starts the app’s bundled `pulsedeckctl mcp --stdio` server. Legacy CLI aliases use the same backend for compatibility; DJ skills use the connected MCP tools.

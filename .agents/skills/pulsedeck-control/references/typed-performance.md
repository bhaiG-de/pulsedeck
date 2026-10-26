# Shared typed performance

The DJ chooses the records, paired passages, phrase relationship and combined sound. The shared surface validates and executes those decisions. It does not choose the next song or invent a replacement musical target.

## Tools and transport

MCP exposes `performance_observe`, `performance_validate`, `performance_commit`, `performance_receipts`, `performance_cancel`, `deck_control`, and source/library/session tools. `deck_control` accepts the full typed engine vocabulary: transport, cueing, tempo/Sync, pitch, EQ, filters, loops, FX, jog/scratch and sampler controls. Discover the relevant schema rather than substituting a familiar bass/fader recipe for the desired sound.

Use the [connected MCP server](../../references/mcp.md), not a local transport process. `performance_validate` accepts `{"plan":SCORE}` and returns an immutable `ticket`; `performance_commit` accepts `{"ticket":"RETURNED_TICKET"}`. A full `{"plan":SCORE}` commit remains available for compatibility. `performance_observe` accepts `deck`, `start_event` and `count`; `performance_receipts` and `performance_cancel` accept `{"id":JOB_ID}` using the returned `job_id`. Follow each actual tool schema.

Direct mutations accept an observed `expected_load_revision` and optional `request_id`. Retain that retry identity after an uncertain acknowledgement. A score's immutable `operation_id` identifies an identical uncertain commit; changing the arrangement is a new operation.

For a direct preparation gesture, add `expected_state` with the intended `playing`, `launch_pending` and/or `channel_fader_max` values. Direct deck receipts report actual application, rejection or cancellation disposition, including the source load revision, position and fader/ramp state at the callback. Follow the [recovery branch](recovery.md) from those facts.

## Bind the arrangement to the prepared sources

Prepare the chosen source, entry, tempo, key lock, Sync and isolated controls using [deck preparation](deck-preparation.md). Then observe. The `performance_observe` packet contains source identities, controls, a `generation`, exact loaded-source event handles, separate fractional beat/bar telemetry and the active session endpoint when present. Source waveform evidence still comes from [library inspection](../../pulsedeck-library/SKILL.md).

Carry `generation` unchanged into the score. Choose event timing with `{"type":"source_event","deck":"A","raw_index":N}`: the server resolves its exact source time against that generation. Count musical intervals using effective beats; use the observed raw detector index for the anchor. Legacy complete event handles also work, but their floating-point fields need no manual reconstruction. Seek or prepare first, then obtain the generation used for the score. Ordinary playback advances time without requiring another preparation cycle.

A score contains `operation_id`, `generation`, `actions`, `recovery`, and optional `closing`. An action has an `id`, `when`, and `commands` and/or `curves`. One command is a valid list; simultaneous members form one atomic gesture. Full schema discovery supplies exact field names and supported bounds.

Choose the timing that expresses the musical relationship:

- `{"type":"source_event","deck":"A","raw_index":N}` uses the selected source event without copying timestamps; `{"type":"event","event":HANDLE}` accepts an unchanged full handle. For a paused incoming source, link its known authored launch with the action’s `after` IDs so the compiler can establish that future source timeline.
- `{"type":"in","duration":{"unit":"seconds","value":N}}` is measured from the packet's observation frame, so validation time does not shift the score later.
- `{"type":"after","action":"ID","offset":{"unit":"seconds","value":N}}` follows actual completion. `after:["ID"]` on a fixed-event action additionally requires that predecessor to finish before that event.
- `{"type":"finish","before_endpoint_seconds":0}` places the action so its curves and native completion finish at the declared `closing` endpoint. Choose a positive offset to finish earlier. This uses the original session endpoint, including control settling, rather than a hand-computed wall-clock delay.
- Durations can use `{"unit":"beats","clock":"A","value":N}` when that clock is stable and advancing. The validator reports an unproven projection when a paused, changing or follower clock cannot support that conversion. Use exact source events or an established stable clock for the intended relationship.

A curve names a typed target and increasing endpoints, such as `{"target":{"type":"eq_mid","deck":"B"},"points":[{"at":{"unit":"seconds","value":4},"value":0.6},{"at":{"unit":"seconds","value":8},"value":1}]}`. Values are native control units, not percentages or dB. EQ/gain use 0–2 with unity 1; faders and FX use 0–1. Filter uses −1–1: negative low-passes, positive high-passes, and 0 is neutral. Shape these movements from the source contributions and desired combined passage.

A curve starts from the target’s actual value when its action begins; its first endpoint is at least 0.001 seconds later, and subsequent endpoints strictly increase. To open an isolated fader over eight seconds, prepare it at zero before observation and specify only the eight-second endpoint at one. An `at:0` point is not an instantaneous-set primitive. Establish a different initial baseline in preparation or a preceding completed action.

For a sustained beatmix, prepare the outgoing master, bounded incoming Sync follower and aligned entry, then put the audible overlap and clearance in one action with `alignment:{"deck":"B","phase_tolerance_beats":0.1,"bar_beats":4,"maintain":true}`. Choose the tolerance from the intended rhythmic relationship; omit `bar_beats` when origins are unknown. `maintain:true` requires the prepared native Sync relationship and explicit rate limits, checks the declared phase and optional bar relationship through that action's complete curve span, and uses the authored recovery if it cannot maintain the contract. Put outgoing clearance in this same span when both grooves remain audible until clearance. Native Sync corrects fractional phase within the chosen bounds; it cannot establish phrase identity or fix a wrong grid.

Without `maintain:true`, `alignment` gates only the initial atomic gesture. That suits a deliberate free-clock or nonrhythmic exchange, or demonstrably stable manual matching. It does not maintain a beatmatch. A playing gesture can check the paused entry before starting playback and exposure together. Derived follower tempo changes within an owned bounded Sync relationship do not stale the generation; manual changes, source/position/grid changes and role/bounds changes still do.

## Validate, commit and follow the result

Validate the complete bounded arrangement, including coupled controls, before commitment. Errors identify infeasible timing, stale state, conflicting controls or unproven projections. Keep valid preparation and change the affected musical relationship using current evidence. A transfer cannot wait for a reveal that finishes after its fixed target.

Read the returned `forecast` before commitment. Its windows align cached source envelopes with the score's predicted source progression and fader/EQ movement; findings are timed advisory evidence. Give the art skill the contribution/ownership course, pulse/key/rate context and explicit partial/unavailable coverage. Compare these with the intended support, withdrawal and arrival. Revise an unintended relationship and validate the changed score; deliberate space remains a valid artistic choice. Validation success proves feasibility, not that every advisory is resolved or that the mix will sound smooth. The committed receipt retains the forecast for comparison with actual receipts and source progression.

Use validated `end_seconds` bounds for fixed dependants, completion-relative `after` for relationships that follow actual completion, and `finish` for a session-bound closing gesture.

After reviewing the forecast, commit its returned `ticket` and retain `job_id`. `arm_within_seconds` is the remaining time at validation until the first chosen action; the ticket preserves that cue and does not move it later. An expired cue returns a timing failure, so compose a reachable alternative from current state. Reusing the ticket after an uncertain acknowledgement returns the same committed job without replay. A revised score needs fresh validation and a new operation ID. The native owner rechecks state before controls execute. Receipts distinguish applied, verified, failed, cancelled and pending members and expose actual controls. A successful receipt establishes execution; the art skill judges whether the resulting support, foreground and tension/release course made musical sense.

For a continuing multi-segment curve, `curve_boundary_verified:true` means the native ramp reached that segment’s authored envelope boundary before its linked successor continued it. It does not claim the control became stationary and fully settled there. The final segment and completion-relative dependants retain settled-completion checks; an unrelated replacement remains cancellation.

Declare recovery as an available carrier plus explicit native control baselines and a short renderer-time duration. Include carrier/other faders and all three carrier EQ baselines, with filter/FX or other affected controls as needed. Preserve intentional tone choices. Native recovery cannot make an ended or unavailable source healthy; its receipt tells you when the declared fallback could not run. A manual takeover yields authority rather than fighting the user.

For closing, copy the observed `session.session_id` and `session.endpoint_seconds` into `closing` with the selected carrier. The endpoint remains bound to that observation frame and the existing timed session. Declare `silence_from_seconds` only for an intentional silent ending. Use a `finish` action for the selected closing fade/gesture. Coverage and fade validation account for the planned exit; bounded Sync coverage needs declared native rate limits. A completed timer, a completed fade and a musically complete finish are separate outcomes.

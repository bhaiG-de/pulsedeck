# Timed performance and continuity

The agent keeps choosing the records and arranging their passages. The endpoint session supplies time ownership without choosing songs. Use the connected MCP tools; a missing session tool is a connection/capability problem, not a reason to start a second transport.

## Keep the original finish

Start `mix_session_start` once while a ready carrier plays, with exactly one of `duration_seconds` or `finish_unix_ms`. `prepared_id` is optional. A duration request is, for example:

```json
{"duration_seconds":3600}
```

Keep its returned `session_id` and absolute finish. Use `mix_session_status` with that identity and copy the coherent `performance_observe.session` endpoint into a closing score. Score/direct performance gestures preserve the endpoint. Recovery does not earn another full duration; an app restart does not resume playback.

`mix_session_stop` cancels that endpoint and its pending owned move while ordinary music keeps playing. If the user wants music stopped, also apply the appropriate direct `pause` controls. `automation_stop` is a broader automation operation; use it only for that intended scope because it also revokes/yields the active endpoint.

A failed move or missing audible route is attention, not completion. The timer remains active through a recoverable gap and still stops the decks at the original endpoint. Human takeover/stop, engine restart, stalled output or device errors can end authority. Inspect the actual lifecycle and `continuity_notice` before continuing.

## Prepare and arrange the continuation

Load and cue the chosen successor while its deck is isolated. Plan the normal musical exchange while the carrier still has a useful statement and exits. A loaded track or Play behind a closed fader is not an audible handover. Use verified score receipts to identify the released deck and prepare the following move.

`mix_session_successor` can explicitly reserve a ready paused isolated source as a one-shot emergency handover: `{"deck":"B","session_id":"OBSERVED_SESSION"}`. This is optional artist-selected contingency, not repertoire selection. If the carrier reaches its last second without an active normal transition, the runtime starts that prepared entry, neutralizes its gain/EQ/filter/FX, disables incoming Sync and opens its fader before pausing the outgoing source. It preserves the output route and can fail if that route cannot carry the source. Those tone resets are part of this fallback's contract; use the score's authored recovery when deliberate timbre must be preserved instead.

Loading, seeking or playing the reserved target invalidates the reservation. A normal handover consumes it. Cancel with `mix_session_successor` using the same deck/session and `cancel:true`; do not repeatedly rearm a stale entry.

## Follow and close musically

Use `events_get` with `session_id`, the saved `since` cursor and a bounded `wait_ms`. `successor_due`, `no_audible_carrier_route` and fallback outcomes are decision prompts, not musical phrase boundaries. Keep the performer active enough to choose and register its next passage; a passive log does not wake an idle agent.

For the final passage, compare remaining session time with usable source time at its played rate, including loops, tempo changes and final fade. Arrange a worthwhile continuation/extension before exhaustion. Declare `closing` on the final score using the observed session endpoint and place the final gesture with `when:{"type":"finish","before_endpoint_seconds":0}`. The compiler accounts for the action’s curve duration and native completion; choose an earlier finish offset when the musical ending needs it. Validate coverage against the original endpoint. An intentionally earlier silence belongs in the closing plan; an accidental EOF gap does not become a successful finish because the timer later completes.

After a failed transition, establish sound from the intended available source and current route; an ended outgoing clock cannot trigger future work. Report any gap and evaluate the recovered musical course separately from the original blend.

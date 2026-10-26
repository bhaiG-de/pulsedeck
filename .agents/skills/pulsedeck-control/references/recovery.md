# Recover the musical performance from actual state

Keep the original endpoint and the listener's intended musical course. Read `performance_receipts` and a coherent `performance_observe`: actual advancing sources/positions, readiness, faders/EQ/filter/FX, rate/Sync, remaining work and session endpoint. Reuse valid source maps, loaded audio, exact cues and useful applied controls.

| Outcome | Next musical action |
|---|---|
| Validation/commit rejected before arming | Keep preparation; correct only the infeasible arrangement or source binding, then validate a complete new score. |
| Acknowledgement uncertain | Recommit the same reviewed ticket, identical operation ID/body or direct `request_id`; a new ID can duplicate the gesture. |
| Accepted score remains active but no longer fits | Inspect its applied members, then cancel the owned score before replacing pending gestures. Choose stabilization only if the authored recovery remains appropriate. |
| A member failed or its event passed | Retire that branch, observe fresh state and compose future reachable events with a new operation ID. Retrying the past event cannot move it. |
| Incoming source advances but reveal/transfer failed | Keep its actual position and useful contribution. Arrange only the missing role transfer/clearance; do not re-seek an exposed source merely to replay the old plan. |
| Outgoing ended; intended incoming is ready and isolated | Establish its appropriate audible continuation directly through `deck_control`, verify source progression/output and then recompose. Do not depend on the ended clock. |
| Human stopped or took over | Respect that state and report it; do not restore remembered controls over the user. |

The native score retains its declared recovery carrier and explicit control baselines. A `recovered` lifecycle reports recovery, not completion of the intended handover. Preserve deliberate EQ/filter/FX choices. If that carrier is unavailable, the runtime cannot invent healthy music; use current evidence for a new artist-chosen branch.

Direct `deck_control` receipts report the callback's `before`/`after` state and actual outcome. `cancel_launch` can report `cancelled_pending` or `no_pending_launch`; it does not pause a playing source or cancel a separate fader ramp. If the launch already happened, continue the advancing passage and arrange its remaining transfers. For an intentional re-entry, first author the isolation/pause, then use the guarded preparation seek in [deck preparation](deck-preparation.md). A callback timeout with `receipt_ticket` is unresolved execution: use `engine_transport` with that ticket to retrieve the actual receipt rather than issuing another gesture. Receipt history is bounded; an expired lookup is not evidence that nothing played.

Recompose the smallest useful remainder: identify the surviving foundation and statement, choose the next complete incoming approach/arrival, and count backward from a reachable future exchange. Validate with `performance_validate`, commit with `performance_commit`, and follow that new `job_id` through `performance_receipts`/`events_get`. Keep the same timed session.

For example, if the old launch was A736 and A now passes A752, an intact A statement through A816 can support B144→B160: launch at A800 so the 16-beat approach arrives at A816. Use fresh integer source-event anchors and actual playing rates. Preserve already completed preparation; seek to B144 only if B remains isolated and that new entry is intended. If B is already audible, recount from its current passage instead. If A cannot supply the remaining support, choose a compact reachable exchange or another musical continuation.

Verify the realized foundation/foreground transfer and following development. Credit actual recovered music, not a score that failed to play.

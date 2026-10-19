# Assisted next moves

The Suggest panel and `mix.*` commands share the planner and native recipes. For supported moves, select an option and execute it with **one action call**. PulseDeck owns preparation, launch, audible transfer, clearance and recovery; the model chooses the musical move.

For a **continuous set**, use [continuous sets](continuous-sets.md): start one detached successor queue and end the agent turn. The workflow below is for a single individually selected move.

## Single-move workflow

1. Request `mix.options` once for the active pair and requested collection. For Spotify Liked Songs use `playlist: "spotify:liked", provider: "spotify"`. Read the returned source, entry, technique, timing, rate correction, passage evidence, warnings and continuation runway. `mix.options` is read-only and uses the indexed collection, not just the latest provider page.
2. Choose a feasible option and review its supplied evidence. Unknown vocals and inferred phrases remain uncertain; acknowledge those uncertainties or audition when appropriate. `review_confirmed` records that review, not a claim that listening occurred. Avoid extra metadata/schema reads when the option already supplies the needed evidence.
3. Execute once and consume lifecycle feedback from the same invocation:

```sh
pd mix execute --option-id OPTION_ID --review-confirmed --follow --request-id MOVE_UNIQUE_ID
```

If the target contains a different paused source, add `--replace-prepared` only when replacing it is authorized. The target must be isolated; playing and user-controlled decks are protected. Do not combine this action with a second Play, Sync, seek, fader, persistent low-level queue or shell timer.

If `mix.execute` is absent from the running app, report that the running build needs updating; do not silently recreate the same supported move with a long primitive-control loop.

The invocation returns acceptance immediately, then streams compact `mix.updated` events: `preparing`, `ready`, `armed`, `transitioning`, and a terminal result. An uncertain native acknowledgement can briefly report `arming`; the app keeps the original job identity and resolves or cancels it without replaying the move. `fallback_used` can become true during preparation. Native recovery may produce `recovering` then `recovered`; **recovered is an incomplete move with the outgoing carrier restored, not a successful handover**. Failures and completion include `decision_required`; completion carries the continuation and next-decision budget.

A long-running tool may yield a process handle. Retain it and consume its output when waiting for the next musical decision; do not issue transport/status polls during accepted native execution. An idle stream is normal while waiting for the scheduled boundary. Selection reads and a wait for a terminal event are distinct from executing the action twice.

4. On `completed`, choose and execute the next move promptly using the released deck. The engine has already verified the advancing incoming carrier and final clearance. No extra `mix.status` is needed for an unambiguous completion event. The next-decision budget is a maximum allowance, not a reason to wait before preparing.

## Ownership, fallback and event recovery

`mix.execute` is app-owned: closing the CLI or losing the connection does not cancel it. Keep the acceptance `operation_id` and request ID. Retry an uncertain submission with the same request ID and identical arguments; never resubmit a new action ID just because the stream was quiet.

`arming` is a normal ownership state while the native submission is acknowledged. Continue following the same operation; it is not a request to submit again. A delayed acknowledgement is confirmed or cancelled by the app within its bounded recovery window.

If loading misses the selected passage, the default fallback permits a revalidated **short exchange with the same source and incoming entry**, a future native boundary, and sufficient continuation runway. It never broadens the collection or forces a large rate change. `--allow-fallback=false` opts out. A failed native launch leaves/restores the healthy outgoing carrier. If the source cannot become ready or no valid fallback remains, the operation reports failure and preserves the carrier; it cannot create unavailable audio. React to that decision event while music remains.

After a lost connection, resume from the saved event cursor:

```sh
pd events wait --operation-id OPERATION_ID --since CURSOR --timeout 60s
```

Use `mix.status` once when the event buffer reports `resync_required` or the receipt is ambiguous. The `--follow` client handles buffer resynchronization internally. No automatic replay occurs after an app restart. Use `mix.cancel` with `operation_id` for this move, or `automation stop` for all owned pending automation. These cancel work and leave ordinary playback running; stop music separately when requested.

## Optional human preparation and audition

For a deliberate audition, `mix.prepare(option_id)` leaves the target paused and isolated. `mix.preview(prepared_id, enabled)` requires a distinct headphone output, is bounded to 30 seconds, and restores the entry on stop. Then `mix.arm(prepared_id, review_confirmed:true)` submits the complete native recipe. Keep the chosen option and prepared identity bound together. This staged workflow is for actual preparation/audition needs; routine agent moves use `mix.execute`.

## Runway and repertoire

Every option reserves music after incoming clearance for the **following** move: 60 seconds of unknown successor preparation, 15 seconds for a recovery attempt, 15 seconds for the musical decision, plus 20 beat intervals for the next short exchange/verification. The calculation uses the prepared rate and current full-PCM audible bounds at −60 dBFS, with conservative waveform fallback for older metadata until source preparation upgrades it. Late entries that cannot support this horizon are rejected, even when the current overlap fits. `continuation_seconds`, `minimum_continuation_seconds` and `next_decision_within_seconds` make this visible.

Supported recipes are EQ/layer blends, filter blends, short phrase exchanges, echo exits and reverb exits. Echo/reverb exits use eight-bar native recipes with independent post-fader main returns and separate headphone effects; outgoing sends close before clearance and the bounded return may persist up to eight seconds across paused/replaced dry sources. Read capability reasons for other techniques. Custom FX beyond the exposed exit recipes, loops, tempo curves, stems and gestures require their own supported execution workflow; do not relabel an EQ blend as another technique.

`mix.session.start(prepared_id, duration_seconds)` creates an application-owned duration endpoint for a ready/armed move; it does not choose subsequent tracks. Configure it once when a bounded set needs it, and continue selecting moves from completion events. `mix.session.status`/`.stop` inspect/revoke that endpoint. Avoid primitive source-bound timers for a whole set.

## App-owned successor queue

See [continuous sets](continuous-sets.md) for the complete start-and-handoff workflow. Continuous queue start returns immediately without `--follow`; the application owns preparation, selection, native transitions and its duration endpoint. Healthy queues need no model wakeups. Retain the receipt, report acceptance and end the turn.

Ordered queues support `mix.queue.edit` with `expected_revision`; edits replace pending sources and retain the active move. Source preparation is isolated, bounded by carrier runway and canceled by owner. Collection queues require complete indexed membership and choose analysed candidates across that scope. Source failures have a bounded consecutive skip budget. `mix.queue.notice` reports actionable outcomes; terminal summaries persist without replay after restart. Manual takeover yields ownership and requires a new user request before resuming.

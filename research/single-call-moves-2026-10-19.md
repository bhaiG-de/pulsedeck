# One-call moves and event feedback — 19 October 2026

## Public contract

Read `mix.options` for the scoped collection, choose a musical move, then submit it once:

```sh
pulsedeckctl mix execute --option-id OPTION --review-confirmed --follow --request-id UNIQUE_MOVE --json
```

The same mutation is available as `mix.execute({option_id, review_confirmed, replace_prepared?, allow_analysis?, allow_fallback?})`. It returns an accepted `operation_id`, the selected option and an event cursor. The app owns the operation after the CLI disconnects. `review_confirmed` acknowledges review of supplied passage evidence and uncertainty; it does not fabricate vocal detection or listening.

The CLI follows `events.get` with `operation_id`, a cursor and a condition-variable wait. It streams only move lifecycle changes and performs one status resynchronization if the bounded event history has a gap. It does not poll deck transport to time actions. Closing the follower does not cancel the move. `mix.cancel(operation_id)` cancels owned preparation/native work; `automation.stop` also stops accepted preparations so they cannot arm after a stop.

States: preparing → ready → arming → armed → transitioning → completed. Arming reserves ownership before native submission; an uncertain acknowledgement remains in that state against its original native job and is bounded to confirmation/cancellation. It never triggers a duplicate move. A fallback can update the selected recipe while still preparing. Native failure can produce recovering → recovered; recovery is explicitly incomplete, with the outgoing carrier restored. Failed, cancelled, yielded, interrupted and invalidated are also terminal. The stream includes operation/job identity, fallback use, source/technique, timing, errors, continuation and the next musical decision budget. Full waveform/snapshot data does not enter this stream.

The [Mixxx source comparison](mixxx-architecture-and-improvements-2026-10-19.md) uncovered a feedback defect after live validation: fast transport intentionally omits track metadata, so comparing its absent track against prepared metadata falsely invalidated valid moves. Feedback now checks source/load, grid, seek, deck and routing revisions. Full metadata identity checks remain at preparation/arming boundaries. The observer reads transport after locking preparations to avoid judging a newly prepared move against an older read.

The UI receives the same state changes through `mix-updated`, with full option evidence and monotonic cursors. It reconciles once after subscribing and ignores older snapshots; the former 700 ms status polling and periodic option polling have been removed. Manual Prepare → Headphone preview → Arm remains available alongside Execute move.

## Execution and fallback

The app worker isolates and loads the selected target, checks identity/ownership, applies entry/rate/EQ/filter controls and compiles a native recipe. All launch, audibility, layer transfer and clearance timing stays on the existing render-thread scheduler. Every transfer requires an advancing incoming carrier; native failure recovery preserves/restores the outgoing carrier.

If preparation misses the original passage, the automatic path may select an eligible short exchange with the same incoming source and entry. It rechecks the future boundary, phase drift, natural-speed policy, passage evidence and following-move runway. This is bounded replanning, not a new song search or an independent competing fader schedule. `allow_fallback:false` disables this substitution. A finite preparation deadline cancels owned provider work and requests a decision before the useful carrier audio is exhausted. Unavailable audio cannot be manufactured; a failed load or absent feasible passage reports failure while preserving the current carrier.

Prepared windows are revalidated directly with their selected entry/exit, without depending on whether they still rank in the shortlist's top three boundaries. Target tempo comes from the selected future passage, remains pinned through preparation and is checked against the incoming entry rate. Normal playhead BPM jitter no longer invalidates a stable future passage; source/grid/seek/rate/control changes remain guarded.

Accepted move summaries are journaled. Restart restores terminal operation identities and emits interruption for unfinished work, including preparation that had no native job yet. It never resumes or replays playback. An idempotent retry retrieves its original acceptance and the follower can consume the restored terminal event.

## Continuation runway

The old check reserved only four bars after incoming clearance. The observed 174-second source entered at 101.58 seconds and finished the handover around 134.07 seconds, leaving about 40 seconds for another Spotify load. That is inadmissible now.

For each candidate:

- Predict incoming wall-clock music remaining after clearance plus native verification, using its prepared rate and the end of non-silent band-energy evidence.
- Require 60 seconds for the unknown successor's preparation, 15 seconds for a recovery attempt, 15 seconds for the next musical decision and 20 beat intervals for the next short exchange/verification.
- Reject insufficient continuation with `next_transition_runway`.
- Expose continuation, minimum continuation, successor preparation budget and the next-decision allowance in the option. Submit the following move promptly after completion rather than consuming this entire allowance.

These are explicit conservative operational budgets, not guarantees of provider response times or of musical quality. Existing grid, pulse, phase, key, energy and ±2% speed checks still apply.

## Skills

The control entrypoint and assisted-move reference now default to one `mix.execute --follow` invocation per supported move. Continuous-session and transition references clearly scope primitive scheduling to custom moves. The art skill uses returned evidence/uncertainty and runway for supported moves rather than rerunning timing arithmetic. Library instructions use the indexed, scoped planner rather than paging only the latest liked songs. A running build without `mix.execute` is reported as needing an update.

## Evidence

See `artifacts/single-call-moves-2026-10-19/` for manifests, logs, summaries and the preserved pre-fix Luna observation. Audio checks decode unchanged actual local MP3s: Monolink's Return to Oz (ARTBAT Remix) and Andrew Meller's Born Slippy (Reincarnation Mix). Normal EQ/filter/short recipes, a delayed short fallback and an injected stalled incoming launch are rendered without opening audio hardware. Every frame retains a contributing carrier; finite, protected PCM and zero silent 100 ms windows are asserted. The failed launch must return recovered with the outgoing full and the incoming isolated.

Control transport fixtures test a single submission followed by events and event-gap recovery; browser fixtures test UI behavior, not musical quality. Separate native restart checks exercise the actual built app with audio disabled. Live Spotify network behavior, subjective sound and physical headphone isolation require a later performance trial.

### Final validation

- 154 backend tests passed; four opt-in tests ignored and the existing 100k-row SQL speed assertion filtered after measuring 107.71 ms against its 100 ms limit. That query code was unchanged.
- 71 frontend tests passed; browser checks passed for one-call execution, event feedback, no move polling and the retained audition workflow.
- CLI/protocol tests passed, including exactly one submission followed by events and gap resynchronization.
- Strict Clippy passed for the app, CLI, protocol and engine, including all targets.
- All five actual-song native renders passed; zero silent 100 ms windows, bounded rate correction and a contributing carrier on every frame.
- Actual audio-disabled desktop restart test passed, including a cursor ahead of the restored journal, idempotent replay protection and cancel/preview lock regressions.
- All three skill validators and seven packaging checks passed.
- Debug bundle rebuilt and the idle main app relaunched; the new running instance advertises `mix.execute`, with both decks stopped.

Detailed machine-readable results: `artifacts/single-call-moves-2026-10-19/validation.json`. A live Spotify set was not rerun.

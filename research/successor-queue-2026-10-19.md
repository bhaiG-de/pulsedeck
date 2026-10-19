# App-owned successor queue

The Suggest panel now contains a **Successor queue** section. A person can add selected tracks, reorder/remove pending tracks, choose an explicit collection, set musical limits and duration, review the uncertainty policy, then start or stop. An agent starts the same feature in one command and follows events. The queue has no second fader controller: each handover invokes the existing `mix.execute` service and native scheduler.

## Commands

Start a healthy audible carrier first. Preserve an existing preparation unless replacement was explicitly selected.

```sh
target/debug/pulsedeckctl mix queue start \
  --playlist spotify:liked --provider spotify --energy-direction keep \
  --max-stretch-percent 2 --duration-seconds 3600 \
  --review-confirmed --replace-prepared --follow --request-id liked-set-1 --json

target/debug/pulsedeckctl mix queue start \
  --sources '["local:42","local:17","local:9"]' --provider local \
  --techniques '["eq_blend","filter_blend","echo_out","reverb_exit"]' \
  --review-confirmed --replace-prepared --follow --request-id ordered-set-1 --json
```

Explicit sources retain order. Local IDs resolve to canonical library paths. Spotify URIs must remain exact. Collection selection uses all indexed analysed candidates in the chosen scope, the existing play history and the same planner policy as Suggest. `energy_direction`, `max_stretch_percent` and `techniques` are enforced musical constraints; the queue does not infer a genre from an arbitrary prose label. For a genre-specific set, select its repertoire explicitly or supply an appropriately curated playlist.

Collection mode rejects incomplete playlist membership with `scope_incomplete` and coverage details. The existing full indexing action follows all provider pages; the latest cache page is never treated as the full collection. Indexing is distinct from PCM analysis. A single queued unknown source is prepared on the isolated target before its exact-source passages are planned.

```sh
target/debug/pulsedeckctl mix queue status --queue-id QUEUE_ID --json
target/debug/pulsedeckctl mix queue edit --queue-id QUEUE_ID \
  --expected-revision 7 --sources '["local:9","local:17"]' --json
target/debug/pulsedeckctl mix queue stop --queue-id QUEUE_ID --json
```

Edits atomically replace only pending ordered sources. Preparing/active moves remain bound to their original source and operation ID. Revisions prevent stale edits from silently overwriting newer order. The UI displays item state, availability, technique and failure reason. Acknowledging review permits inferred phrase/vocal evidence within the specified queue policy; it does not turn uncertain content into confirmed musical facts.

## Ownership and lifecycle

`mix.queue.start` returns an accepted queue ID. A worker plans the next move whenever the other deck becomes available. It uses app-native completion receipts to advance; an agent does not need to wake up and sequence each phase. `mix.queue.updated` supplies compact feedback; the UI receives full queue snapshots through `mix-queue-updated`. Queue status is read once on UI attachment or event-buffer resynchronization, not repeatedly polled by the UI or CLI follower.

Each item moves through pending/planning, optional isolated loading/analysis, preparation, arming and native transition to completion. A completed handover increments `completed_moves`. Recovery, source failure, cancellation and restart do not count as completed handovers. Failed sources can be skipped up to `max_skips` consecutive failures (default three, maximum ten). The limit resets after a successful handover. Unsupported grids, insufficient runway, protected prepared material or lost ownership request attention.

Unknown-source preparation is bounded to at most 60 wall seconds and ends earlier when the current carrier needs its final safe transition/recovery reserve. A watchdog checks the reference source, grid, seek, deck and global routing revisions and target isolation. Source ownership includes an opaque engine load ticket. Stop/deadline invalidation changes only that generation; a later manual source owner is preserved. Decoding/analysis may finish computation after cancellation, but cannot use the stale ticket to replace a newer source or arm a move.

Queue stop cancels its outstanding source preparation/native move and its duration endpoint, leaving ordinary music running. External performance controls yield ownership; independent headphone listening controls do not. `automation.stop` also yields queue ownership and cancels pending moves. An interrupted queue is restored as a terminal summary after restart. Accepted request IDs remain idempotent; retrying retrieves the original receipt without replaying playback.

When a duration is supplied, the existing monotonic `MixSession` endpoint starts at queue acceptance, including preparation time, and pauses transports at that deadline. An ordered queue exhausted earlier requests attention instead of choosing unauthorized extra music. Without a duration, ordered exhaustion completes queue ownership and leaves the final source playing. Missing or unavailable audio cannot be manufactured; a terminal attention event requires intervention before the preserved carrier ends.

## Files and tests

Backend: `src-tauri/src/mix_queue.rs`, included by Control, shares the existing prepared-move map, event condition variable, journal and native executor. Control exposes `mix.queue.start/status/edit/stop`; the protocol catalog and CLI share their schema. Local owned source loading uses the existing loader with a scoped ticket and cancellation checks. `SuccessorQueuePanel.tsx` is embedded in Suggest and prevents competing assisted moves while the queue is active.

Deterministic checks cover policy bounds/review, revision-safe pending edits, finite failure skips, truthful completion counts and restart semantics. Actual-local snapshot checks use unchanged local MP3 identities and verify source scope plus metadata-free transport/takeover guards. CLI transport checks prove one queue start followed by queue events. Browser checks exercise add/order/start/edit/stop and event feedback; they are UI/control checks, not listening tests.

The positive runtime driver is:

```sh
python3 tools/test-successor-queue-runtime.py \
  --output artifacts/mixxx-learning-2026-10-19/successor-runtime.json
```

It requires the newly built debug app, the explicit retained library snapshot and actual local sources. It copies the snapshot into a fresh private temporary profile, loads Return to Oz, queues Born Slippy then Explore Your Future, requires at least two native completions, stops an observed uncached-source preparation, then kills/restarts and checks interruption, empty stopped decks and idempotent acceptance. No source library or audio file is modified.

The **test-only null renderer** requires all of: debug build, `PULSEDECK_ISOLATED=1`, explicit temporary `PULSEDECK_DATA_DIR`, private `PULSEDECK_SOCKET` and `PULSEDECK_TEST_NULL_AUDIO=1`. Main refuses unsafe/release activation. It reuses actual `RenderState`, PCM, commands, native scheduling and DSP, discarding the output instead of opening a device. Fixed preallocated buffers and a 15-minute wall limit bound work; preparation uses a paced clock, while armed native batches are accelerated. Stop/shutdown use the existing lifecycle. This establishes functional source/control integration; accelerated timing is not wall-clock, latency, CPU-deadline or listening evidence.

### Actual runtime result

The integration run passed with **two completed native handovers**, an observed uncached local-source job stopped during preparation, the healthy carrier preserved, and a killed/restarted queue restored as interrupted without replay. Retrying its original start request retrieved the same receipt and did not restart playback. The isolated app then shut down cleanly. See [runtime evidence](../artifacts/queue-audio-2026-10-19/successor-runtime.json) and its sibling app log. The source files and original library snapshot were unchanged; no audio device was opened.

# Continuous sets: active DJing or optional automation

Choose how much musical control the task calls for. An agent can perform a set through individually chosen moves and direct controls, or delegate selection and execution to the application. The autonomous queue is optional; it is not a policy that an agent must hand over after the opener.

## Start on the intended instance

Use the installed client for the Applications app. Check the current source/deck/output state; for Spotify also inspect `spotify.native.status`. Allow `restoring:true` to settle before diagnosing login. Native `connected:true` and `playback_permission:true` permit preparation even when `account` is null. Respect audible music, genuine user takeovers and stops.

For Liked Songs, a scoped opener query is an efficient first look:

```sh
pd call mix.openers --args '{"playlist":"spotify:liked","provider":"spotify","limit":8}' --json
```

Search or browse the requested collection as useful for the musical decision. A complete indexed scope is reusable; `mix.index_scope` can fetch missing membership. Indexing is not audio analysis. An unanalysed song can be prepared on demand; waiting for an entire collection to finish is unnecessary.

Load an appropriate opener on a free isolated deck, wait for source readiness, then start it audibly. Native loading already applies loudness normalization; adjust gain for the actual musical balance rather than multiplying it to undo that correction. Keep the next preparation ahead of the current track's useful exit.

## When the agent is making the musical choices

Read the art manual's relevant style/example, inspect useful passages and choose the relationship yourself. `mix.options` offers ranked proposals with their evidence and uncertainty:

```sh
pd call mix.options --args '{"playlist":"spotify:liked","provider":"spotify","limit":12}' --json
pd mix execute --option-id OPTION_ID --follow --request-id MOVE_UNIQUE_ID
```

Use a proposal when its actual entry, arrival and mixer shape suit the idea. A four-bar short exchange and an EQ bass swap are not compulsory answers to every pair. For a different phrase relationship, loops, more selective mid/high control or another supported gesture, use the individual/direct workflow in [transition execution](transitions.md) and [sessions](session.md). Hardware capabilities and ownership still apply.

Follow feedback and prepare the subsequent move at clearance; react to a failed source while a carrier remains. Observing and continuing a set is allowed. Schedule known actions on the native clock, and use events or focused state reads for the next decision rather than letting model response timing drive the faders. Keep an original finish timestamp for a bounded set, including any recovery time.

## When application-owned selection is wanted

```sh
pd mix queue start --playlist spotify:liked --provider spotify --energy-direction keep --duration-seconds 3600 --replace-prepared --request-id SET_UNIQUE_ID --wait-ready --json
```

This delegates the *musical selection and windows* as well as execution. The application does not read the art manual or understand vocal meaning. Tempo/key scores, waveform changes and grid priors are limited evidence; a ready queue is not a guarantee of good phrasing or of a completed hour.

Tempo limits, technique subsets and a consecutive-skip cutoff can be supplied when desired. The planner otherwise uses the engine's supported rate range, ranks musical fit, and treats key and continuation-budget concerns as advice. `max_skips:0` means no arbitrary consecutive-skip cutoff; preparation runway, source availability and bounded runtime history still constrain what can execute.

Acceptance starts preparation. `--wait-ready` consumes startup events and exits when the first successor is armed or startup fails. Resume a yielded CLI handle to consume its result; a timeout does not cancel the queue. Retain the queue ID instead of submitting duplicates.

The application can run after the agent returns when that suits the request. For an observed/performed set, remain involved as useful. `mix.queue.notice` reports an actionable outcome; an external event stream or passive file alone does not wake an idle model. A slow heartbeat can miss the remaining musical runway.

## Failures and the endpoint

A terminal queue may preserve ordinary playback while no longer preparing anything. Treat that as a decision while music remains, not as evidence that the requested duration completed. Inspect the exact rejection and use musical judgment: another source, passage, technique or workflow may work. A repeated identical failed queue is not recovery.

If a gap occurs, report it and establish audible playback from an actually ready entry before rebuilding the transition plan. A stopped outgoing clock cannot launch a future action. Preserve the original endpoint when recovering. Queue stop cancels automation but leaves ordinary playback; a genuine duration endpoint stops playback. Honor an explicit user stop or takeover.

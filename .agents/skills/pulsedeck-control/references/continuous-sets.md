# Continuous sets

PulseDeck owns continuous execution. The agent supplies repertoire and musical policy, starts it, and returns. `pd` below means the checkout's `target/debug/pulsedeckctl` (or the intended configured binary).

## Check and choose one opener

Use one compact status read to identify the instance, currently playing/prepared material and output. For Spotify, check `spotify.native.status`: library login and native deck playback are separate. `restoring:true` means saved authorization is still loading: allow restoration to complete before judging sign-in. `connected:true` with `playback_permission:true` authorizes native preparation; `account:null` is normal for streaming-only authorization and does not mean signed out. Actual sign-in failure is a blocker; report it instead of searching another provider. Respect user-prepared material and preserve healthy playback on takeover.

An indexed opener query supplies a bounded analysed shortlist without Spotify pagination or per-source inspection:

```sh
pd call mix.openers --args '{"playlist":"spotify:liked","provider":"spotify","min_bpm":118,"max_bpm":126,"limit":8}' --json
```

Choose a suitable opener from the returned artist/title, duration, BPM, key and energy. Adjust BPM bounds or add `search` for the requested musical style when needed; these example bounds are for house, not all genres. Metadata does not prove musical quality. If `coverage.complete` is false, run `pd call mix.index_scope --args '{"scope":"spotify:liked"}' --json` once and repeat the opener query. Do not fetch the collection again with `spotify.liked` page loops. Existing complete scope is reusable; refresh only when the user requests fresher membership or evidence shows it changed.

Load the chosen source on a free isolated deck. Both `track` and `uri` accept complete Spotify track URIs. Await this one readiness receipt before Play; combine known sequential commands in one tool invocation where practical. Use the actual chosen source in place of SOURCE_URI:

```sh
pd call deck.load --args '{"deck":"A","track":"SOURCE_URI"}' --timeout 120s --json
pd deck A play --json
```

Loading normalizes loudness independently of trim. Do not multiply master/trim gain to compensate for that correction. A healthy opener needs sufficient music for successor preparation and overlap.

## Submit once and finish

Start immediately after the opener is audible; do not spend its runway browsing possible successors. Collection mode already selects across the indexed analysed pool. An ordered set uses `--sources` instead of collection flags.

```sh
pd mix queue start --playlist spotify:liked --provider spotify --energy-direction keep --max-stretch-percent 2 --duration-seconds 3600 --review-confirmed --replace-prepared --request-id SET_UNIQUE_ID --json
```

Set duration and energy/tempo/technique policy to the user's request. Omit `--techniques` to allow all available techniques. `--review-confirmed` accepts inferred phrase/vocal uncertainty, not a claim of listening. `--replace-prepared` requires authorization to replace a paused preparation.

On `ok:true`, `stage:accepted`, retain the queue/request IDs, report the handoff and **end the turn**. `handoff.agent_can_finish:true` means no ongoing agent work is needed. Do not use `--follow`, `events wait/watch`, `write_stdin`, sleeps or periodic status for a healthy continuous queue. The app advances the set and enforces the endpoint after the agent exits. Watching an empty stream still spends model tokens when a tool times out.

## Actionable outcomes

Native `mix.queue.updated` events serve UI progress; `mix.queue.notice` reports terminal completion or attention. The app shows a persistent outcome notice and retains the final queue summary. External hosts can subscribe to `events.get` with `queue_id` and `types:["mix.queue.notice"]` outside model inference; an event transport alone does not automatically resume a Codex chat. Do not keep the model alive to simulate it.

When the user asks for status or an actionable notice actually arrives, use one `mix queue status --queue-id QUEUE_ID --json` if its evidence is insufficient. A manual takeover yields ownership: do not automatically restart. Stop uses `mix queue stop`; it cancels owned automation while ordinary playback continues. A duration endpoint stops playback; a failed or exhausted queue preserves the carrier and requests attention. Recover from actual state and report any observed gap.

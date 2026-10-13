---
name: pulsedeck-prepare-next-track
description: Prepare an incoming PulseDeck deck through pulsedeckctl while protecting the live deck. Use for loading the next song, matching tempo, setting an entry position, and headphone preparation before a transition; supports planning-only requests.
---

# Prepare the next track

Use this when the user asks “prepare this song on B while A keeps playing”, “get the next track ready in my headphones”, or asks for a compatible next track without beginning a transition. Produce a ready, isolated incoming deck and a verified summary. Loading, cue selection, and preparation are authorized by such a request; starting a master-output transition is a separate action. For planning-only requests, inspect and explain proposed settings without changing controls.

Read [CLI reference](../references/cli.md) for invocation, response stages, source guards, and recovery. Read [musical evidence](../references/musical-evidence.md) when choosing a track or entry point. Use the track-inspection and set-preparation skills when deeper selection is needed.

## Inspect before changing controls

Run discovery and inspect current state:

```sh
pulsedeckctl commands list --json
pulsedeckctl commands describe deck.load --json
pulsedeckctl commands describe engine.command --json
pulsedeckctl commands describe engine.wait --json
pulsedeckctl status --json
pulsedeckctl jobs list --json
```

Record both deck source identities, load revisions, playing/position state, BPM/rate, key lock, sync/master, loop state, headphone cue, gain/EQ/filter/effects, and channel faders. Check live remaining time before starting; leave a margin for load and key-lock preparation, and report when the requested preparation cannot safely finish before the current track ends. Also inspect audio routing, crossfader, master/cue gains and mix, active ramps, and source jobs. Compact status metadata does not replace full analysis when confidence matters; `pulsedeckctl call engine.metadata --args '{}' --json` supplies loaded track analysis and waveforms, so save that potentially large response to a file and inspect relevant fields/ranges.

A paused deck is not automatically available: it may hold a user's prepared next track, a cue preview, a ramp target, or an unfinished load. Respect an explicit destination. If both decks are occupied in an ongoing blend, report the conflict and wait for a free deck rather than replacing either. Never use `--replace-playing` to fulfill ordinary next-track preparation. A stopped deck can begin playing through manual control between observations; native load rejects playing replacement by default, but load is not a general compare-and-swap against every concurrent control change.

Treat the live deck's source/revision, position revision, mixer settings and master role as protected. Its position should advance normally while preparation runs. A source revision guards source identity only; it does not detect manual fader, tempo, or position changes. Inspect again between phases and yield if the observed live state changes unexpectedly.

## Choose an entry and tempo strategy

Favor an existing cue supported by waveform/beatgrid evidence and the intended next transition. Preserve cue metadata unless the user asks for an edit. Use an ordinary seek to position the paused incoming deck; triggering a hot cue starts playback. A clean intro candidate means observed low amplitude, reduced bass, or a recurring instrumental-looking pattern, not proof of an instrumental section or absence of vocals. State uncertainty when no listening occurred.

For manual tempo matching, calculate `100 * (live_effective_bpm / incoming_base_bpm - 1)`, with live effective BPM derived from the live BPM and playback rate. Inspect tempo confidence and half/double-time alternatives first. Do not replace the incoming track's analyzed BPM merely to match another song. Favor modest changes; harmonic compatibility and key lock are useful guides, not proof of a pleasing mix. Key lock changes can trigger preparation; await readiness.

Preserve the live master role. If the live deck is already master and its grid is credible, incoming `sync on` may be appropriate. It also performs phase alignment and can seek the incoming deck, so apply sync before the final entry seek and verify the resulting position. If the current master is different or evidence is weak, keep incoming sync off and match tempo manually. Do not change the live deck's master/sync configuration silently. Beatgrid origin is not a proven downbeat or phrase start; preparation does not establish exact phrase alignment. Scheduled transport is unsupported.

## Execute isolated preparation

1. Before loading, close only the incoming channel fader. Preserve the live fader, crossfader, master gain and routing. A zero incoming channel fader isolates its contribution to the software master mix while headphone cue remains available because the cue bus is pre-fader. The native setter acknowledgment reflects the applied target; audio smoothing settles over rendered frames, so do not start a preview immediately and assume the signal is already zero.
2. Load the selected source without playing replacement. A synchronous load returns `ready` and its new `load_revision`; a `--no-wait` load returns `accepted` with a job ID. Inspect/wait the job, then verify source identity and capture the new revision before issuing preparation controls. Do not keep using the pre-load revision.
3. Apply incoming-only settings under the new source guard: disable stale loop state, choose sync/tempo and key lock, select the final entry position, and enable headphone cue when requested. Neutralize incoming filter/effects or unusual EQ only when preparing the requested clean starting state; explain intentional bass reduction reserved for the transition. Preserve the live settings.
4. Wait for incoming pitch/key-lock preparation, inspect readiness, and leave playback paused unless headphone audition was requested. Audition only after master isolation has settled, routing is actually separate, and cue gain/mix are suitable. Do not start/restart the audio device, remap outputs, mute the global master, or change global cue controls merely to finish a preparation request.
5. Verify the final state against the baseline. If `cue_left`/`cue_right` are null or there is no distinct headphone output, say the deck is cue-enabled but headphone output is not configured; do not call it ready in the headphones. Fixing routing belongs to an authorized setup task. Headphone audibility and musical suitability require listening/hardware verification.

If resetting an incoming filter with Smart CFX enabled also alters its effects, inspect the resultant incoming FX state; never infer a neutral effect from one setter alone. Do not move the crossfader to isolate B: Smart Fader can change both decks' tempo/effects.

### Executable example: prepare local track 42 on B

Run these only after inspection establishes B is available and A is the protected live deck. Replace the track ID and entry time with inspected selections. `pulsedeckctl` must be on PATH; the shared reference explains repository-local invocation and isolated sockets.

```sh
# Stop on a failed command, and keep this operation's snapshots separate.
set -e
PREP_DIR=$(mktemp -d /tmp/pulsedeck-prep.XXXXXX)
# Guard the pre-load B source when silencing its channel.
pulsedeckctl status --json > "$PREP_DIR/before.json"
OLD_B_REV=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"]["engine"]["load_revisions"][1])' "$PREP_DIR/before.json")
pulsedeckctl deck B fader set 0 --expected-load-revision "$OLD_B_REV" --json
pulsedeckctl deck B load --track local:42 --expected-load-revision "$OLD_B_REV" --json > "$PREP_DIR/load.json"
B_REV=$(python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); assert r["ok"] and r["stage"]=="ready"; print(r["result"]["load_revision"])' "$PREP_DIR/load.json")

# Example manual matching: use 0 only when equal tempo is supported by inspection.
pulsedeckctl deck B sync off --expected-load-revision "$B_REV" --json
pulsedeckctl deck B tempo set --percent 0 --expected-load-revision "$B_REV" --json
pulsedeckctl deck B keylock on --expected-load-revision "$B_REV" --json
pulsedeckctl deck B loop off --expected-load-revision "$B_REV" --json
pulsedeckctl deck B seek --at 0s --expected-load-revision "$B_REV" --json
pulsedeckctl deck B headphones on --expected-load-revision "$B_REV" --json
pulsedeckctl deck B wait --expected-load-revision "$B_REV" --timeout 30s --json
pulsedeckctl status --json
```

Check each response before advancing; stop on a failure. These commands deliberately leave B paused. When sync is chosen instead, use `pulsedeckctl deck B sync on --expected-load-revision "$B_REV" --json` before the final seek. Re-read state because sync may change tempo and position. For a specifically requested cue edit, an explicit-time setter such as `pulsedeckctl deck B hotcue H set --at 32s --expected-load-revision "$B_REV" --json` preserves transport; inspect the existing slot and use a cue revision guard through `cues.set` for durable concurrent edits.

A preparation batch may group settings after the successful load. Put `expected_load_revision` on every individual deck step, including `engine.wait`; an outer batch guard does not protect its children. Validate before running, wait its job, and inspect partial results. Do not combine a load and controls with a guessed future revision.

## Verification and response

Confirm incoming source and revision match the selected load; position matches the chosen entry; playback is paused; `preparation_pending` is false; loop state and rate/sync/keylock match the plan; channel fader remains zero; cue is enabled when requested; no unexpected ramps/jobs remain. Confirm live source/revision and position revision match baseline, playback continues and position advances, and its fader/EQ/filter/effects, rate, master role, global crossfader/master gain and routing have not changed unexpectedly.

Return destination/source/revision, entry timestamp and evidence, tempo/key-lock/sync choices, master isolation and cue status, readiness, live-deck preservation, and unresolved listening uncertainty. For example: “B contains the selected track at 8.0s, paused, with its channel fader at zero and headphone cue enabled. A continued advancing with its source and mixer state unchanged. The intro choice comes from waveform evidence; headphone sound and phrase alignment have not been heard.” A planning response instead gives the intended settings and uncertainty and explicitly says they have not been applied.

## Failures and takeover

On load failure, preserve live playback, leave incoming isolated, inspect the source job and error, and report whether the previous incoming source survived. Retry only the identified failure with a bounded attempt; use another eligible track when authorized. Never wait indefinitely while the live track approaches its end.

On timeout, inspect state/jobs first: timeout is not proof of cancellation. `pulsedeckctl jobs cancel JOB_ID --json` stops future batch steps and interrupts engine waits; an active provider load also needs `pulsedeckctl deck B load cancel --json`, followed by state verification. Cancellation does not undo already-applied controls. On a stale source guard, unexpected manual changes, or a takeover request, stop issuing preparation controls and re-inspect. Do not “restore” the user's new state to a cached baseline. Use `pulsedeckctl automation stop --json` for an explicit automation-stop request; it cancels ramps/jobs and releases cue/jog state without pausing all playback. Re-read jobs, ramps and decks to report what actually stopped and what is still active.

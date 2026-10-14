---
name: pulsedeck-session-dj
description: Run a requested PulseDeck playlist or practice set through repeated track selection, isolated preparation, musically reasoned transitions, and verification using pulsedeckctl. Use for continuous DJ sessions and stopping their automation; use set preparation for planning a playlist without playback.
---

# Run a DJ session

Use for “play this playlist and handle the transitions”, “run a 30-minute practice set with long blends and minimal effects”, or “stop automation; I’m taking over”. Operate through the existing shell CLI; the AI chooses musical strategy and PulseDeck executes it. A request to run the set authorizes the necessary preparations and transitions within its stated scope. Do not ask again before each already-authorized step. A request to explain or plan first authorizes observations and a plan, not playback or mixer changes.

Read [CLI reference](../references/cli.md) to bind the correct runtime and understand guards, stages, ramps, jobs, errors, and cancellation. Read [musical evidence](../references/musical-evidence.md) when choosing tracks and transition regions. Use [set preparation](../pulsedeck-set-preparation/SKILL.md), [track inspection](../pulsedeck-track-inspection/SKILL.md), [incoming preparation](../pulsedeck-prepare-next-track/SKILL.md), and [transition](../pulsedeck-transition/SKILL.md) for their detailed procedures as needed. This skill owns the repeated session loop, timing budget, recovery, and handoff.

## Establish intent and observe the session

Resolve the requested playlist/order, duration, mixing style, effects preference, recording intent, and end behavior from the request and current session. Clarify a genuinely missing essential choice while continuing read-only inspection. Preserve explicit track order unless selection/reordering is authorized. Recording and library/cue edits are separate intentions; running a playlist does not require either.

With `pd` bound as described in the CLI reference:

```sh
pd commands list
pd commands describe track.inspect
pd commands describe deck.load
pd commands describe engine.wait
pd commands describe automation.ramp
pd status
pd playlist list
pd library tracks --limit 100 --offset 0
pd jobs list
```

Follow pagination through `next_offset` when relevant. Reconstruct the saved order from `playlist.list`'s `track_ids`; a playlist-filtered library query is not a promised ordered queue. Store identities and requested order in session notes, not a hardcoded decision script.

Capture audio/routing, recording restrictions, both sources and load revisions, transport/position revisions, BPM/rate/confidence, cues, preparation flags, loops, EQ/fader/filter/effects, sync/master, crossfader, and active ramps/source jobs. `status` is compact; `track.inspect` supplies bounded metadata/cue/waveform detail without altering playback. Verify whether an existing playing deck belongs to this request before interrupting it. A paused deck may hold a user's prepared track; it is not automatically disposable.

Estimate time honestly. Source duration sum differs from wall-clock set duration after entry/exit trims, overlaps, playback rate, and loops. For each ordinary unlooped live deck, remaining wall seconds are approximately `(duration_seconds-position_seconds)/playback_rate`; an active loop invalidates a simple end estimate. Track actual elapsed time separately. A requested 30 minutes is a target requiring ongoing decisions, not a guarantee derived from file lengths.

## Keep elapsed time in a stable clock domain

At the first playback acknowledgment, save a single performance baseline: instance, UTC wall timestamp, `engine.rendered_frames` and `engine.sample_rate`. Do not start the duration budget when merely reading skills or loading tracks. With audio continuously running at the same sample rate/instance, `(current_rendered_frames - baseline_rendered_frames) / sample_rate` measures rendered playback elapsed time across CLI calls. Check those continuity conditions; device stops/restarts or rate changes require reconciling intervals with wall time. UTC timestamps provide an independent wall-clock check, with ordinary system-clock adjustment caveats.

Do not carry process/session-local monotonic readings between separate shell executions unless you verified they share an origin in this environment. Separate exec sessions can expose different monotonic origins, so an apparently plausible phase estimate can substantially understate actual playback elapsed. A monotonic clock inside one continuously running observer is useful; otherwise use the persisted frame/UTC baseline. Compare elapsed against transport positions/rates and position revisions to detect an implausible time estimate.

Before every timed phase, recompute elapsed from the same baseline and inspect current positions. When a phase is late, reassess the passage and remaining session budget; skip an expired action or simplify rather than executing a stale schedule. Trigger an approximate intervention from current observed position/window evidence, not an unchecked old timer. Leave time for CLI/observation latency and final verification. At the requested endpoint stop future automation promptly; pause decks if the user's end behavior asks for stopped playback, because `automation stop` alone leaves playback running. Report actual duration/overrun and omitted tracks rather than filling the intended plan into the result.

## Inspect → select → prepare → transition → verify → repeat

1. **Inspect ahead.** While the live deck continues, inspect the next candidate's overview and detailed ranges near likely entry/exit regions. Use metadata/confidence, existing cues, bass/level/texture changes and recurring patterns. Distinguish observed features from inferred sections; no listening claim from JSON. Beatgrid origin is not a proven downbeat, and 8/16/32-bar patterns are candidates rather than universal rules.
2. **Select within intent.** Compare tempo-change size, key evidence and plausible phrase/texture compatibility. Favor a manageable successor and explain transition difficulty. Harmonic matches are guides; inferred waveform level does not establish perceptual energy or mood. Preserve requested order, or report an obstacle rather than silently replacing a requested song.
3. **Prepare early and inaudibly.** Budget incoming load, pitch/key-lock preparation, inspection, transition duration and recovery margin before the outgoing track's chosen exit. Base the margin on observed load times and uncertainty; do not wait until the last bar. Identify the available deck, close only its channel fader, load without `--replace-playing`, wait for `ready`, capture the returned source/revision, and apply guarded incoming controls. Choose the entry from existing cues/evidence, isolate master contribution, set tempo/key lock/sync/headphone cue as appropriate, and wait for preparation. Do not globally mute the master or change routing merely to prepare a song. In silent trials, both master and cue gain are deliberately zero.
4. **Plan the overlap.** State outgoing/incoming identities and revisions, timestamps, provisional meter/phrase assumptions, overlap in bars/beats/seconds, bass exchange, fader moves and purposeful optional effects. Count phrase progression on both tracks. Choose a shorter fade/cut when confidence, drift or competing density make a long blend unreliable; a loop needs a verified rhythmic region and an explicit exit. Do not invent richer analysis until a concrete observed mixing failure shows a need.
5. **Execute and monitor.** Re-observe immediately before transport/fader intervention. Native timed ramps advance on rendered audio frames; audio must run. Use guarded deck operations and named/global ramp controls appropriate to the plan. An `applied` ramp is not completed; wait/inspect its target and `active_ramps`. Avoid one huge unattended batch: it cannot react to changing evidence, manual controls, a failed load, or the next track ending. Short validated batches may group known steps, with a source guard on each deck step; an outer batch guard does not protect children.
6. **Verify the handover.** Confirm incoming source/revision, position progression, preparation complete, final fader/EQ/filter/effects, old-deck contribution closed, no unintended jobs/ramps and acceptable software meters. Only then pause the outgoing deck if that is part of the plan; do not unload a track still contributing to the mix. Record actual overlap/elapsed time and any uncertainty. Release the old deck for the following preparation, reassess remaining time and repeat until the requested endpoint.

No native scheduled transport exists. Polling or shell sleeps can approximate an intervention window but cannot promise an exact beat/downbeat/phrase start. Ramp quantization applies to the ramp, not the separate play command. Prefer bounded immediate execution when exact timing cannot be delivered; describe the limit before claiming a phrase-perfect result.

Include command and reasoning latency in each overlap budget. Native ramps control their own rendered duration, but gaps between starting ramps, observing results and choosing the next stage lengthen the whole transition. A blind run's intended 32-second overlap took about 53 seconds even after its clock was corrected. For already-decided controls, a short source-guarded batch can launch concurrent targets and wait a known phase without a model/tool round trip between every knob. Keep observations between bounded phases and verify endpoints before outgoing pause; don't replace the session with one blind script. Reserve time for final cancellation/pause acknowledgments, and don't begin optional preparation when the requested endpoint is already near. Report actual overlap length separately from the nominal ramp durations.

During a session, monitor compact state at a cadence suitable to remaining time rather than dumping full analysis repeatedly:

```sh
pd state watch --count 4 --interval 250ms
pd jobs list
pd status
```

Look for position discontinuities (`position_revision`), source changes (`load_revisions`), preparation/source jobs, stopped audio, unexpected loops, manual target movement, or changed sync/master. Guards protect source identity, not every knob or seek. Native ramps may cancel on clock pause/seek/source change; audio stop freezes progress. Mid-ramp tempo changes require reassessment. A manually moved target takes over that ramp; do not fight the user by replacing it.

## Executable example: one bounded handover within a session

This illustrates the execution phase after selection/preparation, not a universal musical prescription. Bind `pd` first. Preconditions established by inspection: A is the authorized live track, B is the selected prepared incoming track at its chosen entry, audio runs, A's channel is open, B's channel is zero, crossfader is centered, both sources are current, Smart Fader/CFX behavior is understood, and the chosen passage supports a short minimal-effects fade. Preserve global levels/routing and existing cues. For an actual 16-bar blend, calculate seconds from inspected effective BPM and declared meter rather than copying this two-second trial duration.

```sh
SESSION_DIR=$(mktemp -d /tmp/pulsedeck-session.XXXXXX)
pd status > "$SESSION_DIR/before.json"
A_REV=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"]["engine"]["load_revisions"][0])' "$SESSION_DIR/before.json")
B_REV=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"]["engine"]["load_revisions"][1])' "$SESSION_DIR/before.json")
# Check every response; abort this phase on failure instead of continuing.
pd deck B wait --expected-load-revision "$B_REV" --timeout 30s &&
pd deck B play --expected-load-revision "$B_REV" &&
pd call automation.ramp --args '{"target":{"type":"channel_fader","deck":"B"},"value":1,"seconds":2}' --expected-load-revision "$B_REV" &&
pd call automation.ramp --args '{"target":{"type":"channel_fader","deck":"A"},"value":0,"seconds":2}' --expected-load-revision "$A_REV" &&
pd call engine.wait --args '{"ramps":true}' --timeout 10s &&
pd status > "$SESSION_DIR/after.json"
```

Read the final snapshot and assert both identities/revisions, B playing and advancing, A fader zero, B fader one, preparation complete and all intended ramps finished. Do not blindly run the following pause if any assertion or earlier command failed:

```sh
pd deck A pause --expected-load-revision "$A_REV"
pd status
```

This fades channel contribution only. It does not promise equal perceived loudness, a bass exchange, phrase alignment or a complete long-blend strategy. Select EQ/bass changes using the transition skill and current evidence.

**Planning example:** for “explain the 30-minute set first”, run only discovery, playlist/library reads, `status`, `jobs list`, and relevant `track.inspect` queries. Return the ordered sources, likely overlaps, total source duration versus conditional set estimate, cue/phrase evidence, fallback choices and unknowns. Explicitly say no transport/mixer mutations were applied and wait for the requested execution instruction.

## Failure recovery and manual takeover

- **Failed incoming load/preparation:** inspect the specific error, `jobs get JOB_ID`, source jobs and incoming state. Keep the healthy live deck playing and the failed incoming channel isolated. A failed job stops later steps but does not roll back earlier changes. Select an authorized alternate or a simpler transition within the remaining budget; retry only a diagnosed condition with a bounded attempt. Do not indefinitely wait for a failed job while the live song ends. If no viable incoming source exists, report the risk promptly; a supported loop is an option only if its region/grid is credible and extending the track is within scope.
- **Timeout/uncertain mutation:** read state/job before retrying. An operation may have applied despite a timed-out reply. Reuse the same request ID for an identical uncertain mutation when appropriate; repeated toggle/play/cue mutations with fresh IDs are not harmless. Correct invalid syntax using error details and `commands describe`.
- **Stale source or unexplained manual change:** stop issuing this plan's controls, re-inspect and yield. Do not reload/reset the user's new source or knobs to cached values. Report what completed and what is abandoned; no automatic resume after takeover.
- **Requested stop:** issue `automation stop` promptly, then verify its actual effect. It cancels jobs, native ramps and cue/jog holds; it does not generally pause playback, stop recording or restore prior knobs. Job cancellation prevents future steps and interrupts engine waits, but an active load/import/provider operation may continue. Identify any active load and use its explicit `deck.load.cancel` capability when cancellation is requested; verify the source job afterward. Report persistent work that cannot be cancelled.

```sh
pd automation stop
pd jobs list
pd status
# When an observed B load belongs to the stopped session and is still active:
pd deck B load cancel
pd status
# Inspect a specific returned job, including failed/cancelled step results:
# Set SESSION_JOB to the actual job ID from an accepted response.
pd jobs get "$SESSION_JOB"
```

`jobs wait` returns a nonzero exit for failure/cancellation; `jobs get` retrieves that outcome for diagnosis. `automation stop` acknowledging `applied` is not enough: observe ramps false and job terminal states before saying automation has stopped. A healthy ordinary playing deck should continue after takeover unless the user also asked to pause/stop songs. A cancelled ramp leaves its target at the value reached so far: inspect and report that partial EQ/fader/filter setting instead of implying the original or intended final value was restored. Do not add a fade, pause, or knob reset merely because automation stopped.

## Expected progress and completion report

Report current track/deck/source revision, next prepared source/entry/readiness, actual elapsed and remaining budget, completed transitions, pending jobs/ramps, adapted decisions, and incomplete actions. At requested completion, verify final playback/recording behavior agrees with the user's intent rather than assuming “stop session” means stop every service. Distinguish software-state evidence from listening/hardware evidence: silent/generated fixtures verify source guards, readiness, transport progression, ramps, cancellation and continuity. Timbre, bass balance, vocal clashes, perceived loudness, timing feel and output/headphone sound still require audible verification.

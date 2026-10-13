---
name: pulsedeck-transition
description: Plan or execute a musically reasoned two-track PulseDeck transition through pulsedeckctl, including entry/exit evidence, phrase assumptions, bass exchange, native ramps, and completion checks. Use for blending or cutting between decks; planning-only requests remain read-only.
---

# Transition between two tracks

Use for requests such as “plan a smooth 16-bar transition”, “mix from A to B now with a gradual bass exchange”, or “use a short transition with minimal effects”. “Explain first and wait” authorizes inspection and a plan, not playback or mixer changes. An execution request authorizes its necessary preparation and ordered controls; do not ask again for each already-authorized step.

Read [CLI reference](../references/cli.md) for the `pd` shell function, endpoints, response stages, source guards, jobs, and takeover. Read [musical evidence](../references/musical-evidence.md) when interpreting analysis. Use [next-track preparation](../pulsedeck-prepare-next-track/SKILL.md) when loading or isolating the incoming deck, and [track inspection](../pulsedeck-track-inspection/SKILL.md) for deeper entry/exit evidence. PulseDeck executes controls; choose the musical strategy from observed evidence rather than assuming a genre formula.

## Observe and propose

```sh
pd commands list
pd commands describe track.inspect
pd commands describe engine.command
pd commands describe automation.ramp
pd commands describe engine.wait
pd status
pd jobs list
pd call track.inspect --args '{"deck":"A","points":64}'
pd call track.inspect --args '{"deck":"B","points":64}'
```

Inspect both source identities and load revisions, playing state, positions and position revisions, remaining time, analyzed BPM/key/confidence, beatgrid origin, existing cues and track duration. Capture actual playback rate, master/sync, preparation, loops, faders, gains, EQ, filter/FX, crossfader, active ramps, audio running/routing, and source jobs. Save large responses to temporary files. If B is empty, inspect its selected source before loading; do not substitute a demo or replace a playing deck to make the plan convenient.

Inspect detailed ranges around candidate regions with actual track-bounded timestamps. For example, on a track longer than 80 seconds:

```sh
pd call track.inspect --args '{"deck":"A","start_seconds":48,"end_seconds":80,"points":128}'
pd call track.inspect --args '{"deck":"B","start_seconds":0,"end_seconds":32,"points":128}'
```

Look for sustained bass reduction, amplitude/texture changes, plausible silences, and recurring changes across neighboring ranges. Separate these observations from hypotheses such as “likely outro” or “possible phrase boundary”. Band waveforms do not prove vocals or section names. State that listening has not occurred when the evidence is JSON only. Prefer existing cues; editing cue metadata requires an edit request, and explicit-time setters preserve playback.

Choose a long blend only when both passages plausibly sustain rhythm and their densities/key/tempo make overlap manageable. Shorten or simplify when grids drift, tempo confidence is weak, both passages have strong competing bass, or phrase evidence is ambiguous. A supported loop can extend a short exit if its grid and loop boundary are credible; declare the loop and its extra runtime, verify it is active, and release it deliberately. Effects are optional: identify their purpose, keep depth restrained, and specify final settings. Do not add effects merely because controls exist.

Declare meter, phrase and tempo assumptions. With provisional 4/4, 16 bars = 64 beats; at 120 effective BPM that is 32 seconds of wall time. Effective BPM is analyzed/base BPM times playback rate. Incoming source time advances by its playback rate during that wall time. Count progression from both chosen entry/exit points; aligned beats do not guarantee aligned phrases. Grid origin is neither a proven downbeat nor a phrase start. Check nearby 8/16/32-bar candidates without imposing them universally. Follow density changes through the overlap, not just at its start: an incoming sparse opening may regain bass halfway through a long blend, while the outgoing bass may disappear near its end. Leave a margin after the planned fade for control latency and verification rather than finishing exactly at the source end. Do not overwrite analyzed BPM to perform tempo matching.

Return a concrete plan before execution when requested:

- Sources, deck load revisions and cue revisions; protected live state.
- Outgoing intervention region and incoming entry time, observations, and uncertainty.
- BPM/rate/key strategy, provisional meter and phrase alignment, and alternative if evidence is weak.
- Overlap in bars, beats and seconds, with each track's expected source-time progression; ensure enough remaining audio.
- Ordered preparation, starts, bass exchange, fader stages, optional purposeful effects/loops, and their final values.
- Completion checks and listening/hardware checks that cannot be proved from software state.

For example: “Assuming four beats/bar and a credible 120 BPM grid, blend 16 bars/64 beats/32 seconds. A's observed bass reduction near 80s and B's sparse opening are candidates, not confirmed outro/intro labels. Starting A's overlap near 60s and B at 0s means they reach about 92s and 31s of source audio if B's rate is 120/124. B may regain bass near 16s, so exchange bass before competing low ends accumulate, finish with B audible and A isolated, leave remaining-time margin, verify, then pause A. Actual start timing and phrase feel need listening.” Substitute actual evidence and controls; this example is not an executable prescription for arbitrary tracks.

## Execute and verify each phase

Prepare the incoming deck before opening it on master: close its channel fader, load without `--replace-playing`, capture the returned new revision, apply guarded tempo/key-lock/sync settings, turn off unwanted old loops, seek the chosen entry, and wait preparation. Protect the outgoing deck. When using sync, it can seek the incoming track; perform final entry positioning afterward. Preserve cues and protected controls. Do not mute the global master to prepare a real performance; muted master and cue gain are specifically for isolated generated-fixture trials.

Re-observe both decks immediately before starting. Yield on an unexplained source, position revision, tempo, master, or mixer change. Source guards do not protect against manual knob/seek changes. Verify no conflicting ramps; do not silently cancel automation belonging to another operation. Native ramps bypass Smart Fader/CFX compound behavior. Plan with that fact, and avoid silently switching smart modes or moving a crossfader that would unexpectedly change both decks.

Scheduled transport is unsupported. An immediate `deck B play` followed by ramps cannot promise an exact downbeat or phrase start. Shell polling/sleeps can estimate an intervention window only. Ramp quantization applies to that ramp on the declared clock grid, not to the separate play command. If the requested precision cannot be achieved, explain the limitation and choose an authorized approximate intervention or leave a plan for manual timing.

Use native ramps for smooth fader/EQ/filter changes. A seconds ramp needs rendered audio; a beat-clock ramp also needs its clock deck playing. It uses the clock's BPM/grid at start, cancels on clock pause/seek/source replacement, and freezes when rendering stops. Avoid changing clock tempo mid-ramp. For a ramp targeting B but clocked on A, the single source guard protects the target source; separately re-observe A's identity/position. Prefer short guarded phases and observations over a long blind batch.

### Executable short blend example

This illustrates mechanics for an authorized A-to-B blend, with A playing and B already prepared/paused at its chosen entry. It assumes the inspected crossfader permits both channels, fader/gain targets are appropriate, B can contribute without clipping, and there is sufficient remaining audio. Use `pd` from the shared reference. It is a two-second overlap example, not a claimed phrase-timed transition. For a longer plan, derive durations from observed effective BPM and adapt the stages.

```sh
set -e
TRANSITION_DIR=$(mktemp -d /tmp/pulsedeck-transition.XXXXXX)
pd status > "$TRANSITION_DIR/before.json"
A_REV=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"]["engine"]["load_revisions"][0])' "$TRANSITION_DIR/before.json")
B_REV=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"]["engine"]["load_revisions"][1])' "$TRANSITION_DIR/before.json")
pd deck B wait --expected-load-revision "$B_REV" --timeout 30s
pd deck B eq set --low 0 --expected-load-revision "$B_REV"
pd deck B play --expected-load-revision "$B_REV"
# Initial short fade-in; the following ramps run natively, not via knob polling.
pd call automation.ramp --args '{"target":{"type":"channel_fader","deck":"B"},"value":1,"seconds":0.5}' --expected-load-revision "$B_REV"
pd call engine.wait --args '{"ramps":true}' --timeout 5s
pd status > "$TRANSITION_DIR/overlap.json"
# Check identities, normal position advancement, playing state and manual changes here.
pd call automation.ramp --args '{"target":{"type":"eq_low","deck":"A"},"value":0,"seconds":1}' --expected-load-revision "$A_REV"
pd call automation.ramp --args '{"target":{"type":"eq_low","deck":"B"},"value":1,"seconds":1}' --expected-load-revision "$B_REV"
pd call engine.wait --args '{"ramps":true}' --timeout 5s
pd status > "$TRANSITION_DIR/bass-exchanged.json"
pd call automation.ramp --args '{"target":{"type":"channel_fader","deck":"A"},"value":0,"seconds":0.5}' --expected-load-revision "$A_REV"
pd call engine.wait --args '{"ramps":true}' --timeout 5s
pd status > "$TRANSITION_DIR/completed.json"
# Verify B still plays, ready at the intended source/rate, B fader and low EQ=1,
# A fader=0, no active ramps, unchanged source revisions and no unexpected seek.
# Only after inspecting the completed snapshot:
pd deck A pause --expected-load-revision "$A_REV"
pd status
```

Do not paste through observation comments without inspecting the recorded snapshots. A valid `applied` acknowledgment is an engine fence, not completion of a timed ramp. `engine.wait` with `ramps:true` waits all active ramps; afterward inspect endpoints because a cancelled ramp also ceases being active. If the clock pauses and the wait returns, a ramp may have cancelled at an intermediate value; neither successful wait nor inactive ramps alone proves the requested endpoint. At each phase compare position revisions, identities, mixer targets and unexpected jobs. Commands are sequential shell calls, so the nominal overlap gains IPC/observation latency; report measured elapsed time rather than claiming exactly two seconds.

For an inspected four-beat clock and a requested sixteen-beat fader ramp, this syntax is supported when A is playing:

```sh
pd call automation.ramp --args '{"target":{"type":"channel_fader","deck":"B"},"value":1,"beats":16,"clock":"A"}' --expected-load-revision "$B_REV"
pd call engine.wait --args '{"ramps":true}' --timeout 30s
```

Choose seconds or beats deliberately. If adding `"quantize":4`, describe the next four-beat grid boundary only; do not claim it establishes a musical bar/downbeat. Keep outgoing playback running while it supplies the beat clock. Put individual guards on every deck step in a batch, validate it before run, inspect its accepted job, and wait/check partial results. Batches do not roll back previous actions or establish scheduled transport.

## Completion, failures and takeover

Before pausing or unloading the outgoing deck, verify incoming source/revision, normal advancing position, playback/rate, no preparation pending, planned final fader/EQ/filter/FX, and intended master audibility route; verify outgoing isolation and absence of active ramps. Check remaining loop/cue/sync/master states explicitly. A zero software fader target plus `applied` does not instantly settle the short native smoothing; rendered ramp completion provides a clearer endpoint. Give sources, actual action results, timings and remaining uncertainty, not just “transition succeeded”. Listening still must judge bass balance, loudness, vocal/phrase clashes, artifacts and musical feel.

On invalid commands, follow discovery and structured error corrections. On timeout, inspect state/job before retrying; an operation may have applied. On source conflict or unexpected manual movement, stop issuing the next phase, inspect, and yield rather than restoring a cached user state. Preserve the continuing audible deck during a failed load or preparation; don't wait indefinitely near its end. If a blend has partly applied, inspect what is currently playing/audible and select a simple supported stabilization within the authorized task instead of replaying the whole batch.

`pd jobs cancel JOB_ID` stops future job steps and interrupts engine waits; it does not undo active ramps or universally cancel loads/imports/provider operations. On requested takeover use `pd automation stop`, then `pd jobs list` and `pd status`. It cancels ramps/jobs and releases cue/jog holds, but leaves playback/recording and current knob values in place; operation-specific cancellation may still be needed. Manual movement of a ramped control cancels that target. Report what actually stopped, any active operation, and any incomplete handoff; never fight the user's new control state.

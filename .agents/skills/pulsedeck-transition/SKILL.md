---
name: pulsedeck-transition
description: Plan or execute a musically reasoned two-track PulseDeck transition through pulsedeckctl, including entry/exit evidence, phrase assumptions, bass exchange, native ramps, and completion checks. Use for blending or cutting between decks; planning-only requests remain read-only.
---

# Transition between two tracks

Use for requests such as “plan a smooth 16-bar transition”, “mix from A to B now with a gradual bass exchange”, or “use a short transition with minimal effects”. “Explain first and wait” authorizes inspection and a plan, not playback or mixer changes. An execution request authorizes its necessary preparation and ordered controls; do not ask again for each already-authorized step.

Read [CLI reference](../references/cli.md) for the `pd` shell function, endpoints, response stages, source guards, jobs, and takeover. Read [musical evidence](../references/musical-evidence.md) when interpreting analysis. Use [next-track preparation](../pulsedeck-prepare-next-track/SKILL.md) when loading or isolating the incoming deck, and [track inspection](../pulsedeck-track-inspection/SKILL.md) for deeper entry/exit evidence. PulseDeck executes controls; choose the musical strategy from observed evidence rather than assuming a genre formula.

Read and apply [DJ musicianship](../references/dj-musicianship.md) before a musical transition. Record the protected outgoing payoff, selected A handover, B entry and arrival, actual lead beats, launch timestamp, bar phase, musical roles and tempo/key strategy. Plan low, mid and high contributions through the overlap: transfer competing bands as incoming content arrives, preserve body and air, and verify the final incoming tone. A bass swap alone does not establish clarity or fullness. Prioritize compatible keys when the user requests harmonic mixing. Match the user's requested pace without reducing every mix to a short fader gesture. An immediate cut request can use the next suitable available boundary; an ordinary phrase mix needs the planned future event.

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

Inspect both source identities and load revisions, playing state, positions and position revisions, remaining time, BPM/key/provenance, actual beat/downbeat events, grid origin/manual override, `rhythm_error`, cues and duration. Read both native `beat_position` values from one snapshot. Capture rate, master/sync, preparation, loops, faders, EQ/filter/FX, crossfader, active ramps/launches, routing, and source jobs. Save large responses to files. Full-model scores are uncalibrated; an analysis failure/warning requires manual validation before an automated beatmix. If B is empty, inspect its selected source; do not substitute a demo or replace a playing deck for convenience.

Default Beat This! decoding applies native DBN constraints for one whole-track meter of three, four, five or seven beats per bar over 55–300 BPM; check its predicted bar phase against the passage. Offset-only alignment retains neural timestamp intervals. A manual BPM/half/double correction instead selects a whole-track constant-tempo grid; inspect loaded `beatgrid_uniform` and `beat_events.uniform`, not original neural counts, for the effective clock. Manual bar origin retains detected meter, with four as the unknown-meter fallback. Neither mode establishes phrase or vocal compatibility.

Inspect detailed ranges around candidate regions with actual track-bounded timestamps. For example, on a track longer than 80 seconds:

```sh
pd call track.inspect --args '{"deck":"A","start_seconds":48,"end_seconds":80,"points":128}'
pd call track.inspect --args '{"deck":"B","start_seconds":0,"end_seconds":32,"points":128}'
```

Look for sustained bass reduction, amplitude/texture changes, plausible silences, and recurring changes across neighboring ranges. Separate these observations from hypotheses such as “likely outro” or “possible phrase boundary”. Band waveforms do not prove vocals or section names. State that listening has not occurred when the evidence is JSON only. Prefer existing cues; editing cue metadata requires an edit request, and explicit-time setters preserve playback.

Choose a long blend only when both passages plausibly sustain rhythm and their densities/key/tempo make overlap manageable. Shorten or simplify when grids drift, tempo confidence is weak, both passages have strong competing bass, or phrase evidence is ambiguous. A supported loop can extend a short exit if its grid and loop boundary are credible; declare the loop and its extra runtime, verify it is active, and release it deliberately. Effects are optional: identify their purpose, keep depth restrained, and specify final settings. Do not add effects merely because controls exist.

Declare meter, phrase and tempo assumptions. With provisional 4/4, 16 bars = 64 beats; at steady 120 effective BPM that is 32 wall seconds. Full-model grids preserve variable beat spacing, so count actual events and use their timestamp differences/playback rate for a variable-tempo passage. Effective BPM is base BPM times rate, not a replacement for the event sequence. Count progression from both entry/exit points; aligned beats and predicted bar starts do not establish aligned arrangements. Check nearby 8/16/32-bar candidates without imposing them universally. Follow density through the overlap: a sparse opening may regain bass halfway through, while outgoing bass may disappear near its end. Leave a margin for verification. Do not overwrite analyzed BPM to perform tempo matching.

Return a concrete plan before execution when requested:

- Sources, deck load revisions and cue revisions; protected live state.
- Outgoing intervention region and incoming entry time, observations, and uncertainty.
- BPM/rate/key strategy, provisional meter and phrase alignment, and alternative if evidence is weak.
- Overlap in bars, beats and seconds, with each track's expected source-time progression; ensure enough remaining audio.
- Ordered preparation, starts, low/mid/high exchanges, fader stages, optional purposeful effects/loops, and their final values.
- Completion checks and listening/hardware checks that cannot be proved from software state.

For example: “Assuming four beats/bar and a credible 120 BPM grid, blend 16 bars/64 beats/32 seconds. A's observed bass reduction near 80s and B's sparse opening are candidates, not confirmed outro/intro labels. Starting A's overlap near 60s and B at 0s means they reach about 92s and 31s of source audio if B's rate is 120/124. B may regain bass near 16s, so exchange bass before competing low ends accumulate, finish with B audible and A isolated, leave remaining-time margin, verify, then pause A. Actual start timing and phrase feel need listening.” Substitute actual evidence and controls; this example is not an executable prescription for arbitrary tracks.

## Execute and verify each phase

Before an ordinary beatmix, apply the shared reference's [phrase planning and launch verification](../references/musical-evidence.md#phrase-planning-and-launch-verification): name B's arrival event, count backward to its entry and A's launch boundary, then verify beat phase while B is isolated. Tempo matching plus an immediate unsynchronized play from 0:00 is insufficient. Choose overlap lengths from the tracks' musical progression, rather than selecting a convenient number of seconds first.

Do not open B merely because its launch completed and the fractional phases match. Verify bar phase against credible downbeat origins and verify the planned section arrival remains ahead. Include enough isolated lead for these checks. B advances during tool/model calls; a late observation can consume the intro. If the audible opening or arrival is missed, choose the next viable event or reprepare B while isolated. Do not silently redefine the musical target around the delayed command.

Prepare the incoming deck before opening it on master: close its channel fader, load without `--replace-playing`, capture the returned new revision, apply guarded tempo/key-lock/sync settings, turn off unwanted old loops, seek the chosen entry, and wait preparation. Protect the outgoing deck. Current native Sync preserves a paused entry but can seek a playing deck's phase: settle sync and final entry while isolated and paused, then verify before arming the launch. Preserve cues and protected controls. Do not mute the global master to prepare a real performance; muted master and cue gain are specifically for isolated generated-fixture trials.

Re-observe both decks immediately before starting. Yield on an unexplained source, position revision, tempo, master, or mixer change. Source guards do not protect against manual knob/seek changes. Verify no conflicting ramps; do not silently cancel automation belonging to another operation. Native ramps bypass Smart Fader/CFX compound behavior. Plan with that fact, and avoid silently switching smart modes or moving a crossfader that would unexpectedly change both decks.

For an exact future grid timestamp, use native `deck launch` from the CLI reference; ordinary unsynced play remains immediate. An immediate `deck B play` followed by ramps cannot promise an exact downbeat or phrase start. Shell polling/sleeps can estimate an intervention window only. Ramp quantization applies to that ramp on the declared clock grid, not to the separate play command. If the requested precision cannot be achieved, explain the limitation and choose an authorized approximate intervention or leave a plan for manual timing.

Use native ramps for smooth fader/EQ/filter changes. A seconds ramp needs rendered audio; a beat-clock ramp also needs its clock deck playing. It uses the clock's BPM/grid at start, cancels on clock pause/seek/source replacement, and freezes when rendering stops. Avoid changing clock tempo mid-ramp. For a ramp targeting B but clocked on A, the single source guard protects the target source; separately re-observe A's identity/position. Prefer short guarded phases and observations over a long blind batch.

### Execution surfaces after the musical plan

Do not use a generic next-beat/two-second recipe as the live musical default. The selected launch timestamp, entry, control durations and arrival belong to the handover plan. Discover/validate each command and inspect its response. These are separate command shapes, not a complete routine to paste through:

```sh
# Bind these from observed sources and the chosen future A grid event.
# CLOCK_LOAD_REV is A's load revision, never A's position_revision.
pd deck B launch --clock A --clock-revision "$CLOCK_LOAD_REV" --at "$A_LAUNCH_SECONDS" --expected-load-revision "$B_LOAD_REV"
# After the queued launch, wait and inspect B playing, source guards,
# readiness, beat AND bar phase, and remaining entry-to-arrival lead.
pd call engine.wait --args '{"deck":"B","launches":true}' --expected-load-revision "$B_LOAD_REV" --timeout 10s
pd status
```

A launch acknowledgement means queued, not audible or complete. Keep B's channel closed until the planned opening and alignment checks; ensure the checking lead does not consume B's intended entry. If the current tooling cannot coordinate a very short reveal after verification, choose a wider musical window instead of promising exact simultaneous cuts.

For already-planned fader, EQ or filter changes, build guarded `automation.ramp` requests with durations in beats from the selected passage. A supported shape is `{"target":{"type":"channel_fader","deck":"B"},"value":1,"beats":N,"clock":"A"}` where N is the actual chosen duration, not a default. Value/target and bass ownership depend on the plan. Seconds-based ramps remain useful for a deliberately brief cut/smoothing gesture; they do not establish phrasing. Do not reuse one duration for every genre or pair.

Submit coordinated targets in a validated short batch when appropriate, with source guards on each deck step. Quantization is per ramp; calls submitted across a boundary can start on different boundaries. Explicit native launch timing does not also schedule the faders. Verify the installed timing surfaces can deliver the selected routine; separate shell/model calls cannot promise sample-accurate choreography.

Observe between bounded musical phases. Wait for batch dispatch to complete and inspect all step results before waiting for its ramps; a ramp wait cannot cover future batch steps. `engine.wait` with `ramps:true` waits active ramps, but cancellation also makes a ramp inactive. Inspect endpoints, source/position revisions, actual arrivals and final audibility before pausing A. A paused/seeked clock can cancel its ramps partway through. Measure total audible overlap, including command/observation gaps, rather than summing only nominal ramp durations. For automated handovers, apply [timed execution](../references/timed-execution.md): each sequential phase must finish with observation/dispatch margin before the next arming deadline, and an audible partial failure needs an immediate prepared stabilization.

Use effects only after checking the [PulseDeck effect signal path](../references/dj-musicianship.md#pulsedeck-effect-limits). The current channel fader attenuates deck echo/reverb as well as dry audio; turning echo on then closing the channel does not produce an independently sustained post-fader echo-out.

## Completion, failures and takeover

Before pausing or unloading the outgoing deck, verify incoming source/revision, normal advancing position, playback/rate, no preparation pending, planned final fader/EQ/filter/FX, and intended master audibility route; verify outgoing isolation and absence of active ramps. Check remaining loop/cue/sync/master states explicitly. A zero software fader target plus `applied` does not instantly settle the short native smoothing; rendered ramp completion provides a clearer endpoint. Give sources, actual action results, timings and remaining uncertainty, not just “transition succeeded”. Listening still must judge bass balance, loudness, vocal/phrase clashes, artifacts and musical feel.

On invalid commands, follow discovery and structured error corrections. On timeout, inspect state/job before retrying; an operation may have applied. On source conflict or unexpected manual movement, stop issuing the next phase, inspect, and yield rather than restoring a cached user state. Preserve the continuing audible deck during a failed load or preparation; don't wait indefinitely near its end. If a blend has partly applied, inspect what is currently playing/audible and select a simple supported stabilization within the authorized task instead of replaying the whole batch.

`pd jobs cancel JOB_ID` stops future job steps and interrupts engine waits; it does not undo active ramps or universally cancel loads/imports/provider operations. On requested takeover use `pd automation stop`, then `pd jobs list` and `pd status`. It cancels ramps/jobs and releases cue/jog holds, but leaves playback/recording and current knob values in place; operation-specific cancellation may still be needed. Manual movement of a ramped control cancels that target. Report what actually stopped, any active operation, and any incomplete handoff; never fight the user's new control state.

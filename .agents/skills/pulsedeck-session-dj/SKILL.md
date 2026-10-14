---
name: pulsedeck-session-dj
description: Run a requested PulseDeck playlist or practice set through repeated track selection, isolated preparation, musically reasoned transitions, and verification using pulsedeckctl. Use for continuous DJ sessions and stopping their automation; use set preparation for planning a playlist without playback.
---

# Run a DJ session

Use for “play this playlist and handle the transitions”, “run a 30-minute practice set with long blends and minimal effects”, or “stop automation; I’m taking over”. Operate through the existing shell CLI; the AI chooses musical strategy and PulseDeck executes it. A request to run the set authorizes the necessary preparations and transitions within its stated scope. Do not ask again before each already-authorized step. A request to explain or plan first authorizes observations and a plan, not playback or mixer changes.

Read [CLI reference](../references/cli.md) to bind the correct runtime and understand guards, stages, ramps, jobs, errors, and cancellation. Read [musical evidence](../references/musical-evidence.md) when choosing tracks and transition regions. Use [set preparation](../pulsedeck-set-preparation/SKILL.md), [track inspection](../pulsedeck-track-inspection/SKILL.md), [incoming preparation](../pulsedeck-prepare-next-track/SKILL.md), and [transition](../pulsedeck-transition/SKILL.md) for their detailed procedures as needed. This skill owns the repeated session loop, timing budget, recovery, and handoff.

## Establish intent and observe the session

Resolve the requested playlist/order, duration, mixing style, effects preference, recording intent, and end behavior from the request and current session. Clarify a genuinely missing essential choice while continuing read-only inspection. Preserve explicit track order unless selection/reordering is authorized. Recording and library/cue edits are separate intentions; running a playlist does not require either.

Apply [DJ musicianship](../references/dj-musicianship.md) throughout the session. Translate pace into track occupancy, audible overlap and final gesture separately. If the user wants the first buildup/drop before each change, preserve that payoff and choose a suitable following phrase boundary; a first bass entry is not sufficient evidence. A requested BPM interval is an allowed range or intended arc, not a target at its upper edge. Preserve those preferences across follow-ups.

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

For each handover, use [phrase planning and launch verification](../references/musical-evidence.md#phrase-planning-and-launch-verification). Record both musical boundary candidates, actual B entry-to-arrival beat events, exact outgoing launch timestamp, grid/downbeat/manual provenance and phase error from both native `beat_position` values before B becomes audible. Full Beat This! preserves variable timestamp spacing; mean BPM is not an exact event clock. An analysis failure/warning requires manual validation before automated beatmixing. Model scores are uncalibrated; a successful fader ramp does not establish phrasing, alignment or rekordbox parity.

Maintain a small rolling session plan: current protected payoff/exit, next source and alternate, incoming entry/arrival, launch/open/band-transfer/clear events, target tempo/key strategy, ready-by deadline and evidence. Include low, mid and high contributions so the overlap retains body and air without piling up competing parts. When harmonic mixing is a priority, shortlist compatible keys as played before ranking convenience. Begin the next selection as soon as a deck is safely released. If loading misses the musical deadline, advance to another credible boundary; job readiness is not the mix-out cue. Avoid reusing one transition duration across materially different pairs. Explain progress in musical terms and report uncertainty once without filling the live session with research commentary.

Default inference uses native DBN decoding to select one whole-track meter of three, four, five or seven beats per bar over 55–300 BPM. It does not support arbitrary meter changes. Read `beatgrid_uniform`/`beat_events.uniform`: offset-only correction preserves active neural intervals, while manual BPM/half/double correction selects a constant-tempo grid across the whole source. Original neural counts are provenance, not effective event counts. Manual bar-origin numbering retains detected meter, falling back to four if unknown. Beatmatch with rate/tempo setters to preserve grid mode.

1. **Inspect ahead.** While the live deck continues, inspect overview and detailed ranges near likely entries/exits. Read `beat_events` and narrow truncated views before counting events; distinguish model downbeats, manual origins and unresolved `rhythm_error`. Use existing cues and bass/level/texture changes as arrangement evidence. No listening claim from JSON. A predicted bar start is separate from a phrase, and 8/16/32-bar patterns are candidates.
2. **Select within intent.** Compare tempo-change size, key evidence and plausible phrase/texture compatibility. Favor a manageable successor and explain transition difficulty. Harmonic matches are guides; inferred waveform level does not establish perceptual energy or mood. Preserve requested order, or report an obstacle rather than silently replacing a requested song.
3. **Prepare early and inaudibly.** Budget incoming load, pitch/key-lock preparation, inspection, transition duration and recovery margin before the outgoing track's chosen exit. Base the margin on observed load times and uncertainty; do not wait until the last bar. Identify the available deck, close only its channel fader, load without `--replace-playing`, wait for `ready`, capture the returned source/revision, and apply guarded incoming controls. Choose the entry from existing cues/evidence, isolate master contribution, set tempo/key lock/sync/headphone cue as appropriate, and wait for preparation. Do not globally mute the master or change routing merely to prepare a song. In silent trials, both master and cue gain are deliberately zero.
4. **Plan the overlap.** State outgoing/incoming identities and revisions, timestamps, provisional meter/phrase assumptions, overlap in bars/beats/seconds, bass exchange, fader moves and purposeful optional effects. Count phrase progression on both tracks. Choose a shorter fade/cut when confidence, drift or competing density make a long blend unreliable; a loop needs a verified rhythmic region and an explicit exit. Do not invent richer analysis until a concrete observed mixing failure shows a need.
5. **Execute and monitor.** Re-observe immediately before transport/fader intervention. Native timed ramps advance on rendered audio frames; audio must run. Use guarded deck operations and named/global ramp controls appropriate to the plan. An `applied` ramp is not completed; wait/inspect its target and `active_ramps`. Avoid one huge unattended batch: it cannot react to changing evidence, manual controls, a failed load, or the next track ending. Short validated batches may group known steps, with a source guard on each deck step; an outer batch guard does not protect children.
6. **Verify the handover.** Confirm incoming source/revision, position progression, preparation complete, final fader/EQ/filter/effects, old-deck contribution closed, no unintended jobs/ramps and acceptable software meters. Only then pause the outgoing deck if that is part of the plan; do not unload a track still contributing to the mix. Record actual overlap/elapsed time and any uncertainty. Release the old deck for the following preparation, reassess remaining time and repeat until the requested endpoint.

Native `deck launch` supports queued starts; see the CLI reference. Ordinary unsynced play starts immediately. Polling or shell sleeps can approximate an intervention window but cannot promise an exact beat/downbeat/phrase start. Ramp quantization applies to the ramp, not the separate play command. When exact timing cannot be delivered, choose a fresh eligible musical window or a justified alternative entry, preserve any protected payoff, and describe the approximation. Bounded immediate execution is not permission to fire an expired phrase launch or cut at an arbitrary next beat. Apply [timed execution](../references/timed-execution.md) before automated handovers: start the controller before queuing, leave margin between sequential phases, and prepare prompt recovery from a partially audible blend. A faster model can reduce planning latency; it does not replace these controls.

Include command and reasoning latency in each overlap budget. Native ramps control their own rendered duration, but gaps between starting ramps, observing results and choosing the next stage lengthen the whole transition. A blind run's intended 32-second overlap took about 53 seconds even after its clock was corrected. For already-decided controls, a short source-guarded batch can launch concurrent targets and wait a known phase without a model/tool round trip between every knob. Keep observations between bounded phases and verify endpoints before outgoing pause; don't replace the session with one blind script. Reserve time for final cancellation/pause acknowledgments, and don't begin optional preparation when the requested endpoint is already near. Report actual overlap length separately from the nominal ramp durations.

During a session, monitor compact state at a cadence suitable to remaining time rather than dumping full analysis repeatedly:

```sh
pd state watch --count 4 --interval 250ms
pd jobs list
pd status
```

Look for position discontinuities (`position_revision`), source changes (`load_revisions`), preparation/source jobs, stopped audio, unexpected loops, manual target movement, or changed sync/master. Guards protect source identity, not every knob or seek. Native ramps may cancel on clock pause/seek/source change; audio stop freezes progress. Mid-ramp tempo changes require reassessment. A manually moved target takes over that ramp; do not fight the user by replacing it.

## Execute the planned handover

Use the [transition skill](../pulsedeck-transition/SKILL.md#execution-surfaces-after-the-musical-plan) for command shapes after the arrangement plan is complete. Do not turn a next-beat launch and short fade into the session's automatic recipe. Queue the selected future event, leave enough isolated lead to verify beat/bar phase and arrival timing, then execute the chosen bounded mixer stages. A completed source load is readiness evidence, not an instruction to mix immediately.

Before releasing the outgoing deck, inspect both sources/revisions, incoming advancement and readiness, final fader/EQ/FX, active ramps/launches and the actual musical handover. Pause or replace the outgoing deck only after its contribution has cleared. A clean engine state is operational evidence; report perceptual quality only when heard or confirmed by the user.

**Planning example:** for “explain the 30-minute set first”, run only discovery, playlist/library reads, `status`, `jobs list`, and relevant `track.inspect` queries. Return the ordered sources, likely overlaps, total source duration versus conditional set estimate, cue/phrase evidence, fallback choices and unknowns. Explicitly say no transport/mixer mutations were applied and wait for the requested execution instruction.

## Failure recovery and manual takeover

- **Failed incoming load/preparation:** inspect the specific error, `jobs get JOB_ID`, source jobs and incoming state. Keep the healthy live deck playing and the failed incoming channel isolated. A failed job stops later steps but does not roll back earlier changes. Select an authorized alternate or a simpler transition within the remaining budget; retry only a diagnosed condition with a bounded attempt. Do not indefinitely wait for a failed job while the live song ends. If no viable incoming source exists, report the risk promptly; a supported loop is an option only if its region/grid is credible and extending the track is within scope.
- **Timeout/uncertain mutation:** read state/job before retrying. An operation may have applied despite a timed-out reply. Reuse the same request ID for an identical uncertain mutation when appropriate; repeated toggle/play/cue mutations with fresh IDs are not harmless. Correct invalid syntax using error details and `commands describe`.
- **Stale source or unexplained manual change:** stop issuing this plan's controls, re-inspect and yield. Do not reload/reset the user's new source or knobs to cached values. Report what completed and what is abandoned; no automatic resume after takeover.
- **Requested stop:** issue `automation stop` promptly, then verify its actual effect. It cancels jobs, native ramps, pending launches and cue/jog holds; it does not generally pause playback, stop recording or restore prior knobs. Job cancellation prevents future steps and interrupts engine waits, but an active load/import/provider operation may continue. Identify any active load and use its explicit `deck.load.cancel` capability when cancellation is requested; verify the source job afterward. Report persistent work that cannot be cancelled.

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

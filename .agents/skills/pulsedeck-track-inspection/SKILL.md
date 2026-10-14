---
name: pulsedeck-track-inspection
description: Inspect PulseDeck track metadata, confidence, cues, and timestamped band waveforms to suggest entry/exit regions, explain changes, or mark requested cues without moving playback. Use for track preparation and musical evidence; not for unattended playback or executing a transition.
---

# PulseDeck track inspection

Use the existing `pulsedeckctl` CLI. Read [CLI reference](../references/cli.md) for setup, response stages, errors, source guards, and cancellation; read [musical evidence](../references/musical-evidence.md) when reasoning about grids and sections. Inspection and cue editing do not require audible playback.

## Establish the source and evidence

Bind shell variables to the running instance and the selected track; never copy an example's ID onto a different source. Use an executable on PATH, or the repository client at `target/debug/pulsedeckctl`.

```sh
CTL="$(pwd)/target/debug/pulsedeckctl"
"$CTL" commands list --json
"$CTL" commands describe track.inspect --json
"$CTL" commands describe cues.set --json
"$CTL" library tracks --search 'requested title' --json
"$CTL" status --json
```

Identify source, duration, title/artist, loaded deck/revision, BPM/key/provenance, effective grid mode, events, `rhythm_error` and cues. Full Beat This! `final0` plus native DBN selects a whole-track meter of three, four, five or seven beats per bar over 55–300 BPM; it does not identify arbitrary meter changes or phrases. Manual origin/BPM overrides are separate edits; origin edits retain the detected bar length. Scores are uncalibrated. Neural `bpm_ambiguous:true` flags broad local tempo spread or an explicitly inferred pulse/phase repair. A repaired passage may have consistent local BPM and still need review: inspect `bar_evidence.pulse_repairs`, `half_phase_corrected`, and `beat_events.local_tempo.inferred_events_in_range` before automation. Inserted midpoint events are reconstructions, not fresh neural observations. Missing fields or failed analysis remain uncertain even if an older estimate is retained. Use `cues.get` as the authoritative cue snapshot.

Inspect an overview, then bounded ranges. `track.inspect` takes exactly one of `deck` or `source`; `points` is 1–512 (default 64), not `bins`. It returns metadata, source/load revision, cues, requested range, waveform bins and `beat_events`. Events contain `seconds`, `beat_index`, anchor-relative `beat_position` and `downbeat`; the view reports counts/truncation, model frame resolution 0.02 seconds, `source`, `manual` and `uniform`. Uniform events are the effective manual lattice, not original model detections. The neural summary keeps original counts/model/scores with `scores_calibrated:false`, so its counts may differ. Narrow truncated ranges before counting progression. More points do not interpolate neural beats. Null bands are missing evidence.

For an already loaded deck A, these are read-only planning commands. The range example applies to a track longer than 24 seconds; choose bounds from its returned duration.

```sh
"$CTL" call track.inspect --args '{"deck":"A","points":64}' --json
"$CTL" call track.inspect --args '{"deck":"A","start_seconds":8,"end_seconds":24,"points":256}' --json
"$CTL" call cues.get --args '{"deck":"A"}' --json
"$CTL" call engine.transport --args '{}' --json
```

To explain a change at 2:10, examine before/after 130 seconds, clamp the range to the actual duration, and zoom again if a coarse overview merges the change into one bin. Peak maxima describe local amplitude; low/mid/high values describe band energy on a shared scale, not isolated instruments. Compare adjacent windows within this track, avoiding loudness or energy rankings between differently mastered tracks based on peak data alone.

## Make musical judgments

For preparation toward mixing, use the [structure and handover guide](../references/dj-musicianship.md#hear-the-structure-then-choose-the-controls). Produce a short event map with source ranges, beat/bar positions, candidate roles, evidence and confidence. Compare the lead-in and aftermath before suggesting a buildup/drop; the initial kick or bass arrival alone does not identify the first payoff. Include pickups and uncertainty in the map, keeping predicted bar starts distinct from arrangement boundaries. No audio-listening or automatic section-recognition capability is implied by `track.inspect`.

Look for sustained bass arrival/removal, amplitude changes, silence or fade tails, changing frequency balance, and recurring patterns. A single quiet bin can be a transient gap; check neighboring bins and a longer range. Distinguish an isolated event from a sustained region suitable for an overlap.

Relate passages to the returned beat/downbeat timestamps and count actual events. For a loaded deck, native `beat_position` gives its current phase on the effective grid. Do not replace a neural grid with `beat_index=(time-offset)*BPM/60`; mean BPM arithmetic is only a constant-tempo estimate. Declare a meter assumption before using bars and compare model downbeat spacing. For assumed 4/4, 8/16/32 bars correspond to 32/64/128 beats; compare them with arrangement changes. A predicted downbeat or manually edited origin does not establish the first phrase or an exact transition start.

For a likely intro, favor a stable, lower-density region with room before a marked bass or amplitude change. For exits, compare a sustained bass reduction, a repeating groove with enough remaining time, and a shorter late fade if supported. Explain why each might fit a blend or shorter cut, and its risks. Check how much room remains before the next density change: at 120 BPM, an entry at 8 seconds with bass arrival at 16 leaves roughly 16 beats (four bars under 4/4), not a 16-bar blend. A later cue is not automatically a better intro. Frequency bands cannot prove vocals, a drop, chorus, breakdown, or absence of vocals. Say “bass energy falls around …; possible reduced-density passage,” not “I heard the vocals stop.” Never claim listening from numerical analysis.

A planning request ends with suggestions and evidence. Add/change cues only when requested. Do not seek, trigger hot cues, play, or change mixer controls to inspect a track. If analysis is weak, recommend a conservative cue candidate and audible confirmation instead of inventing a section map or reanalyzing automatically.

## Correct a requested beatgrid

Use the CLI reference's [grid correction and launch procedure](../references/cli.md#correct-a-grid-and-queue-a-musical-launch). The native `deck B grid set --offset SECONDS --expected-load-revision REV` command is the same correction as the UI's Set bar 1 here. Choose a known musical downbeat, verify transient alignment around that passage and farther into the track, then check effective offset, manual provenance and preserved position/cues. Corrections persist independently of model predictions. Apply them only when requested/authorized; inspection alone remains read-only. Missing phase is unknown. If `rhythm_error` reports failed analysis, require manual validation before an automated beatmix; do not silently promote an older estimate to a high-precision result.

Offset-only correction retains variable neural intervals. Explicit `deck B bpm set BPM` changes to a constant-tempo grid for the whole track; half/double BPM uses that same mode. Inspect loaded `beatgrid_uniform` and `beat_events.uniform` as authoritative, because original neural metadata remains provenance. Manual bar-origin edits retain detected meter, with four as the unknown-meter fallback. For an authorized refresh use `deck B analyze`, wait completion and reinspect; existing manual overrides survive reanalysis.

## Mark a requested cue while preserving transport

Before editing, re-read cues and use a free slot; preserve unrelated points and labels. If all relevant slots are occupied, report that and propose the intended replacement for the user's request. A source-specific `cues.set` single-point edit preserves other fields and does not move playback. Hot indices are 0–7; memory indices 0–9. Memory points sort chronologically, so re-read the resulting index and label rather than assuming the insertion slot remained fixed.

When editing a loaded deck, use both the observed load revision and `expected_revision` from `cues.get`. On `source_changed` or revision conflict, re-read identity/cues and re-evaluate; do not retry against a new source or overwrite a manual cue change. Use explicit seconds, not “current,” for a researched point. Keep speculative labels qualified, such as “Candidate intro.”

Record the play state/position before and after. Require `ok:true` and stage `persisted`; verify returned canonical source, changed point/label, unchanged unrelated cues, and matching loaded source/revision. With audio stopped, position should be unchanged. During playback, position naturally advances: verify it remains playing and progresses continuously rather than demanding identical positions. Native `applied` only acknowledges control application; `accepted` is not persistence or completion.

This execution example is for an already loaded A and an evidence-backed decision to mark 8 seconds. Change that chosen timestamp only after inspection. It selects a free hot slot and binds both guards from actual responses rather than an example revision. After binding `CTL` from the repository root, run it in a temporary working directory so evidence files do not replace unrelated files. Keep `PULSEDECK_SOCKET` set for an isolated instance; the client inherits it. The command does not seek or trigger the cue.

```sh
"$CTL" call track.inspect --args '{"deck":"A","points":64}' --json > inspection.json
"$CTL" call engine.transport --args '{}' --json > transport-before.json
python3 - <<'PYCODE'
import json
r = json.load(open("inspection.json"))
assert r["ok"], r
v = r["result"]
i = next((i for i, t in enumerate(v["cues"]["cues"]["hot_cues"]) if t is None), None)
if i is None:
    raise SystemExit("All hot slots are occupied; preserve existing cues and report this.")
assert 8 < v["metadata"]["duration_seconds"]
json.dump({"command":"cues.set", "expected_load_revision":v["load_revision"],
           "args":{"deck":"A", "kind":"hot", "index":i, "seconds":8,
                   "label":"Candidate intro", "expected_revision":v["cues"]["revision"]}},
          open("cue-request.json", "w"))
PYCODE
"$CTL" command --file cue-request.json --json > cue-result.json
"$CTL" call track.inspect --args '{"deck":"A","points":64}' --json > inspection-after.json
"$CTL" call engine.transport --args '{}' --json > transport-after.json
```

Read these responses to verify persistence and preserved transport as described above; a saved JSON file alone is not success. If A's source changed after inspection, the guard rejects the edit. For an unloaded source, bind its canonical identity explicitly and use its cue revision guard; no deck load guard applies.

## Output and recovery

Return candidate timestamps or intervals, supporting measured observations, inferred musical interpretation, uncertainty, likely transition use, and proposed cue slot/label. Include source identity and analysis caveats. For a cue edit, report the actual persisted timestamp/revision and transport verification separately from the musical hypothesis. State what needs listening, particularly perceived phrasing, vocals, meter/downbeat and whether an overlap sounds good.

For invalid input, use the CLI's error code, hint, schema and correction details; fix the specific argument. On transport timeout, inspect state/cues before retrying an uncertain mutation and retain the same request ID for an identical retry. A failed request does not justify pausing a performing deck. Stop further edits promptly on manual takeover. This skill normally launches no automation, jobs or ramps; if continuing a broader workflow, inspect pending work and cancellation semantics before assuming it stopped. Native queued launch is available; shell polling/sleeps cannot deliver exact phrase-timed starts.

## Requests this skill handles

- “Inspect this track and suggest three places to mix out”: overview → detailed ranges → three evidence-backed candidates if available, with uncertainties; no cue mutation.
- “Find a likely clean intro and mark a cue without moving playback”: inspect → choose a supported candidate → guarded free-slot edit → persistence/transport check. “Clean” remains an inference requiring listening.
- “Explain what changes around 2:10”: inspect windows before/after 130 seconds, relate changes to the candidate grid and clearly separate observation from section hypothesis. Return a bounded uncertainty interval rather than false sample precision.

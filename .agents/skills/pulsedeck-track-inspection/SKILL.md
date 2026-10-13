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

Identify the canonical source, duration, title/artist, any loaded deck and its load revision. Read BPM alternatives, ambiguity/confidence and provenance; beatgrid offset/confidence/provenance; key/Camelot alternatives and confidence; existing main/hot/memory cues and annotations. Missing or low-confidence fields stay uncertain; confidence values are algorithm scores, not calibrated probabilities. A manual grid is a user edit, not proof of a musical downbeat. Use `cues.get` as the authoritative cue snapshot rather than assuming library rows contain current annotations.

Inspect an overview first, then bounded detailed ranges around promising changes. `track.inspect` takes exactly one of `deck` or `source`; `points` is 1–512 (default 64), not `bins`. It returns metadata without large wave arrays, canonical source, deck load revision when applicable, `cues.revision` and `cues.cues`, requested range, stored-bin resolution, and timestamped bins. Keep timestamp intervals and original resolution in the evidence. Returned boundaries can straddle the requested range because the query includes touched stored bins; increasing points cannot recover missing resolution. `bands_available:false` and null bands mean no band evidence, not zero energy.

For an already loaded deck A, these are read-only planning commands. The range example applies to a track longer than 24 seconds; choose bounds from its returned duration.

```sh
"$CTL" call track.inspect --args '{"deck":"A","points":64}' --json
"$CTL" call track.inspect --args '{"deck":"A","start_seconds":8,"end_seconds":24,"points":256}' --json
"$CTL" call cues.get --args '{"deck":"A"}' --json
"$CTL" call engine.transport --args '{}' --json
```

To explain a change at 2:10, examine before/after 130 seconds, clamp the range to the actual duration, and zoom again if a coarse overview merges the change into one bin. Peak maxima describe local amplitude; low/mid/high values describe band energy on a shared scale, not isolated instruments. Compare adjacent windows within this track, avoiding loudness or energy rankings between differently mastered tracks based on peak data alone.

## Make musical judgments

Look for sustained bass arrival/removal, amplitude changes, silence or fade tails, changing frequency balance, and recurring patterns. A single quiet bin can be a transient gap; check neighboring bins and a longer range. Distinguish an isolated event from a sustained region suitable for an overlap.

Relate timestamps to the candidate grid using `beat_seconds = 60 / BPM` and `beat_index = (time - offset) / beat_seconds`. Declare a meter assumption before using bars. For assumed 4/4, 8/16/32 bars correspond to 32/64/128 beats; these are hypotheses to compare with repeated waveform changes. Grid origin does not establish true downbeat, first phrase, meter, or an exact transition start.

For a likely intro, favor a stable, lower-density region with room before a marked bass or amplitude change. For exits, compare a sustained bass reduction, a repeating groove with enough remaining time, and a shorter late fade if supported. Explain why each might fit a blend or shorter cut, and its risks. Check how much room remains before the next density change: at 120 BPM, an entry at 8 seconds with bass arrival at 16 leaves roughly 16 beats (four bars under 4/4), not a 16-bar blend. A later cue is not automatically a better intro. Frequency bands cannot prove vocals, a drop, chorus, breakdown, or absence of vocals. Say “bass energy falls around …; possible reduced-density passage,” not “I heard the vocals stop.” Never claim listening from numerical analysis.

A planning request ends with suggestions and evidence. Add/change cues only when requested. Do not seek, trigger hot cues, play, or change mixer controls to inspect a track. If analysis is weak, recommend a conservative cue candidate and audible confirmation instead of inventing a section map or reanalyzing automatically.

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

For invalid input, use the CLI's error code, hint, schema and correction details; fix the specific argument. On transport timeout, inspect state/cues before retrying an uncertain mutation and retain the same request ID for an identical retry. A failed request does not justify pausing a performing deck. Stop further edits promptly on manual takeover. This skill normally launches no automation, jobs or ramps; if continuing a broader workflow, inspect pending work and cancellation semantics before assuming it stopped. Scheduled transport is unsupported; shell polling/sleeps cannot deliver exact phrase-timed starts.

## Requests this skill handles

- “Inspect this track and suggest three places to mix out”: overview → detailed ranges → three evidence-backed candidates if available, with uncertainties; no cue mutation.
- “Find a likely clean intro and mark a cue without moving playback”: inspect → choose a supported candidate → guarded free-slot edit → persistence/transport check. “Clean” remains an inference requiring listening.
- “Explain what changes around 2:10”: inspect windows before/after 130 seconds, relate changes to the candidate grid and clearly separate observation from section hypothesis. Return a bounded uncertainty interval rather than false sample precision.

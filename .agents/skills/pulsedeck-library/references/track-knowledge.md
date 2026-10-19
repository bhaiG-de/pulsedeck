# Track inspection, saved cues and grid edits

## Inspect without moving playback

Bind the source using exactly one of `source` or `deck`. Prefer unloaded source inspection for preparation. Read duration, title/artist, BPM/key estimates and provenance, cue revision, and current source/load revision when loaded. Read [analysis semantics](../../references/musical-evidence.md) for effective grids, repaired pulses and observation limits.

```sh
pd call track.inspect --args '{"deck":"A","points":64}'
pd call cues.get --args '{"deck":"A"}'
```

Inspect an overview, then valid bounded windows around the change. `track.inspect` accepts `points` 1–512, not `bins`. Require `0 <= start < end <= duration`. For a request around 2:10, inspect before/after source second 130 and narrow when the original bins support it. More output points do not improve stored temporal resolution.

Join manual cue spans to waveform bins and effective beat events. Inspect exact source endpoints, meter, individual cue provenance and current grid revisions. There is no automatically generated section view or cue list. Missing manual cues are normal.

For a mix, return a bounded evidence packet: source identity/duration/key/BPM and provenance; available human cue spans; exact effective event positions/timestamps around candidate entries, arrivals and outgoing changes; neighboring bands; grid warnings and revisions. `track.inspect` exposes `metadata.cue_points`, `beat_events` and `bins`; `cues.get` has a nested cue object and its own revision. Do not guess missing fields.
Count beat-position differences over complete ranges. If `beat_events.truncated` is true or `total_in_range` exceeds returned events, narrow/split the interval with enough points (up to 512) before reporting a lead. Do not infer phrase lengths from rounded seconds and whole-track BPM. Keep full event precision in the packet and leave musical judgment to the art skill. Save large responses and extract relevant fields rather than truncating the timing evidence in tool output.

During live preparation, retain complete raw intervals for validation but show the agent only candidate anchors, interval counts, relevant neighboring band summaries, warnings and revisions. Dumping hundreds of bins/events into the conversation consumes the outgoing track's runway. Reuse the current pair's evidence until source/grid identity changes; select exact returned timestamps rather than reconstructing or rounding them.

For musical entry/exit recommendations use `$djing-art`. Retain raw observed time ranges, candidate beat/bar locations, evidence and uncertainty. Do not seek, trigger a cue, play or alter mixer controls just to inspect. A requested headphone audition belongs to `$pulsedeck-control` and requires isolation/routing.

## Edit one canonical marker

Read [named anchors](../../references/named-cues-and-suggestions.md) and current `commands describe cues.set`. Canonical points carry stable IDs, source start/end, name/role, manual/imported origin, confirmation and edits. Main, hot and memory slots bind to points. Use one of `point`, `delete_id` or `bind` per canonical request; do not create another marker representation to repair a projection mismatch.

Before writing, re-read `cues.get`; bind its `expected_revision`. For a loaded target, use the observed top-level `expected_load_revision` as well. A source guard does not guard saved cue edits, and a cue guard does not guard a deck replacement. Use explicit source seconds. Preserve unrelated points/labels, edits and bindings. On revision conflict, inspect and reconcile instead of retrying against a different source.

For a requested slot edit, JSON hot indices are 0–7 and memory indices 0–9. Prefer a free slot; if occupied, use a replacement only when the request covers it. Memory slots sort chronologically, so re-read their final index. Explicit-time setters preserve transport; a hotcue trigger starts playback and is not a metadata edit. Label inferred roles as candidates.

Require successful persistence and re-read the canonical cue. Some native edits respond `applied` with `cue_revision`; verify through `cues.get` rather than mistaking a stage name for a failed write. Check the intended point/binding, unchanged unrelated fields and matching source identity. Record before/after transport: paused position stays fixed; playing position should advance continuously with unchanged position revision rather than be identical.

## Correct a requested grid

Grid correction is a saved metadata change. Apply it when requested/authorized, not to make an inspection look more confident. Choose a known downbeat with evidence and verify local and later timing. For a loaded target, use the [guarded grid procedure](../../references/cli.md#correct-a-grid-and-queue-a-musical-launch), preferably while paused and isolated.

An offset-only `deck B grid set --offset SECONDS` preserves active neural intervals; it leaves an existing uniform override uniform. `deck B bpm set BPM`, including half/double correction, selects a whole-track constant-tempo grid. Manual origin numbering retains detected meter, with four as the fallback if unknown. These operations preserve transport/cues and persist across reload/reanalysis. Reinspect effective offset, uniform/manual flags, events and saved source identity.

Playback matching instead uses `tempo set --bpm` or Sync through the control workflow. Do not correct stored BPM solely because another deck plays at a different tempo. A source grid correction near an armed launch can invalidate native timing; coordinate with its owner instead of changing the clock under queued work.

## Refresh analysis deliberately

Passive reads reuse existing analysis. An inspection request does not authorize expensive refresh or saved metadata changes. For an authorized update use `library analysis update --no-wait`, retain the job/worker ID and inspect completion/errors. `library analysis cancel` stops further work without undoing completed records. A loaded refresh uses guarded `deck B analyze`; wait completion and recheck source/grid/cues.

Reanalysis updates BPM, beat/downbeat timing, key and waveforms. It does not generate cues, label sections or fill empty pads. Saved manual/imported markers stay unchanged.

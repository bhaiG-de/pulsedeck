# Track inspection, saved cues and grid edits

## Inspect without moving playback

Bind the source using exactly one of `source` or `deck`. Prefer unloaded source inspection for preparation. Read duration, title/artist, BPM/key estimates and provenance, cue revision, and current source/load revision when loaded. Read [analysis semantics](../../references/musical-evidence.md) for effective grids, repaired pulses and observation limits.

Use `track_inspect` arguments such as `{"deck":"A","points":64}` and `cues_get` with `{"deck":"A"}`. These are MCP tool calls, not source-control gestures.

Inspect an overview, then valid bounded windows around the change. `track_inspect` accepts `points` 1–512, not `bins`. Require `0 <= start < end <= duration`. For a request around 2:10, inspect before/after source second 130 and narrow when the original bins support it. More output points do not improve stored temporal resolution.

## Build a counted source map

Use the overview to locate repeating plateaus, withdrawals, increases in activity and sustained returns across low/mid/high bands and peaks. Read bounded windows around changes to distinguish a brief fill from a change that persists. Link adjacent windows and repeated material, rather than returning the largest single peak. Manual/imported cue spans can supply human context; missing cues are normal and do not prevent measured structural inference.

Anchor the useful windows to effective beat events. Record source seconds, `beat_position`, meter and the origin/provenance of a phrase hypothesis. A returned downbeat establishes a bar boundary according to the effective grid; a group of eight bars is not automatically a musical phrase. Recurrence and coordinated changes support arrangement boundaries. Let `$djing-art` infer the functional roles and select entries from that map.

`track_inspect` exposes `metadata.cue_points`, `beat_events` and `bins`; `cues_get` has a nested cue object and its own revision. Read actual available fields rather than guessing them. Keep the following compact evidence together:

| Evidence | Useful content |
|---|---|
| Identity and tuning | Canonical source/version, duration, BPM/key estimates and confidence/provenance; loaded source/revision when relevant |
| Clock | Meter and effective downbeat identity; uniform/manual BPM provenance and available neural/local pulse estimates; exact event objects with both beat_index and beat_position, counts and coverage |
| Source course | Ordered neighboring band/peak windows, change locations, repetition, withdrawal and subsequent return; retain the measured values supporting an inference |
| Played balance | Available loudness/normalization measurements, source gain and relevant local amplitudes; distinguish raw source RMS from normalized played level |
| Human knowledge | Existing cue spans/labels, origin/confirmation and cue revision; keep these distinct from new provisional roles |

For a pair, extend the map before first audibility and beyond outgoing clearance to the next meaningful incoming change/developed phrase. Include any intervening sparse span and its beat count. This lets the DJ distinguish a supported approach with a near return from a long simultaneous withdrawal. A source packet supports multiple offsets; it is not an automatically correct mix point.

Count effective beat-position differences over complete ranges. If `beat_events.truncated` is true or `total_in_range` exceeds returned events, narrow/split the interval with enough points (up to 512) before reporting a lead or duration. Preserve exact returned timestamps; whole-track BPM and rounded seconds do not recover missing beat events. Check local timing warnings and source/analysis identity when joining responses.

Retain full raw responses in working files and give the agent a compact ordered map with relevant anchors, per-window bands, counts, coverage and revisions. Compression must preserve the shape and its following event, not reduce the track to one energy score. Reuse this map while identity/grid/analysis remain compatible. Inspect finer ranges only where the next decision needs them; more requested points cannot restore detail absent from the stored waveform.

Functional inference is useful even without semantic annotation: a measured low-band withdrawal and later sustained return can support a planned reduction and restoration of bass weight. It does not by itself prove a vocal, a producer-named drop or an emotional mood. Mark inferred roles and their basis so the art skill can reason concretely without presenting guesses as labels.

Inspection never requires seek, cue trigger, Play or mixer movement. A requested headphone audition belongs to `$pulsedeck-control` and requires isolation/routing. Working source maps and transition observations are session notes; saving canonical cues/tags/grid changes is a separate authorized action.

## Authorized slot and grid corrections

Read [named anchors](../../references/named-cues-and-suggestions.md) before a saved edit. Arbitrary named-point/binding edits require an unexposed metadata-write capability; report it rather than create a parallel cue store. An explicitly requested loaded-deck slot edit can use the actual `deck_control` setter schema under `expected_load_revision`, followed by `cues_get` verification. Preserve unrelated points, bindings and slots; do not trigger a cue to save it.

Grid correction is a persistent metadata change, not playback matching. When requested, choose a supported downbeat with evidence, preferably on an isolated paused deck. `deck_control` with `set_beatgrid` supplies `offset_seconds` and optional `anchor_index`; offset-only realignment preserves active neural intervals. `set_bpm` with `bpm` selects a whole-source uniform clock, including an intended half/double correction. Bind the observed source revision and reinspect effective grid/provenance, source identity and unchanged transport/cues. Do not change an owned clock underneath an armed score.

Playback matching instead uses native `set_playback_bpm`, `set_tempo` or `set_sync` through control. Bulk analysis refresh is not exposed here. Compatible `track_inspect` reads reuse analysis, while an authorized isolated `deck_load` can finish that source's missing analysis before playback. Reanalysis does not create cues or section labels.

## Carry executable anchors through compression

Keep the selected raw event object beside a rounded display label. For beat-bound scheduling, return its `beat_index` as evidence for obtaining its exact `performance_observe` handle on a loaded source; the effective `beat_position` is for interval counts and can differ from the index. If source seconds are required, pass the original numeric value directly. A display such as 164.097 s can exceed its actual event at 164.09683227539062 s and resolve to the following beat. Returning only the rounded label loses the selected anchor.

Include manual/uniform provenance and effective downbeat identity when discussing rhythmic overlap. A neural summary and a manually overridden clock can describe different pulses. Supply enough fine local band/peak recurrence to investigate a material discrepancy; do not silently rewrite canonical analysis or claim a bar origin from a downbeat-free clock.

# Collections, tags and playlists

## Read the requested material

`library.tracks` returns `tracks`, `total` and `next_offset`. Follow non-null offsets until the intended collection is covered or enough eligible candidates are inspected. Sort options are title/BPM/duration; a sorted result is not a DJ sequence. Search is textual, not a semantic mood classifier. Inspect stored tags for genre/mood evidence rather than inferring them from artist/title or a waveform.

Use this indexed, paginated query directly for local candidate discovery; do not export the collection and filter it in the agent. Text search includes indexed file metadata and tags. Apply BPM, playlist and classification filters in the same request, and stop paging once a useful shortlist exists. Default responses omit expensive waveform/analysis arrays; inspect bounded passages only for the selected pair. Missing-BPM tracks are excluded by BPM bounds, so consider a separate small unbounded query when an unanalysed candidate is appropriate and authorized.

```sh
pd library tracks --search house --bpm-min 120 --bpm-max 128 --sort bpm --limit 20 --json
pd library tracks --filters '{"my_tags":[1,2],"tag_mode":"and","rating_min":3,"colors":["#FF0000"]}' --limit 20 --json
```

Use observed taxonomy IDs and color values. `tag_mode` accepts `and` or `or`; rating bounds are 0–5. `call library.tracks --args JSON` provides the same query inside the generic CLI, and session `op:call` can read it without creating another queue. Read-only queries need no deck load or reanalysis.

`--playlist` restricts local membership, but the results remain sorted. For exact saved order, read `playlist.list`, get its integer `track_ids` array and join to the track records. Local IDs may be addressed as `local:ID`; playlist replacement arrays contain unique integer IDs, not strings. Read selected details with `library track get` and canonical markers through `cues.get`.

For Spotify, discover the current `spotify.*` collection methods and follow their pagination. Library/Connect and native deck streaming use distinct login states. Reading liked songs does not prove native load eligibility. Provider URI, actual collection membership and load readiness are different facts. Local playlist methods do not write Spotify playlists.

The experimental native path may be usable while collection `playable_in_deck:false`, status `dj_streaming_available:false` or `account:null` are returned as placeholders. Do not discard the whole liked collection or diagnose sign-out from these fields. Native connected/streaming-permission/restoration state and an actual authorized guarded load determine the next step; a real source refusal remains authoritative. Musical planning can still inspect cached analysis.

For a follower shortlist, use current deck state for playing tempo and source ownership rather than only the original library BPM:

```sh
pd library suggest --deck A --energy keep
pd call library.suggest_next --args '{"deck":"A","playlist":"spotify:liked","max_stretch_percent":3,"allow_half_double":false,"limit":20}'
```

Bind collection IDs from the request/reads. Energy directions are `keep`, `raise`, `lower`; CLI `up`/`down` normalize to the latter two. Inspect scope completeness, exclusions, tempo stretch, key confidence, grid freshness and readiness. Cached suggestion reads start no analysis. `scope_not_cached` requires obtaining that collection within scope, not searching all tracks. Do not clear played history just to obtain more candidates. Let `$djing-art` choose among the candidates and their actual passages; the ranking does not select a transition.

## Import and maintain local records

For requested file import, use explicit existing paths on the machine running PulseDeck; `library.import` accepts a `paths` array or a `folder` for recursive discovery. Imports read file metadata only and never pre-analyze the collection. Missing beat/key/waveform analysis completes during the requested deck load before playback is enabled. Wait for the load receipt and preparation_pending:false before performing. Completed results persist across restarts without expiry; unchanged source content reuses them. The Library drawer combines local playlists and Spotify collections, with continuous automatic paging. Discover the schema before a new form. Inspect returned records and `status.result.analysis` for completed/error/active counts. A successful envelope does not prove every input imported. Do not reload a contributing deck to validate an import.

`library.remove` removes the selected library record. Do not expand a library cleanup into audio-file deletion. Reimport/reanalysis is not a substitute for the requested narrow tag or cue edit.

Free-form local tags use `library.set_tags` or `library track tags local:ID --tags JSON`. The array is a replacement: merge an “add tag” request with the current array before writing. Preserve unmentioned tags. The service trims empties, deduplicates case-insensitively, accepts up to 32 tags of at most 128 characters, and preserves tags through reimport/reanalysis. Re-read the record to verify. Do not invent genre/mood tags without evidence or the user's requested classification.

## Save the requested order

Use `playlist create --name NAME`, `playlist add ID --track local:TRACK_ID`, or `playlist reorder ID --tracks '[INTEGER_IDS]'`. Reorder maps to `playlist.edit action=replace`, replacing membership and order. Read the target immediately before writing and preserve unmentioned members unless the request calls for a reduced selection. Renaming, deleting and removing members use the discovered `playlist.edit` schema.

Validate selected IDs exist, are unique and are local before mutation. There is no playlist revision guard. Reconcile observed intervening edits; do not overwrite them under the assumption that an old read is protected. Duplicate IDs may produce `operation_failed`; use its message to correct the request. On failed replacement, re-read and verify prior membership survived. A prior successful create/add is a separate change and is not rolled back.

After saving, read `playlist.list` and compare exact `track_ids` against the intended full order. Sorted membership cannot prove sequence. If create times out, inspect existing playlists and receipt before retrying: another create with a new ID can produce a duplicate playlist. Use the same request ID for an identical uncertain retry within the runtime receipt window.

## Estimate set duration honestly

Report full source duration separately from a conditional performance estimate. At constant rates:

```text
played_set_seconds = sum((exit_source_i - entry_source_i) / rate_i)
                     - sum(overlap_wall_seconds_i)
                     + additional_loop_wall_seconds
```

Count each extra loop only once; if it is already included in the played time, do not add it again. Bound entries/exits by source duration and overlap by available material. For varying rate, use a piecewise estimate with stated assumptions. At a steady common tempo, beat-based overlap takes `beats * 60 / playing_BPM` seconds; declare meter for bars. Do not subtract trimmed intros/outros again after using trimmed windows. Actual elapsed performance is a separate observation.

If eligible material cannot reach the requested duration, state achievable duration/shortfall and alternatives. Local playlists do not support repeated membership; do not hide that limitation by reporting a duplicate sequence as saved.

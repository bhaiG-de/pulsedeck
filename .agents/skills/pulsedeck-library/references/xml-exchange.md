# Rekordbox XML exchange

Migration changes library/cue metadata and references local audio paths. It does not copy, retag or transcode audio PCM; WAV mix recording belongs to `$pulsedeck-control`. Export covers the local library/playlists, not writes to provider playlists.

Read the requested XML/destination, current library and relevant canonical cues first. Use an explicit absolute path and check its parent. Export protects existing files; `--overwrite` requires replacement intent. An illustrative pair of commands represents separate operations, not instructions to immediately import an export:

```sh
pd library export-rekordbox --path /tmp/pulsedeck-library.xml
# Only for a separately requested import of this file.
pd library import-rekordbox --path /tmp/pulsedeck-library.xml
```

## Export verification

Verify the saved XML parses as `DJ_PLAYLISTS`, track paths and record counts match the library, requested hot/memory marker names/timestamps are present, and playlist membership/order is correct. Export reads canonical source names and bindings. Main cue is not a separate exported marker.

The tempo marker serializes `Metro="4/4"` and `Battito="1"`; this does not prove actual source meter/downbeat. XML is not a complete round trip for analysis confidence, internal provenance, free-form tags or every internal region property. Report the fidelity relevant to the user's request rather than promising a full application backup.

## Import and partial results

Check referenced file URLs resolve locally. When existing cue data matters, export/retain current metadata before a requested migration. Import is not a transactional whole-library replace or perfectly idempotent merge: some records may succeed while others fail; matching files receive XML metadata/cue arrays; playlist nodes can create additional playlists; missing markers can clear existing hot/memory fields. Do not repeat an uncertain import with a new request ID.

An `ok:true` response contains successful records, not a guarantee every XML entry imported. Compare XML collection entries to returned records and inspect `status.result.analysis` (`total`, `completed`, `errors`, `active`). There is no separate import-status endpoint. If a job owns the import, inspect terminal state and step results as well. Report missing paths and retained successes without claiming rollback.

For relevant returned local IDs, verify stored BPM/grid offset, canonical cue points and bindings, marker names/timestamps, and exact playlist `track_ids`. Read `library.track` and `cues.get source:local:ID`; check the loaded projection only if already loaded. Preserve a contributing deck; do not reload it merely for migration verification.

On a canonical/projection discrepancy, inspect source identity/revisions and report the failed or stale update. Do not “repair” it by populating a second cue store or replacing unrelated manual anchors. Cancelling a job prevents later steps but may not stop an active import; inspect actual completion. Do not stop a live session to solve an XML error.

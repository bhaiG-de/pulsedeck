# Library tagging, metadata editing and XML preservation

Implemented 18 October 2026, following the [rekordbox tagging audit](rekordbox-tagging-coverage.md). These editing and classification tools apply to local tracks in the unified Library. Existing personal label chips, playlists, controller navigation, Spotify browsing/playback, saved analysis and cue controls remain available.

## Use

1. Open Library → local collection → **Filters / My Tags** → **Manage categories and tags**. Create, rename, reorder or remove categories and tags. Removing a category/tag removes its assignments; it does not remove audio, personal label chips, cues or analysis.
2. Select a track → **Edit metadata**. Edit title, artist, genre, comments, album, grouping, label, composer, remixer, year, track/disc numbers, ISRC or a custom field. Set a zero-to-five-star rating, a preset/custom RGB color and categorized My Tags.
3. **Save to library** persists a revision-checked patch. Untouched fields, embedded originals, personal labels, cues and analysis remain intact. Name edits also update matching loaded decks without reloading audio. New deck loads use saved display names.
4. **Write saved text edits to file** is separate. It writes explicit saved text edits, keeps an exact original-file backup beside the source, and replaces the source atomically. It does not write library classification into arbitrary binary rating/private frames. Malformed source tags are rejected rather than rewriting a partially recovered container. Ratings, colors and My Tags persist in the library and PulseDeck XML.
5. Filter My Tags using **All selected (AND)** or **Any selected (OR)**. Rating ranges, colors, literal search, playlists and sorting combine in the native query. Clearing filters restores the unfiltered library; no manual More button is introduced.

## Data and consistency

- `track_info` holds effective fields, explicit manual overrides, imported XML fields/attributes, ratings/colors and a metadata revision. Imported originals and explicit edits are separate, so exporting/reimporting legacy fields does not turn them into unsolicited file-write edits.
- Categories/tags have shared stable IDs and ordering. `track_my_tags` has both track→tag and tag→track indexes. The rating and color indexes support native filters. Existing per-track `tracks.tags` labels are preserved separately.
- File backfill updates metadata without audio analysis. Its versioned size/mtime stamp skips unchanged sources. ID3 POPM values are retained with their owner; the first available native ID3 rating uses the conventional 0 / 1–63 / 64–127 / 128–195 / 196–254 / 255 star mapping. Different applications can interpret POPM differently; raw values remain inspectable.
- Optimistic revisions reject stale saves. Failed tag assignments roll back the metadata patch. Deleting assigned taxonomy invalidates affected editor revisions. UI drafts do not overwrite unrelated fields.
- Application metadata edits never erase source tags. Explicit file writeback is tested on MP3, FLAC, Ogg Vorbis, MP4/AAC, WAV and AIFF, with exact original backups and identical decoded PCM. Private ID3 bytes, POPM and unrelated custom values are retained, including four-character TXXX names.
- The writeback checks exposed an AIFF demuxer boundary error: its packet/frame count included bytes of the next chunk. Local AIFF probing/decoding now uses the COMM frame count, preserving the actual sound chunk and avoiding metadata bytes at the tail.

## XML

Standard track attributes include genre, comments, rating, RGB color, album, label, grouping, composer, remixer, year, disc/track number and ISRC. Unknown TRACK attributes are retained. Attribute newlines/tabs are escaped explicitly. Ratings use rekordbox's 0/51/102/153/204/255 representation; colors accept RGB values and common prefixes. [Official XML format](https://cdn.rekordbox.com/files/20200410160904/xml_format_list.pdf).

The published format has no My Tag taxonomy. PulseDeck therefore adds optional `PULSEDECK_TAGS`, `PulseDeckTags` and `PulseDeckCues` extensions for global category order/unused tags, assignments, personal labels, exact custom/multi-value field values, and canonical cue identities/bindings. These extensions support PulseDeck round trips; another application's support must not be assumed. Raw legacy blank keys/control characters are preserved inside JSON, where they are safely escaped.

Partial cue imports retain existing points and occupied bindings; conflicting imported anchors are retained as additional points. A same-value XML BPM/grid import does not relabel or rerun saved analysis. Empty XML key attributes do not replace a missing key with an empty string.

Export uses an independent read-only SQLite snapshot and compact rows, so it does not hydrate cached waveforms/neural arrays or hold the foreground library lock. Bulk XML imports support `summary:true` ID receipts; full responses remain available by default. Database-only imports and metadata/classification writes do not wait for the audio callback to acknowledge an unrelated fence.

## Performance and verification

Native query targets remain p90/p95/p99 **25/50/100 ms**, 200-row pages over 100,000 tracks. One current match set, filtered set and ordered ID table are kept in SQLite; range paging avoids walking every preceding result at deep offsets. Statistics have a bounded cache. Local writes and external SQLite data versions invalidate these caches.

The cold-query follow-up now enforces **under 100 ms for every first request in the regression cohort**, separately from the warm p90/p95/p99 budgets. It includes first unfiltered and broad genre requests, combined My Tag/rating/color filters, and 24 cache-invalidated search + AND/OR + rating/color requests immediately after metadata edits, cycling all three sort orders at offset 90,000.

The search index stores the same SQLite-lowercased text as the original literal predicate, with case-sensitive trigram phrases. Exact substring checks therefore use the index without rereading every matching document; short queries retain their substring scan. The old auxiliary index is rebuilt once, without rewriting track metadata, analysis or cues. Literal punctuation, quotes, wildcards, Unicode, short queries, updates/deletes and index migration have regression coverage. [SQLite documents trigram phrase substring matching and case sensitivity](https://sqlite.org/fts5.html#the_trigram_tokenizer).

Classification filters now start from compact indexed classification rows; a covering `(color,rating,track_id)` index and indexed tag membership checks avoid several broad intermediate sets and unnecessary full-track lookups. All 512 supported tag selections still work, including combined playlist/BPM/search constraints. Broad duration totals use the covering duration index when matches exceed 1,000 and one quarter of the collection; sparse results retain indexed ID lookups. This avoids scanning full track pages for totals. Result ID sets use temporary memory and the main SQLite page cache is capped at 32 MiB. The three result sets contain only IDs and grow with the matching collection; audio/analysis payloads remain outside browser caches. Tag counts use indexed count subqueries. Common AND filters use indexed joins; selections beyond SQLite's join limit retain the complete 512-tag predicate path. Smaller WAL checkpoint batches reduce write stalls while full commit durability remains enabled. Metadata save events refresh tracks without fetching unchanged playlist membership arrays. Repeated pages retain bounded result caches and range paging. SQLite is optimized in development builds as it already is in release; the previous 226 ms benchmark used an unoptimized development SQLite.

The final on-disk regression run (100,000 tracks; 200-row pages; query caches initially empty) measured:

| Operation | First request | Warm p90 / p95 / p99 |
| --- | ---: | ---: |
| Unfiltered | 47.21 ms | 2.44 / 2.51 / 2.89 ms |
| Broad genre (all 100k matches) | 72.89 ms | 2.45 / 3.06 / 3.58 ms |
| AND + rating + color (all 100k matches) | 52.79 ms | 2.57 / 3.03 / 4.48 ms |

The 24 post-edit cold combined queries measured p90/p95/max **74.69/85.91/91.85 ms**. No prewarming or manual result-limit button is used. Measurements and the concurrent playback stress evidence are under `artifacts/query-stress-2026-10-18/`. These are finite measurements on the development machine, not a claim of unlimited library size or guaranteed physical 120 Hz presentation.

Tests cover taxonomy assignment/filter semantics, rename/delete, revision conflicts and rollback, metadata preservation through reimport, XML standard/custom fields and legacy keys, canonical cue collisions, all-format text writeback, deep sort paging and external-write invalidation. The browser regression exercises real editor/filter controls alongside bounded 100k-local/5k-Spotify tables. Workspace tests, Clippy with warnings denied, frontend tests/build and unified command discovery are required gates.

## Native collection round trip

The isolated release check used a copy of the actual **3,403-track** library plus six generated audio fixtures. Startup plus migration/backfill took **5.975 s**. Export of **3,409 tracks** took **115.7 ms**; reimport with stable-ID receipts took **8.091 s**. All original track columns (including labels and complete analysis JSON), playlists, membership and canonical cues matched their pre-change snapshots exactly after reimport and restart. There were zero unreadable files. Stale edits were rejected, combined filters matched full-record pages, and explicit file writeback retained an exact original backup. These are single native timings, not percentiles.

A native-only feedback loop was found and fixed: the two new getters must be classified as reads in the shared protocol. Otherwise category reads emit update events and repeatedly trigger themselves. Duplicate update emissions were removed, and category-count refreshes now follow relevant classification mutations. The final native check verifies getters do not emit mutation events. Saves measured tens of milliseconds after this correction. The editor keeps the track ID it opened with, so sorting/filtering changes after saving cannot close it or switch its target.

## Final real-library launch

The final release bundle was launched against the normal library, with a SQLite backup taken first. All **3,403 original records**, personal labels, playlists/membership and canonical cues matched the pre-migration snapshot exactly. All 3,403 metadata stamps were present, 3,252 tracks contained readable embedded fields and **zero** files had metadata failures. Duplicate canonical paths and missing search rows were zero. The first start with migration took **9.075 s**; restarting took **0.365 s** to the metadata completion log and reported **0 updated, 0 unreadable**. These timings include startup to that log, not a full UI presentation measurement.

The native UI was inspected: all 3,403 tracks, the metadata editor, AND/OR and rating/color controls, and category creation are available. No user metadata was edited during this inspection. The app is left open at category management. Only generated/copied test files were used for file writeback tests; no source music in the real collection was rewritten.

## Controller and native IPC follow-up

Stress scrolling exposed a page-publication race: a newly selected row could reach the controller before its expanded context. `controller.context` now accepts an optional `selected_track_id` and validates/commits both under one lock. Existing requests remain valid. The UI remembers acknowledged contexts and sends this atomic form only while context publication is pending, so normal selection does not resend the full list. The browser test delays native context acknowledgments to reproduce the race; a native controller test covers rollback and the existing context size limit.

The Unix socket server now waits for OS input readiness instead of sleeping 10 ms between accept attempts. Idle shutdown retains a bounded 50 ms wait and existing connection/response limits. Socket ownership, cleanup and round-trip tests remain required.

## Final concurrent native run

The final release ran for **180.47 seconds** against an isolated copy of the real 3,403-track collection plus **100,000 synthetic browse records**. Both real, cached local decks played with keylock at −4%/+6% tempo and loops. Master/cue output was muted for the test; the full audio processors remained active. No source audio files were rewritten.

The workload completed **844 revision-checked title/genre/comment/custom-tag/rating/color/My Tag saves**, **844 broad search/filter/deep-page queries**, **1,510 mixer changes**, and **120 native controller browse gestures**. Search, AND/OR, colors, rating ranges, title/BPM/duration sorting and offsets through 98,000 alternated throughout. The separate real-UI browser regression completed 30 save/filter cycles with both simulated decks advancing, and deliberately delayed controller-context acknowledgments to reproduce the pagination race.

The first native combined query took **70.12 ms**. Native socket round trips, including serialization and concurrent library contention, measured (nearest-rank percentiles, matching the UI timing summaries):

| Operation | n | p90 | p95 | p99 | Maximum |
| --- | ---: | ---: | ---: | ---: | ---: |
| Search/filter/deep page | 844 | 78.67 ms | 84.33 ms | **94.27 ms** | 177.02 ms |
| Metadata/classification save | 844 | 16.07 ms | 20.54 ms | **35.28 ms** | 194.12 ms |
| Mixer command acknowledgment | 1,510 | 20.56 ms | 22.97 ms | 31.11 ms | 48.88 ms |

The engine recorded **31,052 callbacks and zero deadline overruns** during the cohort; maximum callback work was **1.535 ms**. Both deck processors advanced by over 135,000 blocks and both decks remained playing. Controller scrolling completed without selection errors. Existing analysis, canonical cues and personal labels matched their pre-storm snapshots exactly. On initial deck preparation, one copied legacy cached track gained its missing encoded-file fingerprint; all other existing analysis fields were checked unchanged before the stress baseline was taken.

The first-query and p99 query targets are met in this cohort. Rare read/write stalls remain visible in the maxima above; this is not a hard guarantee that every request stays below 100 ms. Physical 120 Hz compositor presentation and audible output quality were not certified by this muted test. The earlier 109.74 ms native p99 failure is retained as `native-before-join-and-ipc.json`; it led to the join, IPC, checkpoint and controller fixes rather than lowering the targets.

Final gates: **406 Rust tests and 67 frontend tests passed**, Clippy passed with warnings denied, frontend/release/CLI builds succeeded, unified control coverage remained complete and whitespace checks passed. The final standalone on-disk latency log is `artifacts/query-stress-2026-10-18/search-performance.log`; native details, raw timing samples, controller snapshots and scripts are in the same directory.

After the stress instance exited, the updated release was launched on the normal library with a fresh SQLite backup. All **3,403 complete track rows**, metadata/classification rows, playlists/membership, tag taxonomy/assignments and canonical cues matched their snapshots exactly. Migration/startup took **1.034 s** and restart **0.381 s** to the metadata completion log, with **0 updated, 0 unreadable**, no duplicate paths and no missing search rows. The normal updated app is left open on Library.

# Embedded metadata indexing and performance

Validated 18 October 2026 on the local Apple Silicon workstation. This extends the [import/playback performance work](local-import-loading-performance.md); it does not change audio-analysis algorithms or source audio files.

## Behavior

- Imports and a bounded startup worker read embedded text/URL metadata: genre, comments, custom ID3 TXXX/WXXX, and other readable standard/custom fields. Real fixtures cover MP3, FLAC, Ogg Vorbis, MP4/AAC, WAV/RIFF, AIFF, ID3v1 and APE tags, Unicode, multiple values and comment descriptions.
- Store format, raw/normalized key, description, language and value separately from editable application labels. Search includes keys, descriptions and values. Compact list pages omit the detailed metadata payload.
- No PCM/audio analysis or artwork decoding is needed for this index. File size/mtime/parser-version stamps skip unchanged reads across restart and reimport. Failed reads retry on a later startup/import, retaining the previous readable metadata.
- Files are read outside library/engine locks; writes use short SQLite transactions and reject superseded reads. Foreground import takes precedence over the background worker. Completion refreshes active searches without resetting an unfiltered list's scroll.
- Relaxed parsing plus existing tolerant container probing and bounded ID3-prefix recovery retain readable tags from malformed files. Invalid individual fields and opaque binary/private data are not reconstructed. Source files are never repaired or rewritten.
- Bulk import can request `summary: true` for stable-ID receipts. The Library uses this to avoid returning cached waveform/model payloads for thousands of existing tracks. Existing callers retain full-record responses by default.

## Data preservation and native integration

An isolated release app imported a SQLite backup of the actual 3,403-track library. Startup plus full backfill took **5.183 s**: 3,403 stamped, zero unreadable, 3,252 with embedded fields. The remaining files contained no readable tags. All original track columns were compared by per-row SHA-256, and playlists, membership and canonical cues were compared exactly: unchanged. No audio-analysis operation was run.

Six fresh format fixtures imported in **41.9 ms**, with analysis version zero and no waveform. Reimport took **40.0 ms** and retained every ID. A 1,070-track House folder reimport took **110.7 ms** using ID receipts: no added duplicate paths, missing FTS rows or analysis payloads in compact pages. These are single native measurements, not percentile claims.

The first reader rejected 36 real files with malformed dates/stacked tags/encodings. Recovery was checked against all 36; the complete isolated pass above includes them and has zero unreadable-file failures. This means readable fields were recovered, not that every malformed field became valid.

The final normal-library startup recovered the 36 previously failed reads, with **zero unreadable files**. A second startup reported **0 updated, 0 unreadable**, confirming persisted stamps skipped all unchanged files. All original columns, playlists, membership and cues still matched the backup; duplicate paths and missing search-index rows were both zero. Native CLI searches of actual genre, comment and custom values succeeded. The actual Library UI showed **203 Afro House matches**, then was cleared back to all **3,403 tracks** and left open.

Workspace Rust tests, Clippy with warnings denied, the release build and isolated/native checks pass. Earlier frontend tests/build, 100k-local/5k-Spotify browser regression and control coverage checks also pass.

## Search measurements

Rust debug SQLite test, synthetic 100,000-track library, 200-row pages, 100 requests per cohort. Existing large analysis JSON is excluded from paging. Targets are p90/p95/p99 **25/50/100 ms** for native queries, independent of frontend presentation time.

| Operation | p90 | p95 | p99 |
| --- | ---: | ---: | ---: |
| Unfiltered pages through offset 98,010 | 9.53 ms | 10.11 ms | 10.21 ms |
| Distinct literal track searches | 1.13 ms | 1.24 ms | 1.36 ms |
| Distinct custom metadata value searches | 0.90 ms | 0.92 ms | 1.01 ms |
| Pages of a genre matching all 100,000 tracks | 14.13 ms | 14.75 ms | 16.46 ms |

The first broad genre search takes **183.22 ms** to construct its result set; it is included in the cohort but is above the 100 ms p99 budget as an individual request. Subsequent pages reuse one SQLite temporary ID set and ordered indexes. The former implementation repeatedly filtered/sorted broad matches and took approximately 264/285/296 ms at p90/p95/p99. Cache invalidation follows local writes and SQLite external data versions. One current search set and a bounded statistics cache avoid retaining unbounded per-query results.

Measurements demonstrate bounded work for the tested sizes, not unlimited scalability or immunity to host saturation. Artifacts are retained under `artifacts/metadata-index-2026-10-18/` (ignored local data).

## rekordbox coverage boundary

The [installed-artifact tagging audit](rekordbox-tagging-coverage.md) confirms that flattened My Tag names can be searched when rekordbox wrote them into file comments. Library-only taxonomy cannot be recovered from loose files. Separate gaps remain: My Tag categories/AND-OR facets, structured rating/color filters, metadata inspection/editing/writeback and broader XML field preservation. No implicit taxonomy or labels are invented from arbitrary comments.

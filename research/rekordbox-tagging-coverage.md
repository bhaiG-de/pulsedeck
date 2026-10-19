# rekordbox tagging coverage compared with PulseDeck

Investigated 18 October 2026. Scope: file metadata indexing/backfill and its boundary with rekordbox's library tagging. Research only; no application source changes, source-file writes, rekordbox imports, library mutations or preference changes were performed.

## Result

PulseDeck's new metadata reader addresses the current file-import task: readable embedded text/URL tags become searchable without audio analysis. This is separate from a rekordbox-style **My Tag category system**, tag editing tools and structured filtering. Importing an audio file cannot recover category assignments that exist only in another application's database.

When rekordbox has already written My Tag names into an audio file's Comments field, PulseDeck's existing reader/search can find those flattened names. That does not recover category names, stable tag IDs, relationships or category-aware AND/OR filtering. No additional taxonomy system is needed to complete the current metadata backfill.

## Documented rekordbox behavior

The 7.2.14 manual documents editable My Tag categories/names, multiple assignments, AND/OR filters, separate rating/color filters, and an option to append My Tag to Comments. Track information can be edited; Reload Tag replaces edited library information with reread file information. Detected-key writeback is separately configurable. These are documented behaviors, not a benchmark or a complete specification of installed 7.2.19. [Official manual, printed pp. 25–31, 230–231, 239](https://cdn.rekordbox.com/files/20260409151936/rekordbox7.214_manual_EN.pdf).

A vendor staff answer for rekordbox 6 says the Comments option preserves existing comment text and appends slash-separated tag names between `/*` and `*/`, saving changes in file metadata. Its age and format-dependent write failures limit extrapolation to every rekordbox 7 file. [Official vendor answer, 12 January 2021](https://forums.pioneerdj.com/hc/en-us/community/posts/900001887546-How-does-Add-My-Tag-to-the-comments-exactly-work-and-how-does-it-deal-with-comments-that-were-there-before).

The official XML format includes Genre, Comments, Rating, Colour, Album, Label, Grouping and other track fields. Rating uses 0, 51, 102, 153, 204, 255 for zero through five stars. The published format has no My Tag category/assignment element. That is a limitation of the published XML contract, not proof against every undocumented export extension. [Official XML specification](https://cdn.rekordbox.com/files/20200410160904/xml_format_list.pdf).

## Installed-artifact evidence

Target identity is unchanged from the [import investigation](rekordbox-import-analysis.md): rekordbox **7.2.19.0342**, main executable SHA-256 `07dcecbaf304f777e435539c83c7dcc02e41de1eaf2e0a75e8a9852e4411a244`. Fresh REA identity record: `ev_4c807bccae531e471f054abaabe0f2dd1d332019507819a2e188d61dfc8aa82e`. Matching retained REA Mach-O record: `ev_0cb56fe857156135bf806b67c3ff43ac5eb91abe354136a7ecf74dc5d45d27ad`, [complete output](evidence/rekordbox-7-installed/main-macho.json).

The following are **shell static observations, not REA decompiler Evidence or runtime execution**. Commands used Apple LLVM `llvm-objdump --macho --arch=arm64 -d --dis-symname SYMBOL`, `c++filt` and `strings` on the matching artifact. Raw symbols/addresses are in the retained [symbol inventory](evidence/rekordbox-native-symbols.txt).

| Observation | What it establishes | Limit |
| --- | --- | --- |
| Native SQL string `select * from djmdMyTag where rb_local_deleted = 0 and ParentID = 'root' order by Seq`; another selects children by `ParentID`; assignment query selects `MyTagID, ContentID` from `djmdSongMyTag`. | The shipped program contains an explicit parent/order/assignment storage model, corroborating the documented category boundary. | These are query strings, not a dump of the user's live schema or data. |
| Shipped helper model bytecode contains `djmdMyTag`, `ParentID`, `Name`, `Attribute`, and separately `djmdSongMyTag`, `MyTagID`, `ContentID`, `TrackNo`. | A second shipped component references distinct taxonomy and assignment models. | Bytecode string constants alone do not establish field types, complete schema or executed behavior. No bytecode was executed. |
| `BrowseListViewer::updateCommentWithMyTag`, entry `0x10079e338`, calls `getCurrentWriteMytag` at `0x10079e368`, then conditionally `getMyTagList`; literal strings at `0x10079e8ac`, `0x10079ed28`, `0x10079ed9c` are ` /* `, ` / `, ` */`. | The installed build contains the documented optional flattened-comment representation. | Does not show the current preference value or prove all tracks have been written. |
| Same function reads an existing field, calls `contains`/`indexOf`/`substring`, appends strings, compares combined length against `0xfd` at `0x10079ed5c`, then calls `DatabaseIF::modifyTrackData` at `0x10079ede4`. | Static control flow supports preservation/replacement of a marked suffix and a length gate. | Exact handling of malformed delimiters, Unicode length and overflow was not fully reconstructed; do not treat 253 as a guaranteed universal user-visible limit. |
| `DatabaseMediator::modifyTrack` calls `modifyTagData` at `0x100c46cac`; that calls `Metadata::SetComment` at `0x100c47ca8` and `addEditableMetadata` at `0x100c47db4`. `Metadata::update` checks `File::hasWriteAccess` at `0x1009e3674`, calls `TagLib::WriteTag::setComment` at `0x1009e370c`, and `save` at `0x1009e38a4`. | This is an actual compiled metadata-write path, corroborating vendor file-writeback guidance. | Execution/queuing, current settings and successful persistence for any particular file were not observed. |

The packaged model files are [djmdMyTag.jsc](evidence/rekordbox-agent/selected-artifacts/jsc_arm64/models/djmdMyTag.jsc) and [djmdSongMyTag.jsc](evidence/rekordbox-agent/selected-artifacts/jsc_arm64/models/djmdSongMyTag.jsc), selected in the retained [artifact manifest](evidence/rekordbox-agent/selection-manifest.json). Native binaries were not patched and vendor code/assets were not copied into implementation.

## Actual PulseDeck coverage and bounded gaps

| Area | Current evidence in PulseDeck source | User impact / scope |
| --- | --- | --- |
| Embedded file tags | `read_file_metadata` in [embedded_metadata.rs](../crates/deck-engine/src/embedded_metadata.rs) uses Lofty with audio properties/artwork disabled. Stores format, raw/standard key, value, description and language. Reads text/URL items and custom ID3 TXXX/WXXX remainder; splits NUL-separated values. Fixtures cover MP3, FLAC, Ogg, MP4, WAV, AIFF plus ID3v1/APE and custom/comment details. | Covers searchable embedded text; does **not** mean every possible binary/private tag or third-party serialized payload is semantically decoded. Embedded binary ratings or private blobs are not a structured rating system. |
| Existing My Tag text in Comments | `metadata_index::search_text` includes field keys, descriptions and values; no special parser is needed for comment words. | Already searchable if written into a source file. Do not invent categories by splitting arbitrary comments or create app labels automatically. |
| My Tag categories and shared identities | `TrackRecord.tags` is a per-track string array; `set_tags` trims, deduplicates case-insensitively, caps 32 labels/128 characters. [TrackTags UI](../frontend/src/TrackTags.tsx) adds/removes individual labels. | No shared category/tag entities, global rename/delete/reorder or assignment import. This is a separate feature, not a missing embedded-file parser. |
| Metadata inspection and editing | Structured file metadata is stored, but browser rows expose editable app-label chips. No general embedded-field inspector/editor was found in the inspected frontend. | A matched comment/custom field can be searchable without being visible as a chip. Users may need a details panel to explain search matches; optional follow-up. |
| Genre/comments/album/label | Stored as embedded fields when readable and indexed in `metadata_search`; no dedicated editable canonical fields in `TrackRecord`. | Search works; dedicated field columns, facet counts and edits are a separate UI/model feature. |
| Rating and color | No canonical rating/color fields or editor/filter in the inspected library/browser model. | Source text can be searched when represented as readable metadata, but cannot behave as star ranges or color facets. Database-only rekordbox values cannot come from loose audio. |
| Search semantics | [library.rs](../src-tauri/src/library.rs) `browser_page` searches one literal case-insensitive substring over combined title/artist/BPM/key/app-label/embedded text. Uses trigram FTS for queries at least three characters and verifies with `instr`. | Fast broad metadata search. No field selector, tag AND/OR facets, separate word queries, rating/color predicates or fuzzy search is implied. Language and format are retained but not included by `search_text`. |
| Refresh/backfill | [metadata_index.rs](../src-tauri/src/metadata_index.rs) uses a parser-version/size/mtime stamp, retries failures on subsequent startup/import, checks changed-during-read, and performs I/O outside engine/library locks. Storage updates only file metadata/search/stamp/error. | Preserves saved analysis, cues, app labels and playlists. No continuous filesystem watcher was found; successful cached reads need a stamp/version change to be reread after parser changes. |
| File writeback | Reader performs no source writes; app labels are stored in SQLite. | Deliberate boundary for this task. PulseDeck label edits will not appear in another DJ app merely by copying audio files. |
| XML migration | `XmlTrack` reads ID/location/name/artist/BPM/key/tempo/cues; `export_xml_named` writes the same limited track attributes. Neither transfers Genre, Comments, Rating, Colour, Album, Label, app labels or all embedded fields. | A real exchange gap: rekordbox library-only genre/comments/rating/color can be omitted even if PulseDeck's loose-file reader is complete. Add explicit XML fields only in a separately scoped migration change; XML cannot be assumed to restore My Tag taxonomy. |

## Read-only check of the current collection

A read-only SQLite connection (`mode=ro`) inspected persisted metadata during the ongoing production backfill. This was a consistent query result, not an atomic snapshot of every current source file or a new read of all audio files:

- 3,403 track rows; 3,216 had nonempty embedded metadata at that moment.
- 670 tracks had comment-like fields.
- No comment-like field contained a complete `/* … */` block. Across **all** fields, three matches occurred in large Vorbis `TRAKTOR4` strings. Those are not evidence of My Tag comments and must not be auto-promoted into labels.

This bounded negative check does not prove the user has no My Tags: other tracks were still being processed, database-only assignments are outside this index, and arbitrary unmarked words cannot be attributed to rekordbox. No track names or private tag values are reproduced here.

## Recommended scope

Finish metadata backfill, surface unreadable-file counts and verify representative genre/comment/custom-field searches. Preserve embedded fields and user app labels separately. Report taxonomy/filter/rating/color/XML limitations honestly; do not silently expand a file-indexing fix into a new tagging product.

Optional follow-ups, in practical order: a read-only metadata details view; standard XML genre/comment/rating/color preservation when migration is requested; and only then a shared category/tag system if the user wants that workflow.

No active rekordbox experiment, complete native decompilation, or exhaustive cross-format metadata conformance test was performed. File indexing cannot reconstruct database-only information.

## Follow-up implementation

The audit above describes the state before the requested feature work. Categorized My Tags, AND/OR and rating/color filters, a metadata editor with explicit backed-up text writeback, and standard/PulseDeck XML preservation are now implemented. See [Library tagging and XML](library-tagging-and-xml.md) for the implementation, verification and remaining interoperability boundaries.

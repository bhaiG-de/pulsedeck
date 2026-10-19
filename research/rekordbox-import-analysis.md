# rekordbox import, analysis and local deck loading

Investigated 18 October 2026. Scope: installed rekordbox 7.2.19.0342 at `/Applications/rekordbox 7/rekordbox.app`; static inspection and previously retained passive runtime evidence. No imports, playback actions, preference changes, library writes, account actions or target launches were performed.

## Findings

**The requested separation is supported by rekordbox: adding local files/folders and analyzing their audio are separate operations.** Its documented Auto Analysis setting controls analysis after import. The evidence does **not** establish that rekordbox's exact policy is “analyze only after pressing Play.” That timing should be treated as a PulseDeck product decision.

The official 7.2.14 manual describes Import Track and Import Folder, adding entries and displaying tags, with analysis conditional on Auto Analysis. When it is disabled, users can select Analyze Track manually. It also distinguishes analysis completion from import completion. These are documented behaviors for 7.2.14, not measured timings for the installed 7.2.19 build. [Official manual, printed pp. 14–15 and 230–231](https://cdn.rekordbox.com/files/20260409151936/rekordbox7.214_manual_EN.pdf).

**A completed analysis cache does not remove playback preparation.** In the installed native code, source opening and saved waveform/beat loading are distinct calls. Thus a library row showing BPM/key is insufficient evidence that its playable audio is already decoded or buffered. The inspected material does not establish the cause or duration of PulseDeck's delay; that requires PulseDeck tracing and measurements.

## Evidence ledger

| Evidence | Observation | Limit |
| --- | --- | --- |
| Fresh REA `inspect_plist`, `ev_4c807bccae531e471f054abaabe0f2dd1d332019507819a2e188d61dfc8aa82e` | Version 7.2.19.0342, bundle `com.pioneerdj.rekordboxdj`, main SHA-256 `07dcecbaf304f777e435539c83c7dcc02e41de1eaf2e0a75e8a9852e4411a244`. | Identity, not behavioral execution. Complete inline output was returned in the investigation tool transcript. |
| Retained REA main Mach-O, `ev_0cb56fe857156135bf806b67c3ff43ac5eb91abe354136a7ecf74dc5d45d27ad`, [record](evidence/rekordbox-7-installed/main-macho.json) | Matching shipped main executable identity and native dependencies. | Existing symbol collections have bounded coverage. |
| Shell static ARM64 disassembly of `db::DatabaseMediator::ImportManager::run`, entry `0x1001dcda4` | Calls `addTrack` at `0x1001dd084`, `create_analysis_file_for_import` at `0x1001dd1a0`, playlist mutation methods and thread cancellation/progress methods. | Direct calls are observed; a method name containing “analysis” does not prove full beat/key inference is performed. Import path also handles XML, sampler and cloud cases. |
| Shell static ARM64 disassembly of `create_analysis_file_for_import`, entry `0x101a17528` | Opens/creates analysis-file handles and paths through `CAnalyzerIF`, and includes cloud-specific branches. | Analysis-file bookkeeping is not equivalent to executing full analysis. All branch conditions and callees were not reconstructed. |
| Shell static ARM64 disassembly of `db::DatabaseMediator::analyzeCollectionLoad`, entry `0x101499d44` | Reads stored track flags, calls `isAnalyzedOldVer` at `0x101499d84`, `0x101499e28`, `0x101499e58`, and conditionally reaches `execAnalyzeCollection`/`analyzeCollection` at `0x101499e00`/`0x101499ea0`. | Supports a conditional analysis path. Does not establish every upstream trigger, exact flag definitions, active preferences or analysis completion ordering against Play. |
| Shell static ARM64 disassembly of `djplay::UiPlayer::loadSongFile`, entry `0x102215cf4` | Calls `ReaderOwnerTmpl<Float2>::loadFile` at `0x102215fd8`; calls `getWavPointFromAnalyzeFile`, `getBeatPointFromAnalyzeFile`, `getWholeWavPointFromAnalyzeFile`, and `DjplayDB::getWavePoint/getSongInfo`; later calls `eventLoadFile` at `0x102216540`. | Confirms separate source/metadata paths in compiled code. Does not measure decoder speed, buffered duration or time to sound. |
| Retained passive [process sample](evidence/rekordbox-process-sample.txt), lines 1787–1804 | `PageBufferedInputStream::run` and a `PagingAudioSource` worker were waiting on events during that sample. | Existing sample was not a controlled bulk-import or deck-load benchmark. It supports worker presence, not exact current paging or latency policy. |

The shell static observations above are **not REA decompiler Evidence**. They were collected with Apple's `/Library/Developer/CommandLineTools/usr/bin/llvm-objdump --macho --arch=arm64 -d --dis-symname SYMBOL` against the hash-matched binary and demangled with `c++filt`. Exact raw symbols and entry addresses are available in the retained [symbol inventory](evidence/rekordbox-native-symbols.txt). No vendor code or assets were copied into PulseDeck.

REA tools were absent from this agent's MCP inventory. Recovery used `npx -y rea-agents@latest doctor --client codex --json`, which exposed the login shell's old Node 22.14 and missing native providers. The already-installed Node 24.21 was then selected only for the CLI process and REA 6.7.0 `inspect-plist` succeeded. No Node upgrade, native provider installation, skill replacement or MCP configuration write was made. No native REA session was opened.

## Implications for PulseDeck

These are design recommendations, not claims about unobserved rekordbox internals:

1. Bulk import should recursively enumerate supported paths, read lightweight metadata, deduplicate and persist collection entries without decoding every full track or scheduling beat/key analysis.
2. Decode only the selected track for playback; reuse saved BPM/key/beat/waveform results when their fingerprint and analysis version match. Metadata cache presence and decoded PCM readiness should remain distinct states.
3. If analysis is missing, publish playback readiness before running expensive beat/key work. Analyze in a bounded background worker and update the waveform/grid afterward. A hard dependency between Play and finished analysis would preserve the delay the user wants removed.
4. Measure metadata reads, decode, waveform preparation, cache reads and engine installation separately. Avoid making playback wait for unrelated import jobs or analysis locks. A paged playback engine could further reduce first-sound latency, but would be a separate architectural change requiring seek/scratch/loop validation.

## Question coverage

| Question | Status |
| --- | --- |
| Can local folder/file import be separated from full analysis? | Answered by official documentation; matching installed static boundaries corroborate it. |
| Does this exact installed build analyze only after Play? | Unresolved; no active import/load experiment was authorized for this subtask. |
| Does an existing analysis eliminate audio-source preparation? | No; separate compiled source-open and saved-data-loading calls are observed. Exact latency remains unknown. |
| Does rekordbox require whole-track PCM decoding before sound? | Unresolved; worker and paging evidence is insufficient to prove the complete policy. |
| Was PulseDeck's specific delay diagnosed here? | No; the main implementation investigation owns that measurement. |

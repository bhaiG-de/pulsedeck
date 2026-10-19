# Library, import and preparation performance

Validated on 18 October 2026 on this Mac (18 CPU cores, 24 GiB RAM). Measurements are local observations, not guarantees on other hardware, remote drives or arbitrary media lengths. The benchmark budget covers music up to eight minutes.

## User behavior

- One **Library** drawer contains local playlists and Spotify collections in one sidebar. Existing local tags, playlists, sort/search, drag/load controls and Spotify account, Connect, widget, album and analysis controls remain available. Next-track suggestions remain separate.
- File/folder and rekordbox XML imports register tags and duration, without decoding or pre-analyzing the collection. Recursive folder discovery handles partial reimports using canonical file paths; existing IDs, analysis, cues, tags and membership survive.
- A selected local track is decoded once, then completes missing key, tempo and waveform analysis before the load receipt becomes ready. Play, cue preview, hot-cue launch, jog and scrub audio are gated in the engine and renderer. Rejected premature Play commands do not cancel preparation. Spotify preparation already analyzes before committing the provider source.
- Completed analysis is stored in SQLite with no TTL or capacity eviction. Restarting the application does not discard it. Full encoded-file hashes verify unchanged local sources cheaply; changed containers/tags fall back to decoded-content identity. Changed audio invalidates incompatible derived results. Explicit reanalysis remains available.
- Settings → Update library analysis and Spotify's opt-in collection/page analysis remain available. Passive import does not enqueue that work.

## Performance budgets

| Operation | p90 | p95 | p99 |
| --- | ---: | ---: | ---: |
| First complete analysis, ≤8 minute music | 10 s | 15 s | 25 s |
| Cached local preparation | 500 ms | 750 ms | 1000 ms |
| Native metadata page or indexed search, 100k collection | 25 ms | 50 ms | 100 ms |
| Browsing render work | 16.7 ms | 20 ms | 33.3 ms |

Frame cadence is recorded separately from render work. Browser/device presentation timing requires actual frame measurements; a low React commit duration alone does not establish 60 FPS.

## Blockers removed

1. Development builds previously optimized the engine but left codecs and SHA-256 unoptimized. A cached 6:12 MP3 took 18.50 seconds in the old package. Dependency optimization reduced that to roughly 0.6 seconds.
2. Cached loads rehashed approximately 130 MiB of decoded PCM. Persisted encoded-content stamps cut fingerprint verification to about 27 ms for that track while retaining decoded-content fallback for modified files.
3. Independent rhythm, tonal-key and waveform stages ran serially. They now run on separate bounded workers with unchanged algorithms and cache identities.
4. Independent meter HMMs ran serially. They now run concurrently, reducing in original meter order to preserve deterministic tie-breaking. The model lock is released before temporal postprocessing so the other deck can infer concurrently.
5. Two inference threads underused this machine. The default now uses available CPU parallelism minus six, clamped to 1–12. The existing environment override remains supported. Constant model weights are packed once at model load. On the same 6:12 source, the old 2/4/8-thread runs took 13.74/11.47/7.98 seconds; prepacking with 12 threads took 6.08 seconds, with identical beat/downbeat timestamps and waveform arrays. Sixteen threads took 5.71 seconds but left less capacity for playback and the interface. RTen’s iterator/runtime and Rayon scheduling dependencies are also optimized in development builds.
6. Rapid requests could overlap obsolete local preparations. Each deck has one preparation worker and superseded queued requests are rejected before decoding.
7. Loading by local track ID read every library record, including waveforms. It now resolves one indexed path.
8. Browser pages omitted waveform output but extracted fields from large analysis JSON. Compact scalar metadata is now maintained transactionally in a separate table, with indexes for sorting and a bounded, write-invalidated count/duration cache. Literal searches of at least three characters use a trigram index before the exact substring predicate; short searches retain exact scan behavior.
9. Both local and Spotify tables mounted the entire fetched collection. Both now mount a small measured window, retain focused tag drafts, and automatically fetch more metadata near the bottom. Album artwork is lazy and decoded off the UI thread into bounded persistent thumbnails. Spotify collection pagination also advances automatically.
10. Transport polling cloned full waveform/model metadata thirty times a second before dropping it. The engine now projects only the compact transport metadata under the existing lock. Full metadata consumers retain the original snapshot.
11. Platter rotation used React transport updates. It now uses compositor interpolation synchronized to the native clock, with immediate jog feedback and stale-clock protection. Detailed waveforms now prerender an expanded, full-resolution strip into separate waveform and marker canvases; Motion's `motion/mini` translates those layers together using the native source clock. Fixed played shading and the playhead do not repaint the waveform. No springs, waveform downsampling or beatgrid simplification were introduced. Repainting is limited to strip recentering, new raster tiles, gestures and actual content changes. Mixer control updates no longer invalidate unchanged waveform pixels. Seek, pause, source changes, loop wrapping and stale-clock protection retain their transport guards. Full-window SVG grain and large blurred shadows on moving surfaces were simplified.
12. Virtual table spacer rows repeatedly changed table layout. Positioned, measured rows now use the installed virtualizer's direct DOM updates and avoid synchronous React flushes during scrolling. Both sources retain automatic paging, keyboard/controller selection, tag editing, album controls and drag/load controls.
13. The 96-tap resampler repeated bounds, loop wrapping and phase checks for every tap. The common interior path now validates once and traverses the same samples and coefficients. Exact sample parity against the original implementation covers all phases, rates, file boundaries and loop boundaries; the original boundary path remains. The callback remains allocation-free.
14. Local decoding allocated and cleared an interleaving buffer for every packet and repeatedly grew the full PCM vector. A bounded container frame hint reserves PCM once, and the packet conversion buffer is reused. Mono, stereo and a partial final packet are checked sample for sample.
15. Seven valid English files had malformed stacked ID3 tags. Tolerant probing recovered all seven without editing their audio files.

## Measurements

Raw measurements are under `artifacts/library-performance-2026-10-18/`; the final Motion/release runs and earlier contended comparison are in its `motion/` subdirectory. Percentiles use nearest rank.

| Final measured operation | Samples | p90 | p95 | p99 | Budget |
| --- | ---: | ---: | ---: | ---: | --- |
| First complete analysis, deterministic real 1–8 minute cohort | 99 fresh | **9.399 s** | **10.615 s** | **22.971 s** | Pass |
| Cached local preparation, 6:12 MP3 | 100 | **348.9 ms** | **475.3 ms** | **793.6 ms** | Pass |
| Native SQLite metadata page, 100k records | 100 | **9.28 ms** | **9.55 ms** | **9.85 ms** | Pass |
| Native SQLite indexed literal search, 100k records | 100 | **2.48 ms** | **2.72 ms** | **2.92 ms** | Pass |
| Native steady scrolling render work, final release | Latest 120 commits | **1 ms** | **2 ms** | **3 ms** | Pass |
| Retina browser fixture scrolling render work, 100k local / 5k Spotify | Latest 120 commits | **0.7 ms** | **1.3 ms** | **4.7 ms** | Pass |

The 100-request analysis cohort ran with two native decks playing and some package compilation and UI testing; all 100 completed preparation, maximum 22.971 s, stderr empty. Ninety-nine requests executed every analysis stage; one duplicate-content memo hit is excluded from the first-analysis percentiles above. It uses the same cohort as the earlier 11.712/13.878/20.756 s development run that missed p90. Analysis algorithms, model weights and analysis precision were retained. The subsequent PCM allocation cleanup affects decoding rather than the analysis calculations.

The final cached cohort also ran alongside a separate first-analysis worker and two-deck native playback, without package compilation. All 100 cached loads executed **zero analysis stages**; median 311.0 ms and maximum 1018.0 ms. An earlier release cohort during simultaneous analysis and LTO compilation missed the cache budget (1028/1194/1300 ms); that measurement remains saved. These budgets do not promise immunity to arbitrary host saturation.

The browser regression mounts about 28 local / 30 Spotify rows after automatic paging, searches an unfetched track, retains an unsaved focused tag draft, and confirms no manual More buttons or browser errors. The final moving-waveform sequence used 14 detailed canvas paints instead of the earlier approximately 418, with the same full-resolution waveform data. It also checks aligned running compositor animations, precise source positions after seeks near both file edges, platter clock progression, and continued mixing without unnecessary waveform invalidation.

### Native audio and presentation

- Ten-minute release CoreAudio stress: **zero deadline overruns in 106,657 callbacks**, worst 4.227 ms. Both decks looped with headphone cue processing and one deck keylocked at +6%, while analysis, library browsing and builds ran. A second release run had **zero overruns in 58,260 callbacks**, worst 1.745 ms.
- Final four-minute release validation, both decks keylocked at different tempos, loops, echo/reverb and crossfader changes, plus two fresh ~8-minute analyses: **zero overruns in 42,517 callbacks**, worst 1.385 ms. Both processor clocks advanced and loop resets occurred. Complete counters are saved separately.
- The native C++ processor audit records **zero allocations and zero frees** across 2,000 process calls and 21 seeks. The Rust callback allocation checks also pass.
- Final native scrolling JavaScript frame cadence was **18/23/27 ms** at p90/p95/p99 in the latest 120-frame window; steady scrolling render work is separately reported above. The browser fixture's scrolling frame intervals were 17.4/17.7/19.7 ms across 229 frames. These include JavaScript scheduling and are not physical compositor presentation measurements.

The display is configured for 120 Hz. The 8.33 ms stretch target for physical presentation is **not certified**. Cached waveform layers and platters can animate independently of JavaScript page updates, but no claim of measured native 120 FPS is made. See [Motion performance](https://motion.dev/docs/performance) and WebKit's [animation frame-rate explainer](https://webkit.github.io/explainers/animation-frame-rate/) for the distinction.

A prior real isolated desktop check rejected premature Play, completed first analysis, stopped through CLI, restarted against the same SQLite database and loaded the cached track in **438.1 ms**. Persisted stage stamps, neural events and waveform bins were unchanged after restart.

## Import audit

The final library contains 3,403 records. House Pack: **1,070/1,070** files. English Pack: **2,322/2,324** files; the remaining two files are zero bytes and contain no audio. No repeated canonical paths exist. There are **82 byte-identical groups / 84 extra copies** at different paths in the source collection; no copies or records were removed. Paths and SHA-256 groups are in `artifacts/library-audit-2026-10-18/`.

## Verification and limits

Workspace Rust tests, Clippy with warnings denied, frontend tests/build, control coverage and skill packaging checks pass. Regressions cover deferred metadata import, reimport preservation, changed-source invalidation, precise SQLite timing persistence, playback gating including stale queued starts, full cache reuse, malformed metadata recovery and large SQL/virtualized browsing. The optimized release bundle is rebuilt and opened with Library available. The final frontend suite has 67 passing tests; the decoder/cache suite has 22 passing tests. Resampler equivalence and existing spectral, alias-rejection, stereo, keylock and seek checks pass. A production-readiness issue found during relaunch—UI command acknowledgments waiting forever for display frames in a hidden WebView—now has a 100 ms timer fallback, canceled frame callbacks and a suspended-frame regression test. Native close/open Library acknowledgments after relaunch took 55/73 ms.

Track PCM still lives in bounded native RAM for loaded decks; this change does not introduce streaming disk playback. Table DOM stays bounded while compact metadata for visited pages accumulates. Stable item-key callbacks preserve the virtualizer’s measurement cache during scrolling. The table is promoted to its own compositor layer and tag fields retain highlights without blurred shadows. Embedded artwork is lazy. The editable Tags column holds application labels; a separate [embedded metadata index](metadata-indexing-performance.md) now searches genres, comments and custom fields without changing those labels or redoing audio analysis. Overview canvases skip subpixel-equivalent redraws; force invalidations still refresh cues, metadata and size changes. There is no finite benchmark that proves unlimited library size. Spotify delivery latency remains dependent on the provider and network; its analysis data is persisted, but provider audio is not written to this cache. Unknown/failed musical estimates remain visible as such rather than fabricated.

See [the independent rekordbox investigation](rekordbox-import-analysis.md) for official documentation and native evidence supporting separate import and analysis operations, including the limits of exact Play timing evidence.

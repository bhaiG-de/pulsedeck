# PulseDeck implementation status

15 October update: automatic cue generation and structural classification are removed end to end. Manual/imported cue knowledge and BPM/beatgrid/key/waveform analysis remain. Older dated observations below describe their historical builds, not the current cue feature set.

Updated 2026-10-14. Implementation and validation are recorded separately from the earlier research/design documents. The requested full roadmap remains the target; native Spotify track loading and desktop cue/tempo/crossfader operation are verified. The adapter remains experimental and the full roadmap is not complete.

The latest local session-feedback implementation and its validation are recorded
in the final section below. Earlier counts and runtime observations describe the
builds tested at those stages.

| Area | Implemented | Validation and remaining work |
| --- | --- | --- |
| Native engine | CPAL/CoreAudio lifecycle thread, bounded command/asset queues, reclamation outside callback | Native Mac speakers smoke passes; allocation audit passes. FLX4 four-output CoreAudio startup observed; physical routing, hot unplug/wake and stress runs pending. |
| Two decks | Native clocks, play/pause/cue/seek, hot/memory cues, loops, beat jump, jog touch/scratch/bend, tempo, sync/master | Behavioral tests pass; desktop transport and loop controls observed. Hardware timing unmeasured. |
| Mixer | Smoothed trim, three-band EQ, filter, channel faders, constant-power crossfader, independent cue/master channels | DSP routing tests pass. FLX4 channel order and cue output need physical verification. |
| Pitch | Preallocated continuous Signalsmith Stretch processor, native source clock and bandlimited RAM resampling; removed whole-track WSOLA worker and raw fallbacks | Generated pitch, stereo, onset, seek, scratch and loop tests pass; callback allocation probes and bounded two-deck benchmark pass. Listening remains separate validation. |
| Analysis | BPM/rhythm, neural beat/downbeat timing, key candidates and waveform bands; no section classifier or automatic cues | Existing rhythm, transport, source guards and cache persistence regressions pass; listening remains separate validation |
| Beat model | One full final0 / RTen inference path with persistent neural timing, native variable beat events and explicit manual grids | Existing timing/cache regressions pass; removal does not bump the analysis/model version or force reanalysis |
| Cue knowledge | Manual/imported points and named spans with hot/memory/main bindings; automatic segmentation, cue placement, vocabulary and associated settings removed | Transactional migration preserves human edits and measured analysis; migration, persistence and XML regressions pass |
| Next-track suggestions | Cached compact key/tempo/meter/grid features, explicit local/liked/playlist/source/provider scopes, canonical keep/raise/lower directions; default 3% stretch and half/double disabled | Scope/cache/stretch/energy tests pass. Reads do not start analysis or backfill. Ranking remains a candidate shortlist, not a DJ strategy. |
| Timed transitions | Existing unified batch.validate/run compiles complete guarded native schedules with atomic event groups, advancement/alignment prerequisites, verified endpoints, owned recovery and durable receipts | Offline queue, idempotency, cancellation, restart and model-absence tests pass. The old frontend guide/planner is removed; the DJ chooses the musical plan. |
| Library | SQLite WAL, imports, search/sort/playlists, persisted cues/grid, XML import/export; versioned Spotify BPM/key/waveform cache, cancellable page analysis and album browsing | Persistence/cascade and XML path/cue/escaping round trips pass. Large library scale, relocation/history and proprietary export pending. |
| Controller | CoreMIDI discovery/connect, bounded packet worker, public FLX4 controls, asynchronous LEDs and history | Parser/feedback fixtures pass. Connected FLX4 enumerated through CoreMIDI/CoreAudio; actual browse/load/play/cue/mixer and rim/touch packets captured. Native UI verified through isolated CoreMIDI input. Full physical platter calibration and all-button/LED sweep remain pending. |
| Sampler/FX | Sixteen sampler slots and loadable samples; echo/reverb/filter with beat timing; independent Smart presets | Render tests include samples/platter touch. Vendor Beat/Pad/Release FX are not replicated. |
| Recording | Bounded master-output ring, off-thread float WAV, drop/error counters | WAV decode and silent CoreAudio recording pass. Long-run tests pending. |
| Spotify library | OAuth PKCE, local callback, Keychain refresh token, own Client ID, playlists/liked songs | Auth/HTTP/parser tests pass; saved-account liked songs and Connect devices verified in the release app. Playback consent is granted and native session/Premium confirmation verified; cancellation preserves existing library session. |
| Spotify playback | Remote Connect play/pause, optional official widget | Separate from decks. Live Premium playback and WKWebView limits unverified. |
| Spotify DJ deck feed | Unmodified librespot 0.8 Player → isolated custom sink → bounded RAM PCM → native decks; Load A/B, cancellation and recording capability | Separate user-approved native OAuth using upstream librespot registration fixes client-token HTTP 400. Full 2:06 and 6:17 tracks loaded; independent clocks/meters, cue return, tempo change and crossfader left/center/right verified. User confirmed audible playback; controlled checks used muted master. Headphone routing and mix quality unmeasured. |
| Provider source adapter | Integration-owned encoded Read/Seek and decoded stereo PCM inputs, generation/engine-owned load tickets, unload and native transport | WAV and decoded-PCM fixtures verify independent deck clocks, tempo, cue/seek, cancellation and recording restrictions. Whole-track preparation; no network work in render callback. |

Native smoke observed two decks advancing through CoreAudio with zero reported stream errors, then resuming after output restart. The test sets master gain to zero and verifies recorded samples remain silent. It does not measure round-trip hardware latency or prove absence of all dropouts: the visible stream-error counter counts backend-reported failures, not independently measured deadline misses.

Validation: **110 Rust tests and 38 frontend tests passed**, strict workspace Clippy passed. TypeScript/Vite build passed. Saved library and separate native-player sign-ins restore through Keychain. The developer Web API identity was the cause of client-token HTTP 400: a credential-free controlled probe rejected it and accepted upstream librespot's native identity. The authorized native adapter subsequently loaded two full tracks into RAM; no Spotify audio was exported or recorded. The optimized Tauri bundle built successfully at 17.06 MiB. Both main and overview waveforms visibly use actual blue low/orange mid/white high envelopes in the release app. Native in-place reanalysis preserved loaded source and transport positions. The initial live test exposed an overly strict tempo agreement gate: candidates were present but the BPM display was blank. The corrected support-based agreement gate keeps credible estimates visible with ambiguity instead of discarding them. Live loaded tracks subsequently displayed 93.32 and 119.99 BPM. Fractional-lag interpolation reduced synthetic mean absolute error from 0.108 to 0.025 BPM; this is not a commercial-music benchmark.

Updated silent two-deck CoreAudio/restart/recording smoke passed with zero backend stream errors and no dropped recording frames. Generated-track loop behavior and real Spotify transport operation were observed in the desktop app. FLX4 runtime checks and remaining physical validation are detailed in [the controller report](flx4-rekordbox-mapping.md).

Future acceptance gates are defined in [the roadmap](implementation-roadmap.md). Physical FLX4 routing/transport, measured latency distributions and a 60-minute deck/library/UI stress run are required before claiming comparable performance. Stems, USB/player exports, Pro DJ Link, DVS, cloud and lighting/video remain open areas.

The updated UI removes the sidebar, stacks waveforms above the decks, moves source choice to the header and browsing to a bottom sheet with a swipe handle. Cache durability across database reopen, stale/incomplete cache rejection, source identity validation and deck-priority interruption have unit coverage. Live native acceptance verified swipe-sheet resizing, cached BPM/key table updates and deck loading during page analysis. Thirty-three real track analyses were persisted and survived an app restart; remaining page work can resume by skipping those entries. The centered layout was observed in the release app. Cursor hold, release and drag seeking were subsequently verified with a real loaded Spotify track.

Analysis v3 adds periodic beat-phase origin/confidence/provenance, uses it for initial native grid alignment, and preserves manual grids. Version-2 metadata is retained; missing phase is computed when its audio is loaded. The detailed display now remains centered at song boundaries, uses linked beat zoom, visible beat/four-beat markers and cue/loop flags, and places full previews in each deck card. [Performance-mode evidence](rekordbox-performance-waveform.md) distinguishes observed vendor UI from independent implementation and remaining gaps.

Waveform updates now use separate lean transport and revision-based immutable metadata, including in the controller feedback path. A native two-deck benchmark reduced recurring snapshots from 385,640 bytes to 1,471 bytes. Renderer caching, bounded animation clocks and ordered cursor holds/absolute seeks are described in [the performance report](waveform-performance.md); live interaction validation passed in the packaged app. The headless four-graph canvas benchmark reduced median drawing time from 15.165 ms to 2.541 ms; this does not measure browser FPS.

Cursor dragging now traverses native PCM audibly in both directions, including on paused decks, with exact final seek before normal playback resumes. Detailed waveforms are 108 pixels tall, remove the gray peak overlay and use shared frequency-band display gain. Direction/sample-order, endpoint, routing independence, key-lock source and callback-allocation fixtures pass; native UI inspection passed. An original generated-tone recording made while the deck stayed paused captured forward/reverse scrub audio with peak 0.2621 and no clipping. A discovered short-gesture release-order defect was fixed with native ScrubEnd and a callback-batch regression.

The connected FLX4 pass adds source-aware controller browser/LOAD, native instant doubles, tempo ranges, loop boundary edits/reloop, all16sampler slots, beat FX selection/timing and documented LED output. Jog displacement now accumulates into absolute audible scrub targets; missed touch-on scratch packets are finite gestures, paused rim/keyboard jog auditions without starting playback, and disconnect releases held hardware jogs. Actual device capture and independent virtual-input UI acceptance are retained under`research/evidence/flx4-*`; count/RPM calibration, all physical buttons/LED states, microphone monitoring and vendor FX algorithms remain unverified or incomplete.

The final browser uses playlists on the left30% and songs on the right70%, with independent selection/scrolling. BROWSE press enters songs, then returns to playlists on the next press; Shiftpress returns directly to playlists. Default opening height is70% and reopening restores that height. Full native UI inspection verified the split and pane transitions. Paused CC21 rim jogging moved the generated demo from00:00 to00:03 while retaining paused play intent through a four-channel CoreAudio stream.

A launch check exposed synchronous CoreAudio discovery waiting inside input AudioUnit initialization even for output queries. Startup/discovery/start/stop now run off the UI thread; device metadata is cached and MIDI refresh is independent of audio discovery. A vendored CPAL0.15.3 patch reads the device buffer range directly instead of creating temporary AudioUnits during discovery ([upstream source](https://github.com/RustAudio/cpal/blob/v0.15.3/src/host/coreaudio/macos/mod.rs)); see`vendor/cpal/PULSEDECK-PATCH.md` and the retained startup stack sample. The temporary virtual MIDI endpoint used for testing is disposed before delivery.

Spotify credential persistence now uses one shared`session-v1`Keychain bundle for library/native grants. Both services restore through one cached read; permission-denied results are cached too. Existing per-client entries migrate once, retained for recovery. Stable refresh-token/scope values skip writes; rotated credentials modify the already authorized item reference without another secret read. Access tokens remain session-only and no OAuth secrets are written to settings, IPC, logs or argv. Credential-cache concurrency, unchanged/rotated persistence, legacy migration, isolated disconnect and failed-write retention have test coverage. The first migration can still require granting access to each old item once.

Final relaunched bundle observed Audio online, MIDI connected, four output channels and zero backend stream errors, with a real Spotify track loaded in deck A after the credential-vault change. The virtual source is disposed; CoreMIDI exposes only the physical DDJ-FLX4 input/output.

MacBook output restart correction: audio/MIDI choices are now independent and the selected output, buffer and routing persist in`audio.json`. Missing preferences prefer MacBook speakers with stereo routing. Startup/manual audio operations are serialized and newer explicit choices suppress late automatic startup; pending UI changes cannot be overwritten by initial hydration. Restart controls reject overlapping clicks.

CPAL disconnect listeners now use a weak reference, removing the stream/listener strong-reference cycle. Lifecycle pauses and drops the previous stream before replacement; obsolete callback/error generations are ignored. A muted native five-restart MacBook speaker regression passed with clock ratios0.975–1.013, stereo output, no clock movement after stop and zero backend stream errors ([evidence](evidence/macbook-audio-restart.json)). The FLX4 remains available for MIDI while MacBook speakers carry audio.

The relaunched release app was observed with MacBook Pro Speakers selected, 512-frame configuration, stereo master routing, separate headphones disabled and FLX4 MIDI connected. A real Spotify track produced a nonzero deck meter; native UI restart retained that loaded source and reported zero backend stream errors. The saved`audio.json`matches MacBook stereo output. macOS speaker mute was false and hardware volume was nonzero. These checks verify routing and rendered signal, not a listening-quality measurement.


Performance UI update (14 October 2026): the viewport is a bounded grid with header, stacked waves, two decks/mixer and a library footer. Only drawers and settings panes scroll. Loaded native decks fit both 1500×940 and 1100×760 windows; platters use identical width/height constraints and the channel faders and crossfader occupy separate rows. The charcoal instrument theme extends to library tables, settings, analysis/grid popovers, sampler and notices. The shared controller/keyboard navigation walks footer → playlists → tracks → playlists → close control → footer; turning while returning to playlists starts a new inward selection path. Two navigation regression tests were added (38 frontend tests pass), and the native keyboard path was observed through each focus target. No new physical MIDI injection was used. Release packaging and TypeScript/Vite build passed. Native layout screenshots are in `evidence/dark-layout-minimum.png` and `evidence/dark-settings.png`.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

## Local integration of PR #1 (14 October 2026)

The local `codex/field-instrument-stack` branch combines incoming head `3d9a279`, current main `0288f44` and the owner's checkpoint `7451f06`. The Field Instrument layout, Aluminium/Ivory/Graphite finishes, fonts and incoming panels are accepted. CLI controls, song tags, Spotify fixes, manual grids and quantized launch are retained. No branch or PR was pushed and main was not merged.

Review found integration blockers from the incoming duplicate inference path and fixed four-beat/constant-tempo section assumptions. The integration derives sections and transition timing from native events; the guide declines mismatched meters. Spotify auto-cue loading now respects persisted cue state, and live generated cue updates skip user-edited slots or replaced sources. Analysis settings use the isolated data directory. The Spotify compatibility switch enables section analysis while the existing core detector continues to run.

Validation: 210 Rust tests and 74 frontend tests pass; three explicit model/pipeline smoke tests pass; workspace Clippy with warnings denied, formatting, TypeScript/Vite build, control coverage and DJ skill packaging checks pass. The browser preview rendered at 1500×940 without viewport overflow or console warnings/errors. The test command enables TypeScript stripping explicitly for the owner's Node 22.14 runtime. The CLI catalog exposes 89 commands, including new settings, vocabulary, section-update and suggestion operations. New audio hardware listening, FLX4 interaction, practice-set review and real-track accuracy evaluation were not performed during this integration. Prior hardware observations above refer to their recorded earlier builds.

CLI and skills now use source-owned named cues, compact scoped suggestions and the existing scheduled batch interface. The frontend and CLI share one control dispatcher; only its transport/UI acknowledgment handlers remain registered. Session skills prepare music early, prefer nearby tempos, corroborate phrasing and resume from actual persisted state after compaction.

Follow-up validation: 213 Rust tests and 74 frontend tests pass; formatting, strict Clippy, TypeScript/Vite build, all 91 command coverage and seven-skill packaging checks pass. The debug desktop bundle was rebuilt and launched. Read-only checks against the running app returned native-bar sections with matching grids, ranked suggestions, analysis settings and a current advisory guide. Playback, settings and library data were not changed by those checks; hardware listening and controller validation remain separate.

Deck loading feedback now replaces the Spotify drawer's preparation banners. UI loads close the drawer and immediately show the selected title, loading overview/scrolling waveforms and a deck placeholder; Spotify cancellation is available on that deck. Native readiness refresh restores the new track and controls, while failed/cancelled preparation restores the previous deck. Per-deck request guards prevent a superseded completion from clearing a newer loading indicator. Local/test-tone loads use the same feedback, and external Spotify loads are reflected by native source-job activity. Build and 74 frontend tests pass; a simulated delayed native session verified replacement, failure recovery, cancellation and no viewport overflow. The updated debug bundle was relaunched before the separately requested liked-songs session began.

Approved session-feedback changes: source fingerprints and independent stage stamps retain successful analysis and avoid repeated model/spectral work on cached loads and label edits. Canonical point identity, binding and tombstone provenance prevent a grid/meter change from transferring a manual edit to another region. Live transport now bypasses application-wide React reconciliation; cold waveform tiles use a worker where supported, with an idle fallback, bounded cache and source-generation cleanup. Output applies fixed 6 dB headroom and stereo-linked lookahead sample protection; diagnostics report gain reduction and protected meters. Protection cannot repair already clipped source material, and this implementation does not assert a true-peak guarantee.

## Completed local session-feedback scope (14 October 2026)

The CLI, GUI and MIDI use the existing unified control dispatcher. Only
`control_request`, `ui_publish` and `ui_reply` remain registered native transport
handlers. Existing `batch.validate` / `batch.run` now accept guarded native
schedules: event groups, source and clock revisions, playback advancement,
deadlines, native ramps, completion endpoints and predeclared recovery. Durable
operation receipts support retry and compaction recovery without replaying a
launch. Renderer stop, source replacement and manual takeover invalidate pending
work. The frontend transition guide, backend mix planner, whole-track WSOLA
worker and obsolete native wrappers are removed.

Named points and regions share one `CueCollection`; sections, hot pads, memory
slots and the main cue use its identities and bindings. Reanalysis preserves
manual edits and source-time provenance. Existing cue operations and the XML
writer preserve names. Suggestions read compact cached features, honor explicit
playlist/liked/provider/source scopes, default to a 3% tempo window and leave
half/double interpretation opt-in. The three consolidated DJ skills separate
library knowledge, musical judgment and control workflows. Performance guidance
prepares early, corroborates phrasing, prefers suitable nearby tempos, queues
complete handovers and resumes from recorded state rather than replaying actions.

Continuous preallocated Signalsmith processing and bandlimited resampling replace
the whole-track stretch preparation path. Source clocks, seek pre-roll and
scratch release remain native. Output protection uses stereo-linked lookahead on
master and headphones with a shared 128-frame latency. Successful analysis
stages and terminal failure outcomes are cached independently; ordinary cached
reads do not start inference. Metadata and waveform construction are outside the
live transport path. Deck/waveform loading feedback remains source guarded.

Final validation on the combined local tree:

| Check | Result |
| --- | --- |
| Serialized `tools/test-all.sh` | 273 Rust tests and 65 frontend tests pass; strict Clippy, formatting, frontend production build, 90-method control coverage and seven-skill validation pass. |
| Explicit full-model checks | Two full Beat This! inference smoke tests and one decode → native rhythm → sections smoke pass. |
| Real desktop CLI/UI trials | 17 checks plus two restart checks pass, using generated local sources and muted CoreAudio. |
| Scheduled handover trial | Seven checks pass, including a complete native handover during eight seconds without CLI calls, recovery, manual takeover, retry/resume receipts, named cue XML and scoped suggestions. |
| Native processor allocation probe | Zero allocations or deallocations over 2,000 processing calls and 21 seeks. |
| Two-deck callback benchmark | 6,000 callbacks of 128 frames: p99.9 0.109 ms, maximum 1.521 ms against a 2.667 ms budget; zero reported software deadline overruns. |
| Foreground native waveform check | Ten seconds of muted two-deck playback: roughly 60 paints/second per waveform, median frame interval 17 ms, p95 paint ≤1 ms; zero app, structural or deck-control updates, and zero additional metadata fetches during the measured interval. |
| Desktop bundle | Rebuilt and relaunched with both decks stopped. Isolated test instances were stopped. |

Local logs and measurement data are retained under `artifacts/implementation/`;
the scheduled trial is under
`artifacts/skill-trials/scheduled-handover-20261014/`. These ignored artifacts are
available in this workspace, not included in the public repository. Generated
muted trials and software benchmarks do not establish perceptual mix quality,
real-song phrase accuracy, physical headphone routing or controller latency.

The changes remain uncommitted on `codex/field-instrument-stack`, above the
existing checkpoint/integration history. Main remains `0288f44`; no push or main
merge was performed. The updated app is open without an automated DJ session.


## Persistent command sessions verified locally (15 October 2026)

`pulsedeckctl session start --stdio` now keeps one mutable command job open and streams registration, queued/running state and verified completion while accepting more input. It extends the existing batch/jobs/events interface and renderer queue. Provider work uses unified execute; pending command ownership is scoped to its actual decks and timing clocks. Relative timing and completed-item dependencies replace model-side invocation timing. Logical updates, removal, retries, exact source-request cancellation and bounded compact status share the existing control path. Legacy raw sessions and guarded batches remain compatible.

A fresh subagent tested public help/catalog/docs without implementation context. Its findings drove fixes for unrelated source-load blocking, false running labels, missing/duplicate terminal receipts, excessive feedback and headphone-cue takeover. Final workspace verification passes 332 Rust tests, 65 frontend tests, strict Clippy, formatting, production frontend build, 93-method unified coverage and seven-skill validation. A real muted generated-audio fixture passes five compound session checks, including queued load/tempo/seek/play/ramp/pause, beat-relative completion, pending edits and more than 64 actions. Final independent replay passes nine assertions across five additional sessions. The isolated fixture is stopped; Luna remains stopped. Main remains unchanged and no changes were pushed.

Local evidence: `artifacts/persistent-session/REPORT.md` and `artifacts/persistent-session-blackbox/final/REPORT.md`. These tests verify software control/timing and do not establish Spotify latency or musical/listening quality.

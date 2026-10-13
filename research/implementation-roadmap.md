# Rust and Tauri implementation roadmap

The first PulseDeck implementation now exists. See [current implementation and validation status](implementation-status.md) for delivered features and remaining acceptance gates; the roadmap below is the full target, not a completion claim.

Scope selected by the user: full rekordbox roadmap, with local two-deck DDJ-FLX4 performance first. Reference build: 7.2.19.0342. The app inspection establishes a C++/JUCE native graph, CoreAudio/CoreMIDI, dedicated preparation/cleanup workers and a separate cloud helper. The implementation below is independent Rust/Tauri work, not a conversion of recovered source.

## First release acceptance boundary

Two decks must load local files, play/pause/cue/seek, scratch and bend from jog input, change tempo, loop, sync and trigger hot cues. The mixer must provide trim, three-band EQ, filters, channel/crossfaders and independent headphone/master buses. Browsing/loading and controller feedback must work without interrupting audio. Device loss, rapid transport changes, sleep/wake and background library jobs must have explicit behavior.

Performance parity means comparing both applications on the same Mac, FLX4, buffer/rate, files and workload. It is an acceptance criterion, not an implementation assumption. Current FLX4 enumeration is blocked; hardware tests become executable when macOS exposes the controller. The Rust toolchain is also not currently present on this host; no compiled Rust/Tauri application is delivered in this pass.

## Ordered work packages

| ID | Priority and dependency | Concrete work | Completion evidence |
|---|---|---|---|
| DJ-01 | First; no app dependency | Build/version fixture, independent test audio, OS device enumeration and read-only MIDI capture tooling. | Hashed input fixtures, visible FLX4 streams/endpoints, measured reference device configuration. |
| DJ-02 | First; DJ-01 for hardware gates | Rust engine crate independent of UI; sample-frame clock, bounded event queues, explicit asset ownership/reclamation, preallocated output. | Deterministic offline rendering and allocation/deadline instrumentation. |
| DJ-03 | First; DJ-02 | CoreAudio adapter, format/buffer negotiation, timestamps, independent buses, device lifecycle. Validate CPAL vs direct CoreAudio. | Physical master/headphone channel test, hardware disconnect/reconnect tests and rendered silence without glitches. |
| DJ-04 | First; DJ-02 | Format decode workers, seek index, paged PCM/cache, cancellation and load/unload state machine. | WAV/AIFF/MP3/FLAC/AAC/ALAC fixtures as supported by chosen decoder; exact seek/priming and rapid-load tests. |
| DJ-05 | First; DJ-02/04 | Two deck transport; cue points, loop edges, hot cues, sample-rate conversion, interpolation and jog trajectories. | Impulse/transient tests, click-free loop boundaries, scratch direction/velocity and cue recovery. |
| DJ-06 | First; DJ-01/05 | Independently authored FLX4 profile from public protocol plus verified captures; high-resolution controls, shift/modes, input timestamps and LED scheduling. | All first-release controls and feedback validated against hardware. Resolve tempo/depth discrepancies before release. |
| DJ-07 | First; DJ-03/05 | Mixer, smoothing, EQ/filter, crossfader curves, cue mix and recording queue. | Separation/no-bleed capture, parameter sweeps, levels/headroom and offline vs physical output comparison. |
| DJ-08 | First; DJ-02/05/07 | Tempo/pitch policy, key lock, beat clock, sync/master, quantize and loop phase. Select algorithms through listening/measurement. | Drift, transient quality, tempo/key changes and scripted reference comparisons. |
| DJ-09 | First; DJ-02/04/05 | Small Tauri shell: device settings, load/deck state, waveform cache, bounded meters and command acknowledgments. | Audio continues during slow UI/library operations; measured frame/input latency and bounded IPC payloads. |
| DJ-10 | First-release gate; DJ-01–09 | Soak/stress and hardware regression suite. | Reference latency distributions, zero observed dropouts in initial 60-minute workload, deadline/max-tail reports, no callback allocations and device lifecycle results. |
| DJ-11 | Next; DJ-04/09 | SQLite local library, metadata/search/playlists/history, relocation, waveform pyramids, cue/grid revisions and XML migration. | Large-library query tests, crash-safe persistence, documented XML round trips and revision restoration. |
| DJ-12 | Next; DJ-11 | BPM/beat/key analysis, confidence values, manual beat-grid editing; auto cues/embeddings as distinct optional stages. | Versioned analysis cache, known annotated fixtures, accuracy/error reports and cancellable workload budgets. |
| DJ-13 | Next; DJ-05/07/08 | Expanded FLX4 workflows: all eight pad modes, sampler, stored loops/cues, instant doubles, shifted controls and Beat/Pad/Release FX. | Hardware diagram action matrix plus audio behavior fixtures. |
| DJ-14 | Next; DJ-06/08/13 | Smart Fader/CFX host orchestration, independent presets/DSP, sync restoration, fader assignment and conflicts. | Captured enable/disable transitions and audio comparisons. Native direct-call evidence guides orchestration; DSP curves remain to be verified. |
| DJ-15 | Later; DJ-11/12 | Offline-first stem service, independent models, caching, stem routing/FX and CPU/GPU budgets. | Separation quality, cancel/recovery, memory limits and no disruption to playback under analysis. |
| DJ-16 | Later; DJ-11/12 | USB device-library export, player compatibility, analysis-file interchange and export recovery. | Exact supported-player/version fixtures and physical-player verification. XML import alone is insufficient. |
| DJ-17 | Later; engine/library stable | Pro DJ Link/link networking, external mixer/device routing, additional controllers, DVS and recording variants. | Each protocol/device gets a separate versioned compatibility and timing matrix. |
| DJ-18 | Later; DJ-11/12 | Partner streaming integrations and track authorization/cache semantics. | Official service access, metadata/seek behavior, expiry/offline rules and workload tests; static vendor code is not service authorization. |
| DJ-19 | Later; DJ-11 | Cloud sync, sharing, queued file transfer and notification service replacing the helper's role. | Conflict/resume/retry state machine, account/API integration and verified notification contracts. |
| DJ-20 | Later; core stable | EDIT/video/lighting and additional modes; distribution, signing, notarization and update recovery. | Feature-specific fixtures and supported hardware, signed releases and update rollback tests. |

Each package must specify supported formats/devices and error behavior before claiming completion. A nominally implemented feature with no equivalent reference scenario remains partially covered. The final product matrix must use the installed version's manuals and enabled modes rather than treating this grouped roadmap as an exhaustive vendor checklist.

## First engine contracts

Commands carry a deck/bus identity, sequence number and host timestamp. The engine acknowledges application at a sample frame and publishes immutable deck/meter snapshots. UI may interpolate presentation but never owns transport time. MIDI input and UI commands are separate producers, so use per-producer bounded queues or a proper bounded multiple-producer design.

Deck state separates unloaded/loading/ready/playing/paused/error. Loading can be canceled; prepared assets transition at callback boundaries. Destruction occurs on a reclamation worker. Seeking invalidates queued audio using a generation ID so late decode results cannot resurrect an old track. A missing PCM region has a declared underrun policy and diagnostic counter.

Output routing is an explicit assignment of logical buses to discovered device channels. Current output channel labels and USB ordering must be verified physically. Mirroring to another device introduces a second clock and requires independent drift handling. Controller inputs, feedback and undocumented settings are separate layers; do not send unverified settings bytes at startup.

Expensive analysis and stems run under a bounded job budget. They provide prepared caches/assets, never execute inference in the render callback. Heavy UI rendering and background jobs must feature in the first hardware soak workload.

## Outstanding inputs

The installed app is resolved. The current hardware discovery returns no FLX4 USB child, audio stream or MIDI endpoint despite the user's confirmed connection. A successful OS enumeration and versioned reference playback session are required for DJ-01 and DJ-10. Fine DSP algorithms, private database/export compatibility, compiled helper internals and partner integrations remain investigation requirements, not completed implementation.

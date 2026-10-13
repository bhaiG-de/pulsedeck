# Rekordbox investigation coverage

Recorded 13 October 2026, Asia/Kolkata. The target is the user's installed macOS rekordbox application and DDJ-FLX4 controller. No other application has been substituted.

| Question | Status | Evidence or next step |
|---|---|---|
| Which installed rekordbox build is being recreated? | Answered | REA plist identifies 7.2.19.0342 at `/Applications/rekordbox 7/rekordbox.app`; captured main SHA-256 unchanged during collection. |
| What language, UI framework and helper processes does it use? | Answered for main and cloud helper | C++/JUCE 8.0.8 main; embedded Electron 19.0.10 helper with a small React notification UI. Other helper roles remain partly inferred. |
| Which libraries implement decode, stretching, analysis and effects? | Partial | mpg123, TensorFlow/ONNX imports and models, first-party stretch symbols/workers, live JUCE mixer/FX graph. Exact algorithms and per-feature reachability remain unresolved. |
| How is its library stored and how can tracks/cues be migrated? | Partial | SQLCipher/SQLite imports and helper ORM observed; published XML interchange exists. Exact private schema and binary analysis/export formats remain uninspected. |
| Does FLX4 need a vendor macOS audio driver? | Answered from official docs | No. See `flx4-official-findings.md` and official driver FAQ. |
| Are controller control and LED messages available? | Partial | Official MIDI list plus 201-row installed FLX4 profile and settings candidates. Two discrepancies require hardware verification. |
| What are actual audio channels, routing and startup behavior? | Unresolved | After confirmed connection, CoreAudio/CoreMIDI/IOUSB probes still do not enumerate FLX4; physical routing tests pending. |
| How do Smart Fader and Smart CFX work? | Partial | Documented behavior plus exact compiled host calls to sync, BPM/tempo, fader-curve and Touch FX restrictions. Exact DSP, parameter curves and firmware/host split unresolved. |
| Can equivalent latency and sound quality be demonstrated? | Unresolved | No reference playback or hardware measurements performed. Benchmark protocol in architecture plan. |
| What Rust/Tauri architecture should we build? | Proposed design grounded in observations | `architecture-and-parity.md` and `implementation-roadmap.md`; native engine with independent UI/workers, prioritized two-deck hardware performance. |

## Evidence identities

`HOST-001` refers to the shell-produced discovery record in `evidence/host-discovery.json`, not REA Evidence. Search roots: `/Applications`, user `Applications`, `Downloads`, `Desktop`, `/Library/Audio/Plug-Ins/HAL`, `/Library/Extensions`; depth bounded at four; application bundles are not recursively searched for another application; large source/dependency directories excluded. Spotlight also returned no rekordbox match. These searches do not establish absence from the entire disk or another Mac.

`REA-READINESS-001` refers to `evidence/rea-doctor.json`, produced by `rea-agents@6.4.0 doctor --client codex --json` using existing Node 24.21.0. It is a readiness result, not target analysis: Codex REA registration is missing/drifted, and no native provider is configured. The skill is installed; the missing identity marker reported by doctor does not negate that installation. No registration or unrelated software was changed.

`DOC-FLX4-001` denotes the official manual, MIDI list and rekordbox hardware diagram retained under `flx4-reference`; source URLs and document pages appear in `flx4-official-findings.md`. These are documentation evidence, not captured hardware traffic or REA-generated target findings.

The initial `HOST-001` record predates installation and is retained as historical evidence. Current REA identities and artifact coverage are listed in `installed-rekordbox-architecture.md`. Its `evidence/rekordbox-7-installed` outputs supersede the initial missing-app observation.

Local `nm` symbol declarations, bounded `llvm-objdump` disassembly, one-second live process sampling, resource reads and native device property probes are labeled shell observations, not REA Evidence. The user-owned rekordbox process was already running; this investigation did not launch it or mutate its settings.

## Next evidence pass

1. App path/version and main identity resolved; preserve this reference build for comparisons.
2. Static REA bundle/plist/Mach-O/signature/dyld collection and Apple graph projection completed. Retained outputs contain explicit coverage limits.
3. Main executable slices, major dependencies, nested helper, controller resources and model assets identified. Continue only specific unresolved feature questions.
4. Trace individual questions: deck render path, audio device setup, MIDI dispatch, waveform/analysis jobs, library interchange. Reuse retained evidence and a matching analysis database. Function analysis requires an available native provider.
5. Resolve FLX4 OS recognition. The user confirmed the data/computer port, but the current native probe lists no endpoint. Then test audio routing and MIDI input/output against the official table and shipped-profile discrepancies.
6. Run bounded user-visible reference scenarios and audio measurements, then replace proposed requirements with build-specific findings.

No target function was decompiled by a native provider. Five exact ARM64 Smart-feature functions were disassembled with built-in LLVM tooling, and an existing native process was sampled. No rekordbox process was launched, no controller MIDI was sent and no performance parity was claimed.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

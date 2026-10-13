# PulseDeck

A working macOS DJ application with a native Rust audio engine and Tauri/React interface, following a REA investigation of installed rekordbox 7.2.19.0342. Audio rendering and CoreMIDI control run outside the webview.

The local-file performance build includes two decks, cue preview, eight hot cues, ten memory cues, loops, beat jump, jog/scratch, tempo, sync/master, key lock and pitch shift, EQ/filter, channel/crossfaders, master/headphone routing, echo/reverb, sixteen sample slots and WAV recording. The SQLite library supports imports, search, playlists and rekordbox XML migration.

## Run

The packaged application is produced at `target/release/bundle/macos/PulseDeck.app`. Open it in Finder. Import local music or use **Load → Load test tone** to try the decks. Keyboard Space controls deck A; Enter controls deck B when focus is outside editable controls.

Choose audio device, requested buffer size and output routing in Settings. On a four-channel device, select separate master and headphone channels. FLX4 CoreAudio/CoreMIDI discovery and connection are exposed in Settings; no vendor macOS driver is installed. Physical FLX4 channel order and control behavior still need verification after macOS recognizes the connected controller.

## Spotify

Settings → Spotify accepts your own public Spotify developer Client ID. Register `http://127.0.0.1:8765/callback` in the developer dashboard, add your account to the development allowlist, and sign in through the browser. The implementation uses PKCE, a local callback and macOS Keychain refresh-token storage. It loads playlists and liked songs and controls an available Spotify Connect device. The optional official Spotify widget has browser-dependent playback/preview limits.

**Native Spotify playback is working through the experimental librespot adapter.** Settings → Spotify → **Sign in to native player** opens a separate streaming-only consent using librespot's upstream native registration; library/Connect credentials stay separate. A controlled request comparison proved that using the developer Web API identity caused HTTP 400 while the native identity succeeds. After native consent, full tracks of 2:06 and 6:17 loaded into the Rust decks, with advancing clocks/meters, cue return, tempo changes and crossfader left/center/right checked in the desktop app. The user confirmed audible playback; controlled transport tests used muted master output. Physical headphone routing and mixing quality remain unmeasured.

Load A/B prepares full-track stereo PCM in RAM before native transport begins. Preparation is capped at256 MiB /about12m40s pertrack and120s; recording is disabled while these sources are loaded. No saved audio/export, vendor credential extraction or custom protected transport is implemented. See [adapter details](crates/spotify-playback/README.md) and [authentication diagnosis](research/spotify-authentication.md).


Native load failures report credential-safe metadata, delivery, format, decoder or playback-key stages. Metadata/delivery failures and playback-key timeouts get one delayed retry within the preparation deadline, using the same authorized session and only before any PCM has arrived. Old drawer errors clear when a new deck preparation starts.

## Develop and validate

Rust, Node/npm and the macOS Command Line Tools are required. Install Rust with rustup and use your standard Cargo installation. If a local toolchain exists under `.toolchain`, the wrapper uses it without changing your shell profile; local toolchains are not part of the repository.

```sh
npm install
npm --prefix frontend install
bash tools/rust-env.sh npm run desktop
bash tools/rust-env.sh npm run package
bash tools/rust-env.sh cargo test --workspace
bash tools/rust-env.sh cargo clippy --workspace --all-targets -- -D warnings
npm --prefix frontend test
bash tools/rust-env.sh cargo run -p deck-engine --example native_smoke
```

The native smoke test runs both decks silently through CoreAudio, records/decodes a WAV, and verifies transport continuity across audio stop/restart. Unit tests cover transport and routing, render-time allocations, track-load cancellation, pitch preparation, key estimation, FLX4 message parsing, XML/persistence and Spotify auth handling. These tests establish current behavior; they do not establish rekordbox performance or sound-quality parity.

## Current limits and roadmap

Tracks are decoded into RAM. Key lock/pitch preparation computes a whole-track WSOLA asset on a worker, with cancellation and a 512 MiB preparation budget. Changes can have preparation latency and briefly use varispeed; Linear resampling and simple DSP need further fidelity work. Analysis v3 uses multi-window spectral rhythm analysis, tuning-aware chroma/key candidates and 4096 aligned peak/low/mid/high waveform bins. Confidence scores expose uncertainty; annotated real-music accuracy benchmarks remain necessary. Smart Fader/CFX are independent presets. Hardware latency, sleep/wake/device loss, long stress runs and a large-library benchmark remain pending.

Stems, proprietary USB/player export, Pro DJ Link, DVS, cloud sync, advanced video/lighting and complete Beat/Pad/Release FX parity are future roadmap work. This build is a functioning first implementation, not a completed replacement for every rekordbox feature.

- [Implementation status and validation](research/implementation-status.md)
- [Full ordered roadmap](research/implementation-roadmap.md)
- [Installed rekordbox architecture](research/installed-rekordbox-architecture.md)
- [Architecture and parity plan](research/architecture-and-parity.md)
- [Official FLX4 findings](research/flx4-official-findings.md)
- [Controller resource findings](research/rekordbox-controller-resources.md)
- [Helper findings](research/rekordbox-agent-findings.md)
- [REA investigation coverage](research/investigation-status.md)

Research evidence stays separate from the original app implementation. The read-only hardware diagnostic can be run with `xcrun swift tools/probe_macos_audio_midi.swift`.

## Deck workspace and analysis cache

The header selects local or Spotify music. Two real three-band waveforms are stacked above the decks and center mixer; the sidebar is removed. **Open Spotify** at the bottom opens a compact bottom sheet with a draggable handle, snap heights, scrollable tracks and a Close footer. Track rows support Load A/B and drag onto a deck; dragging shrinks the sheet to expose the deck targets.

Spotify song tables show cached BPM and Camelot key. **Analyze page** queues up to 50 tracks from the latest loaded page, skips current cached entries, reports progress/failures and supports cancellation. Album names open their tracks; **Analyze albums / playlists on selection** enables first-page background analysis when a collection is selected. Uncached deck loads automatically analyze and persist results. Manual BPM corrections are also cached. Results and 4096 band/peak envelopes live in a separate versioned SQLite table; provider audio remains bounded RAM for authorized playback and is not written to this cache. Pending deck loads interrupt the current batch source preparation and take priority; the page resumes afterward.

The larger detailed waveforms show solid low/mid/high envelopes without a separate peak outline and keep their playheads fixed at center, including file boundaries: a new song starts in the right half. Zoom is linked in beats, markers use native BPM/grid offset, and cue/loop flags remain visible. Each deck card contains its full-track preview. An estimated periodic grid origin supports alignment; the stronger four-beat markers do not prove the actual musical downbeat. The grid editor supports manual alignment and retains it in the cache.

Waveform playback uses a display-frame clock bounded by native transport updates. Immutable waveform metadata is fetched separately from lean transport/meters, so moving the playhead does not resend or rebuild waveform arrays. Hold the detailed waveform with the mouse to stop that deck temporarily; drag horizontally to hear forward/reverse PCM scrubbing, then release at the final position to resume its previous playing state. Scrubbing is audible while paused too, through the normal mixer/cue routing. Full previews remain click-to-seek.

Spotify credentials restore once per app session from one shared Keychain bundle; playlist/deck operations use memory. Unchanged refresh tokens do not trigger Keychain writes, and rotated tokens update the existing authorized item without rereading its secret. The first upgrade migrates the two older credential entries and may ask for their access once.

Audio output is independent of MIDI: MacBook speakers are the initial output and output/buffer/routing choices persist across relaunches. Connecting the FLX4 does not change the selected audio output.


## Interface and controller browsing

The performance view fills the window with a responsive grid: header, stacked waveforms, decks/mixer and library footer. Settings → Appearance offers Hardware Dark, Anatomy Light and Anatomy Dark. Shared semantic tokens style the decks, mixer, library tables, drawers, settings, analysis popovers and notifications. Themes apply immediately, persist locally across restarts and preserve the three-band waveform colors. Anatomy also uses its own engraved SVG dials and platters, thin card outlines, shallow button faces and neutral readouts rather than the Hardware theme’s metallic controls. Anatomy palettes are adapted from [Anatomy](https://skills.wheresryan.sh/anatomy). Only library and settings content scroll.

Turn the browse knob on the main view to focus the library footer. Press to open playlists, press again to enter songs, then press to return through playlists and the close control. Press the close control to return to the footer. Turning on playlists starts a fresh inward selection path. Shift/Back moves outward one level.

## Local configuration and secrets

No environment credentials are required for normal operation. Configure the public Spotify Client ID in the app; OAuth grants remain in macOS Keychain. Access tokens stay in memory. The app does not load credentials from `.env`.

A private `.env`, if needed for local development tooling, must be ignored by Git and have owner-only permissions (`chmod 600 .env`). `.env.example` contains comments only. Local application databases/settings, credentials, audio files, toolchains, build output and inspection captures are excluded from version control. Retained research evidence paths refer to private local captures, which are intentionally absent from this public repository.

## License

Original PulseDeck code is MIT licensed. Vendored CPAL retains its Apache-2.0 license; see [third-party notices](NOTICE.md).

# PulseDeck

A native, two-deck DJ app for macOS, built with Rust and Tauri. Browse local music and Spotify collections, inspect beatgrids and waveforms, and use **Suggest** to plan a next move or hand a continuous set to the app-owned successor queue.

[Download for macOS](https://github.com/bhaiG-de/pulsedeck/releases/latest) · [Release history](https://github.com/bhaiG-de/pulsedeck/releases) · [DJ skills](.agents/skills/README.md)

## Install

1. Download the Apple Silicon (`aarch64`) `.dmg` from **GitHub Releases → Assets**.
2. Open it and launch **Install PulseDeck** for the animated installer; drag its PulseDeck icon into the Applications target or use Install. You can also drag **PulseDeck.app** directly into the DMG’s Applications shortcut.
3. Launch PulseDeck. Import a local folder or configure Spotify in Settings.

Requires an Apple Silicon Mac and macOS 12 or newer. Release downloads include `SHA256SUMS.txt`; verify with `shasum -a 256 -c SHA256SUMS.txt` beside the downloaded DMG.

The current build is ad-hoc signed and **not Apple-notarized**. macOS may require first-launch approval in System Settings → Privacy & Security. User music, library databases and account credentials are not included in downloads.

The native installer uses the app’s aluminium chassis, Hanken Grotesk/DM Mono type and exact header mark, with a live drag-position indicator, reactive install target, Reduced Motion support and transactional app replacement. It confirms replacement, requires the installed app to be quit, and preserves the previous app if source copying fails. It does not modify the library or account data.

The installer can install/update **pulsedeck-control**, **pulsedeck-library** and **djing-art** into `${CODEX_HOME:-~/.codex}/skills`. Existing DJ skills (including legacy names) are backed up outside skill discovery in `pulsedeck-skill-backups` before replacement; restart Codex to discover the installed skills. Uncheck the option for app-only installation. Dragging the app directly in Finder installs only the app.

The signed app includes the CLI at `/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl` and self-contained skill packages in `Contents/Resources/dj-skills`; a source checkout is not required for agent control.

The bundled app icon follows the header's dot-matrix record mark: graphite dots with an orange center on a full-bleed silver surface. Its Imagegen master and prompt are in [src-tauri/icons](src-tauri/icons); regenerate bundle sizes with `python3 tools/make_icon.py` after installing npm dependencies.

## What the app does

### Decks and mixer

- Two native audio decks with stacked, three-band waveforms, overview seeking, jog/scratch, tempo control, key lock and pitch shifting.
- Beat-aware launches and Sync, editable beatgrid origin/BPM, hot cues, memory cues, loops and beat jumps.
- Channel trim, three-band EQ, filters, faders, crossfader and master/headphone routing. Physical cue monitoring needs a separate audio output.
- Stereo-linked output headroom and limiting, with gain-reduction and callback diagnostics.
- Echo and reverb on independent main/cue send-return histories. Returns survive a closed fader, paused deck or replaced track, with bounded tails.
- WAV recording for eligible local playback; streaming-service playback and its residual effect tails block recording.
- Native DDJ-FLX4 MIDI/control integration and explicit audio/controller setup.

### Library and analysis

- Local folder import, searchable library, playlists, tags, metadata tools and rekordbox XML exchange.
- Versioned analysis/cache with locally bundled Beat This! `final0` beat/downbeat inference, key estimates and waveform features. Analysis runs on workers rather than in the audio callback.
- Beat events retain their detected timing, including tempo variation. User/imported cues remain separate; PulseDeck does not invent verified phrases, vocal labels or automatic arrangement sections.
- Full-PCM audible start/end frame measurements at −60 dBFS, independent of musical cues. The planner uses these boundaries to reserve useful transition runway.
- Integrated programme loudness and true peak measured with `ebur128`. Per-deck **Level** normalization targets −18 LUFS, with bounded boost and measured source peak headroom, independently of manual trim. It can be bypassed.

Programme normalization does not make every breakdown/drop equally loud. Overlaps, EQ, effects and stretching can change final peaks; output protection remains active. Estimates and timing receipts do not establish listening quality or detector accuracy parity with commercial DJ software.

### Spotify

Settings has separate sign-ins for **library/Connect access** and **native player playback**. Library login alone does not make songs loadable into decks. The native player uses an experimental librespot adapter; source availability and provider behavior can change.

Liked Songs and playlists can be fully indexed across pagination. Indexing stores membership/metadata; audio analysis is separate. Collection suggestions use the indexed analysed repertoire, not only the newest provider page. Explicit queued sources can be analysed during isolated preparation, subject to the carrier's runway.

Native Spotify preparation keeps bounded full-track PCM in RAM for playback. Provider audio is not saved/exported as a library cache. Credentials use macOS Keychain and app-private configuration, never repository files.

## Suggest and continuous mixing

Open **Suggest** in the top navigation. It shows feasible successors and transitions calculated from the current decks, source/grid/control identities, tempo limits, harmonic/energy evidence and remaining audible material.

Supported native recipes:

| Technique | Shape |
|---|---|
| EQ blend | Staged layer/EQ transfer over a longer overlap |
| Filter blend | Filter and fader transfer |
| Short exchange | Four-bar controlled overlap |
| Echo out | Eight-bar transfer with an independent echo return |
| Reverb exit | Eight-bar transfer with an independent reverb return |

Phrase and vocal evidence can be uncertain; the panel exposes warnings and review requirements. Additional techniques such as stems, vocal handoffs and tempo bridges remain unavailable where their evidence/execution guards are missing.

### Successor queue

Start a suitable playing opener, then expand **Successor queue** in Suggest. Choose an ordered list or a collection, energy direction, close tempo limits, allowed techniques and an optional duration. Review uncertainty/replacement permissions and start it once.

The app owns successive loading, preparation, timing, clearance and recovery. It prepares the next source when a deck is released, bounds consecutive source skips and requests attention if no safe continuation is available. Pending ordered tracks can be reordered or removed using revision-checked edits.

- **Queue accepted:** the app continues after an agent or CLI exits. A model does not need to monitor progress.
- **Actionable outcome:** `mix.queue.notice` reports completion or attention; the UI shows a persistent notice and the final summary is retained.
- **Stop:** cancels owned queue work and its endpoint; ordinary playback continues.
- **Duration:** starts at queue acceptance, includes preparation/overlaps and stops playback at its deadline.
- **Manual takeover:** yields ownership. The queue does not automatically override the user's controls or restart.
- **Restart:** restores an interrupted summary without replaying playback.

Unavailable audio or an unusable grid can prevent continuation. The app preserves a healthy carrier when it can; it cannot supply missing music indefinitely.

## Agents and CLI

Three skills separate [musical judgment](.agents/skills/djing-art/SKILL.md), [performance control](.agents/skills/pulsedeck-control/SKILL.md) and [library knowledge](.agents/skills/pulsedeck-library/SKILL.md). Their [continuous-set workflow](.agents/skills/pulsedeck-control/references/continuous-sets.md) starts one detached queue and ends the agent turn after acceptance. Healthy queues need no `--follow`, periodic reads or model-side timing loop.

Build the CLI with `bash tools/rust-env.sh cargo build -p pulsedeck-cli`. Examples use `pd` as shorthand for `target/debug/pulsedeckctl`; the desktop app must be running.

```sh
# Bounded analysed opener shortlist; no repeated Spotify page fetches.
pd call mix.openers --args '{"playlist":"spotify:liked","provider":"spotify","min_bpm":118,"max_bpm":126,"limit":8}' --json

# After loading/playing the chosen opener, submit the continuous set once.
pd mix queue start --playlist spotify:liked --provider spotify --energy-direction keep --max-stretch-percent 2 --duration-seconds 3600 --review-confirmed --request-id my-set-1 --json

# Explicit status/stop when requested or an actionable outcome needs attention.
pd mix queue status --queue-id QUEUE_ID --json
pd mix queue stop --queue-id QUEUE_ID --json

# Cheap discovery; request one full schema only when needed.
pd commands list --prefix mix. --json
pd commands describe mix.queue.start --json
```

`mix.options` defaults to a compact shortlist with evidence/warnings; `detail:true` exposes full passage/guard data. `commands list` returns concise names/descriptions; `--detail` opts into the full catalog. Request IDs make uncertain retries idempotent. Read-only field selection, event cursors and source/control revision guards are shared with the UI.

Native events are available to background integrations. An event subscription alone does not automatically resume a Codex chat; do not simulate that integration by repeatedly waking a model to read an idle CLI. Interactive `--follow` remains available for people or host-owned background processes.

## Develop

Prerequisites: macOS with Xcode command-line tools, Node.js 22.6 or newer, Python 3 and a stable Rust toolchain. Python installs/verifies model files; playback and inference run natively in Rust. The repository's Rust wrapper supports its private `.toolchain` installation or an existing toolchain.

```sh
npm ci
npm --prefix frontend ci
python3 tools/ensure-beat-models.py
bash tools/rust-env.sh npm run desktop
```

The model installer checks pinned sizes and SHA-256 hashes before replacement. Models are ignored by Git and are fetched before packaging; inference does not download assets during playback. See [model provenance and licenses](src-tauri/resources/beat-models/README.md).

```sh
# Optimized app and drag-to-Applications installer.
bash tools/rust-env.sh npm run package:macos

# Verify bundled identity, icon, model hashes and exclusion of runtime profiles.
python3 tools/verify-release.py target/release/bundle/macos/PulseDeck.app
```

Build outputs live under `target/release/bundle/macos` and `target/release/bundle/dmg`. Local configuration, credentials, databases, audio, captures and validation artifacts are excluded by `.gitignore`.

### Tests and validation

```sh
bash tools/rust-env.sh cargo test --workspace
bash tools/rust-env.sh cargo clippy --workspace --all-targets -- -D warnings
npm --prefix frontend test
npm --prefix frontend run build
python3 tools/check-control-coverage.py
python3 tools/validate-dj-skills.py
python3 tools/test-dj-skill-packaging.py
```

The current regression run passed **473 Rust tests**, **73 frontend tests**, strict Clippy, control-catalog coverage and skill packaging checks (11 opt-in Rust tests ignored; the large-library timing gate excluded). Installer self-tests cover app copy/replacement, skill installation/update/backup, bundled font registration and failure preservation. A real native drag from the mounted DMG installed the app and all three skills into Applications/Codex; the installed app’s signature and bundled assets were verified.

Opt-in real-library/audio tests require their documented inputs. The 100,000-row browser SQL timing test is sensitive to host load and is reported separately when excluded from a regression run. Real-song offline renders exercise transitions, recovery and silence detection; accelerated isolated queue tests establish functional integration, not wall-clock performance or subjective mixing quality.

## Publish a GitHub release

Update the app version consistently in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and the root npm package/lock. Push a matching `vVERSION` tag. The [release workflow](.github/workflows/release-macos.yml) checks version agreement, fetches hash-verified models, builds an Apple Silicon DMG, verifies the bundle and uses `softprops/action-gh-release@v3` to publish the installer and checksum file to GitHub Releases automatically. This follows [Flowbar’s release action](https://github.com/triptu/flowbar/blob/main/.github/workflows/build-dmg.yml), using its currently supported action version. Dispatching the workflow for an existing tag rebuilds and replaces its download assets. Build artifacts are retained for 30 days.

Only the workflow's scoped GitHub token is needed for publishing; no credentials belong in source. Apple Developer signing/notarization is not configured in this release workflow.

## Current limits

- Tracks decode into RAM; very large sources and concurrent analysis require memory headroom.
- Spotify deck playback is experimental and depends on native authorization and provider availability.
- Physical headphone routing, hardware latency, device-loss/sleep stress and listening-quality comparisons need further validation.
- Beat/downbeat/key estimates remain fallible. Phrase/vocal certainty and commercial-software parity are not claimed.
- No general stems mixer, DVS, proprietary player/USB export, Pro DJ Link, cloud sync or complete commercial FX parity.
- The downloadable release currently targets Apple Silicon; Intel builds are not published by this workflow.

## Architecture and evidence

- [Engine](crates/deck-engine/README.md) · [Analysis](crates/audio-analysis/README.md) · [Spotify adapter](crates/spotify-playback/README.md)
- [Successor queue](research/successor-queue-2026-10-19.md) · [Audible boundaries, loudness and effects tails](research/queue-and-audio-2026-10-19.md)
- [Single-call execution](research/single-call-moves-2026-10-19.md) · [Mixxx review](research/mixxx-architecture-and-improvements-2026-10-19.md)
- [Implementation history](research/implementation-status.md) · [Roadmap](research/implementation-roadmap.md)

Third-party provenance and licensing are recorded in [NOTICE.md](NOTICE.md), bundled model notices and vendored dependency license files.

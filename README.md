# PulseDeck

**Your music. One instrument.**

A native, two-deck DJ app for macOS. Mix local music and Spotify, read the beatgrid, shape the overlap, and plan what comes next with **Suggest**. Built with Rust and Tauri, with playback and transition timing owned by the native engine.

[**Download for macOS**](https://github.com/bhaiG-de/pulsedeck/releases/latest) · [Install](#install) · [Explore the app](#inside-pulsedeck) · [Agents & CLI](#agents-and-cli) · [Develop](#develop)

[![PulseDeck's two decks, stacked waveforms and mixer, shown in a close view of a MacBook Pro screen.](docs/images/01-hero.webp)](docs/images/01-hero.webp)

**Two decks, one workspace.** Waveforms, cue points, transport and mixer controls stay together. Choose your next move yourself, or give the successor queue an ordered set to prepare and execute.

## Install

1. Download the Apple Silicon (`aarch64`) `.dmg` from **GitHub Releases → Assets**.
2. Open it and double-click **Install PulseDeck** for the custom installer; drag its PulseDeck icon into the Applications target or use Install. Opening the DMG mounts it in Finder; it does not automatically launch the installer. The installer includes the app inside its own bundle.
3. Launch PulseDeck. Import a local folder or configure Spotify in Settings.

For agent control, select **Install/update DJ skills for Codex**, **Install/update DJ skills for Claude Code**, or both in the installer. See [Claude Code setup](#use-with-claude-code) below.

PulseDeck runs one normal instance per user. Opening it again, including from another app copy, brings the existing window forward. Startup errors show an explanation; invalid optional Spotify settings leave local playback available and retain the settings file for recovery.

Requires an Apple Silicon Mac and macOS 12 or newer. Release downloads include `SHA256SUMS.txt`; verify with `shasum -a 256 -c SHA256SUMS.txt` beside the downloaded DMG.

The current build is ad-hoc signed and **not Apple-notarized**. macOS may require first-launch approval in System Settings → Privacy & Security. User music, library databases and account credentials are not included in downloads.

<details>
<summary>Installer details, DJ skills and bundled tools</summary>

The native installer uses the app’s aluminium chassis, Hanken Grotesk/DM Mono type and exact header mark, with a live drag-position indicator, reactive install target, Reduced Motion support and transactional app replacement. It confirms replacement, requires the installed app to be quit, and preserves the previous app if source copying fails. It does not modify the library or account data.

The installer has separate options to install/update **pulsedeck-control**, **pulsedeck-library** and **djing-art** for Codex and Claude Code. Codex uses `${CODEX_HOME:-~/.codex}/skills`; Claude Code uses `${CLAUDE_CONFIG_DIR:-~/.claude}/skills`. Existing DJ skills (including legacy names) are backed up inside the selected agent's configuration folder, outside skill discovery in `pulsedeck-skill-backups`, before replacement. Unrelated skills are preserved. Restart the selected tools to discover the installed skills. Uncheck both options for app-only installation. In Claude Code, invoke `/pulsedeck-control`, `/pulsedeck-library` or `/djing-art`.

The signed app includes the CLI at `/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl` and self-contained skill packages in `Contents/Resources/dj-skills`; a source checkout is not required for agent control.

</details>

## Inside PulseDeck

### Read the music ahead

Stacked, three-band waveforms put both tracks on the same timeline. Follow the playhead, inspect beatgrid alignment and cue markers, and see the approach to your next move.

[![Close angled view of PulseDeck's stacked waveforms, beatgrid, playhead and cue countdowns.](docs/images/02-waveforms.webp)](docs/images/02-waveforms.webp)

### Decks and mixer

<table>
  <tr>
    <td width="50%"><a href="docs/images/03-deck.webp"><img src="docs/images/03-deck.webp" alt="Close-up of Deck A's yellow jog wheel, tempo, transport and eight hot cues." /></a></td>
    <td width="50%"><a href="docs/images/04-mixer.webp"><img src="docs/images/04-mixer.webp" alt="Close-up of the central mixer with EQ, filters, channel faders, effects and crossfader." /></a></td>
  </tr>
  <tr>
    <td><strong>Hands on the mix.</strong><br />Jog, cue, loop and adjust tempo from one deck surface.</td>
    <td><strong>Shape the overlap.</strong><br />Balance EQ, filters, effects and faders between both decks.</td>
  </tr>
</table>

- Two native audio decks with stacked, three-band waveforms, overview seeking, jog/scratch, tempo control, key lock and pitch shifting.
- Beat-aware launches and Sync, editable beatgrid origin/BPM, hot cues, memory cues, loops and beat jumps.
- Channel trim, three-band EQ, filters, faders, crossfader and master/headphone routing. Physical cue monitoring needs a separate audio output.
- Stereo-linked output headroom and limiting, with gain-reduction and callback diagnostics.
- Echo and reverb on independent main/cue send-return histories. Returns survive a closed fader, paused deck or replaced track, with bounded tails.
- WAV recording for eligible local playback; streaming-service playback and its residual effect tails block recording.
- Native DDJ-FLX4 MIDI/control integration and explicit audio/controller setup.

### Library and analysis

Find the next record in your collection, inspect its metadata, and load it onto either deck. Playlists, tags and search share the same library view.

[![Close view of the local library with collections, track names, BPM, key, duration, deck loading and tags.](docs/images/05-library.webp)](docs/images/05-library.webp)

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

[![Suggest shows ranked next tracks, mixing techniques, tempo differences and a detailed transition timeline.](docs/images/06-suggest.webp)](docs/images/06-suggest.webp)

**A next move with a reason and a time.** Compare the candidate tracks and techniques, inspect preparation requirements, and see when each part of a transition should begin.

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

PulseDeck supports **Codex and Claude Code** through the same bundled CLI and three self-contained DJ skills. Install the skills for either tool, or both, using the installer checkboxes.

Three skills separate [musical judgment](.agents/skills/djing-art/SKILL.md), [performance control](.agents/skills/pulsedeck-control/SKILL.md) and [library knowledge](.agents/skills/pulsedeck-library/SKILL.md). Their [continuous-set workflow](.agents/skills/pulsedeck-control/references/continuous-sets.md) starts one detached queue and ends the agent turn after acceptance. Healthy queues need no `--follow`, periodic reads or model-side timing loop.

### Use with Claude Code

1. Install [Claude Code](https://code.claude.com/docs/en/overview), then select its DJ-skills checkbox in the PulseDeck installer. Codex and Claude Code can be selected independently.
2. Keep PulseDeck open, with local tracks imported or Spotify configured in its Settings.
3. Start a new Claude Code session and invoke a skill, or ask Claude to use the PulseDeck skills. For example:

```text
/pulsedeck-library Show my available local tracks and playlists.
/pulsedeck-control Check PulseDeck's current deck and audio state.
/djing-art Plan a progressive house set from my library without starting playback.
```

The installer follows Claude Code's [personal skill layout](https://code.claude.com/docs/en/skills): `~/.claude/skills/<skill-name>/SKILL.md`, or `skills` inside `CLAUDE_CONFIG_DIR` when configured. Existing PulseDeck skills are backed up before replacement; unrelated skills are preserved. The installed skill packages include their supporting references and use PulseDeck's bundled CLI. The desktop app owns audio and timing during playback.

### Use the CLI directly

The installed app includes `pulsedeckctl`. Define `pd` for the examples below; the desktop app must be running:

```sh
pd() { /Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl "$@"; }
pd status --view compact --json
```

If installed under your home folder, use `~/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl` instead. For development, build with `bash tools/rust-env.sh cargo build -p pulsedeck-cli` and use `target/debug/pulsedeckctl`.

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

Native events are available to background integrations. An event subscription alone does not automatically resume an agent session; do not simulate that integration by repeatedly waking a model to read an idle CLI. Interactive `--follow` remains available for people or host-owned background processes.

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

# Copy the release outside the checkout; test launch, single-instance behavior,
# bundled CLI/UI, library persistence and damaged optional settings.
python3 tools/test-release.py target/release/bundle/macos/PulseDeck.app
# Also exercise muted CoreAudio, recording and native macOS Quit locally:
python3 tools/test-release.py target/release/bundle/macos/PulseDeck.app --audio
```

Build outputs live under `target/release/bundle/macos` and `target/release/bundle/dmg`. Local configuration, credentials, databases, audio, captures and validation artifacts are excluded by `.gitignore`.

<details>
<summary>App icon and README imagery</summary>

The bundled app icon follows the header's dot-matrix record mark: graphite dots with an orange center on a full-bleed silver surface. Its Imagegen master and prompt are in [src-tauri/icons](src-tauri/icons); regenerate bundle sizes with `python3 tools/make_icon.py` after installing npm dependencies.

The six README images show the real production UI in a staged, paused local session. They are 3840 × 2160 WebGL renders on a MacBook Pro model, exported as lossless WebP with pixel-identical decoding to the approved PNGs. Cues are staged; the images do not represent live playback or a completed queue. See [image notes](docs/images/README.md).

</details>

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

The 0.2.2 regression run passed **479 Rust tests**, **73 frontend tests**, strict Clippy, control-catalog coverage and skill packaging checks (11 opt-in Rust tests ignored). Run the 100,000-row timing gate without concurrent release compilation; its wall-clock limits are sensitive to host load. CI reports this benchmark separately without blocking publication on shared-runner speed; functional regressions and installed-DMG checks remain blocking. Installer self-tests cover app copy/replacement, skill installation/update/backup, bundled font registration and failure preservation. A real native drag from the mounted DMG installed the app and all three skills into Applications/Codex; the installed app’s signature and bundled assets were verified.

Opt-in real-library/audio tests require their documented inputs. The 100,000-row browser SQL timing test is sensitive to host load and is reported separately when excluded from a regression run. Real-song offline renders exercise transitions, recovery and silence detection; accelerated isolated queue tests establish functional integration, not wall-clock performance or subjective mixing quality.

## Publish a GitHub release

Update the app version consistently in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and the root npm package/lock. Push a matching `vVERSION` tag. The [release workflow](.github/workflows/release-macos.yml) checks version agreement, runs regression checks, fetches hash-verified models, builds an Apple Silicon DMG, verifies the bundle, launches and tests a copy from the mounted DMG, and uses `softprops/action-gh-release@v3` to publish the installer and checksum file to GitHub Releases automatically. This follows [Flowbar’s release action](https://github.com/triptu/flowbar/blob/main/.github/workflows/build-dmg.yml), using its currently supported action version. Dispatching the workflow for an existing tag rebuilds and replaces its download assets. Build artifacts are retained for 30 days.

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

# Development and release guide

[← Back to PulseDeck](../README.md)

Prerequisites: macOS with Xcode command-line tools, Node.js 22.6 or newer, Python 3 and a stable Rust toolchain. Python installs/verifies model files; playback and inference run natively in Rust. The repository's Rust wrapper supports its private `.toolchain` installation or an existing toolchain.

```sh
npm ci
npm --prefix frontend ci
python3 tools/ensure-beat-models.py
bash tools/rust-env.sh npm run desktop
```

The model installer checks pinned sizes and SHA-256 hashes before replacement. Models are ignored by Git and are fetched before packaging; inference does not download assets during playback. See [model provenance and licenses](../src-tauri/resources/beat-models/README.md).

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

The bundled app icon follows the header's dot-matrix record mark: graphite dots with an orange center on a full-bleed silver surface. Its Imagegen master and prompt are in [src-tauri/icons](../src-tauri/icons); regenerate bundle sizes with `python3 tools/make_icon.py` after installing npm dependencies.

The six README images show the real production UI in a staged, paused local session. They are 3840 × 2160 WebGL renders on a MacBook Pro model, exported as lossless WebP with pixel-identical decoding to the approved PNGs. Cues are staged; the images do not represent live playback or a completed queue. See [image notes](images/README.md).

</details>

## Tests and validation

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

Update the app version consistently in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and the root npm package/lock. Push a matching `vVERSION` tag. The [release workflow](../.github/workflows/release-macos.yml) checks version agreement, runs regression checks, fetches hash-verified models, builds an Apple Silicon installer DMG and a ready-to-run app ZIP, verifies the bundles (including the extracted ZIP), launches and tests a copy from the mounted DMG, and uses `softprops/action-gh-release@v3` to publish both downloads and their checksums to GitHub Releases automatically. This follows [Flowbar’s release action](https://github.com/triptu/flowbar/blob/main/.github/workflows/build-dmg.yml), using its currently supported action version. Dispatching the workflow for an existing tag rebuilds and replaces its download assets. Build artifacts are retained for 30 days.

Only the workflow's scoped GitHub token is needed for publishing; no credentials belong in source. Apple Developer signing/notarization is not configured in this release workflow.

## Current limits

- Tracks decode into RAM; very large sources and concurrent analysis require memory headroom.
- Spotify deck playback is experimental and depends on native authorization and provider availability.
- Physical headphone routing, hardware latency, device-loss/sleep stress and listening-quality comparisons need further validation.
- Beat/downbeat/key estimates remain fallible. Phrase/vocal certainty and commercial-software parity are not claimed.
- No general stems mixer, DVS, proprietary player/USB export, Pro DJ Link, cloud sync or complete commercial FX parity.
- The downloadable release currently targets Apple Silicon; Intel builds are not published by this workflow.

## Architecture and evidence

- [Engine](../crates/deck-engine/README.md) · [Analysis](../crates/audio-analysis/README.md) · [Spotify adapter](../crates/spotify-playback/README.md)
- [Successor queue](../research/successor-queue-2026-10-19.md) · [Audible boundaries, loudness and effects tails](../research/queue-and-audio-2026-10-19.md)
- [Single-call execution](../research/single-call-moves-2026-10-19.md) · [Mixxx review](../research/mixxx-architecture-and-improvements-2026-10-19.md)
- [Implementation history](../research/implementation-status.md) · [Roadmap](../research/implementation-roadmap.md)

Third-party provenance and licensing are recorded in [NOTICE.md](../NOTICE.md), bundled model notices and vendored dependency license files.

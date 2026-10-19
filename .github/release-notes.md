## Stability fixes in 0.2.2

- Opening another app copy focuses the running PulseDeck instead of crashing or restoring the same session twice.
- Startup failures show a useful error; damaged Spotify settings preserve the original file and leave local decks available.
- Invalid device/buffer/routing selections preserve healthy audio output; status reflects renderer failures. Output changes require stopping an active recording first.
- Native macOS Quit finalizes active WAV recording and saves deck cues.
- Sequential batches stop after their first runtime failure, cancelling later actions.
- Decoding checks packet memory limits before growing PCM storage and rejects unsupported sample rates consistently.
- Release publishing now requires regression checks and a launch/UI/library/restart test using the app copied from the DMG.

## Install PulseDeck on macOS

Download the `.dmg` under **Assets**, open it, and launch **Install PulseDeck** for the animated drag-to-install experience. The installer can also install/update the three Codex DJ skills, backing up existing DJ skills before replacement. Restart Codex afterward. Or drag **PulseDeck.app** directly into **Applications** for app-only installation. The `aarch64` build is for Apple Silicon Macs (M1 or newer). macOS 12 or newer is required. `SHA256SUMS.txt` contains the download checksum.

This build is ad-hoc signed and is **not Apple-notarized**. macOS may require approval in System Settings → Privacy & Security before its first launch. No Apple Developer certificate or account credentials are included in this repository or release.

## Included

- Suggest panel and app-owned successor queues with native transition timing, recovery and a duration endpoint.
- Detached agent workflow: no model polling is needed while the queue plays.
- EQ, filter, short exchange, echo-out and reverb-exit transitions.
- Precise decoded-audio boundaries, LUFS normalization and independent effects tails.
- Bundled control CLI, self-contained DJ skills and compact indexed library/control discovery.
- New macOS icon based on the PulseDeck header mark; verified beat-analysis models bundled for offline inference.

Spotify library access and native playback require separate sign-ins through Settings. Spotify audio, user music, library databases and OAuth credentials are not bundled. Recording of streaming-service material is unavailable.

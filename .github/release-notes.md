## Stability fixes in 0.2.2

- Updated 0.2.2 download: one custom installer with the app bundled inside, plus independent Codex and Claude Code DJ-skill installation options and backups.
- Historical mix outcomes no longer appear as a startup toast; new session outcomes still appear when they occur.
- Opening another app copy focuses the running PulseDeck instead of crashing or restoring the same session twice.
- Startup failures show a useful error; damaged Spotify settings preserve the original file and leave local decks available.
- Invalid device/buffer/routing selections preserve healthy audio output; status reflects renderer failures. Output changes require stopping an active recording first.
- Native macOS Quit finalizes active WAV recording and saves deck cues.
- Sequential batches stop after their first runtime failure, cancelling later actions.
- Decoding checks packet memory limits before growing PCM storage and rejects unsupported sample rates consistently.
- Release publishing now requires regression checks and a launch/UI/library/restart test using the app copied from the DMG.

## Install PulseDeck on macOS

**Ready-to-run app:** download **PulseDeck_0.2.2_aarch64.zip**, unzip it, move **PulseDeck.app** to Applications, and open it. No installer or build tools are needed. The app includes its audio engine, offline analysis models, control CLI and DJ skills. Use the DMG below for automatic skill setup in Codex or Claude Code. The ZIP is also not Apple-notarized: if macOS blocks it, dismiss the warning and use System Settings → Privacy & Security → Security → Open Anyway for **PulseDeck.app**, then confirm **Open**.

Download the `.dmg` under **Assets**, open it, and double-click **Install PulseDeck** for the custom drag-to-install experience. Opening the DMG mounts it in Finder; it does not automatically launch the installer. The installer contains the app and has separate options for the three DJ skills in Codex and Claude Code, backing up existing DJ skills before replacement. Restart the selected tools afterward. Turn off both skills options for app-only installation. The `aarch64` build is for Apple Silicon Macs (M1 or newer). macOS 12 or newer is required. `SHA256SUMS.txt` contains the download checksum.

This build is ad-hoc signed and is **not Apple-notarized**. Open **START HERE.html** inside the DMG for offline first-launch instructions. If macOS blocks **Install PulseDeck.app**, dismiss the warning, double-click **Open Privacy & Security.inetloc**, scroll to **Security**, and choose **Open Anyway** for the installer, then confirm **Open**. Approve only a download you trust from this official release. Repeat for **PulseDeck.app** if the installed app is also blocked. On macOS 12, use System Preferences → Security & Privacy → General. The custom installer also includes **First-launch help**. See [Apple’s instructions](https://support.apple.com/en-us/102445). No Apple Developer certificate or account credentials are included in this repository or release.

## Included

- Suggest panel and app-owned successor queues with native transition timing, recovery and a duration endpoint.
- Detached agent workflow: no model polling is needed while the queue plays.
- EQ, filter, short exchange, echo-out and reverb-exit transitions.
- Precise decoded-audio boundaries, LUFS normalization and independent effects tails.
- Bundled control CLI, self-contained DJ skills and compact indexed library/control discovery.
- New macOS icon based on the PulseDeck header mark; verified beat-analysis models bundled for offline inference.

Spotify library access and native playback require separate sign-ins through Settings. Spotify audio, user music, library databases and OAuth credentials are not bundled. Recording of streaming-service material is unavailable.

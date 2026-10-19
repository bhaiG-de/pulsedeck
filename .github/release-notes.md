## Stability fixes in 0.2.2

- Ready-to-run app ZIP with the audio engine, offline analysis models, control CLI and all three Codex/Claude Code DJ skills bundled.
- Historical mix outcomes no longer appear as a startup toast; new session outcomes still appear when they occur.
- Opening another app copy focuses the running PulseDeck instead of crashing or restoring the same session twice.
- macOS Finder/Dock reopening also restores the existing window after it was hidden or minimised. Clean ZIP installation, repeat launches with one process, hidden/minimised window recovery and native quit/restart were verified on Apple Silicon with macOS 26.7.1.
- Startup failures show a useful error; damaged Spotify settings preserve the original file and leave local decks available.
- Invalid device/buffer/routing selections preserve healthy audio output; status reflects renderer failures. Output changes require stopping an active recording first.
- Native macOS Quit finalizes active WAV recording and saves deck cues.
- Sequential batches stop after their first runtime failure, cancelling later actions.
- Decoding checks packet memory limits before growing PCM storage and rejects unsupported sample rates consistently.
- Release publishing now requires regression checks and a launch/UI/library/restart test using the app extracted from the ZIP.

## Install PulseDeck on macOS

Download **PulseDeck_0.2.2_aarch64.zip** under **Assets**, unzip it, move **PulseDeck.app** to Applications, and open it. No installer or build tools are needed. Apple Silicon (M1 or newer) and macOS 12 or newer are required. `SHA256SUMS.txt` contains the download checksum.

The app includes its audio engine, offline analysis models, control CLI and DJ skills. For Codex or Claude Code, copy the three bundled folders from `/Applications/PulseDeck.app/Contents/Resources/dj-skills/` into `~/.codex/skills/` or `~/.claude/skills/`, backing up any existing folders with those names outside the skills folder first. Use the configured agent home if you set `CODEX_HOME` or `CLAUDE_CONFIG_DIR`. Restart the selected agent afterward.

This build is ad-hoc signed and **not Apple-notarized**. If macOS blocks it, dismiss the warning and use **System Settings → Privacy & Security → Security → Open Anyway** for **PulseDeck.app**, then confirm **Open**. On macOS 12, use **System Preferences → Security & Privacy → General**. Approve only a download you trust from the official release. See [Apple’s instructions](https://support.apple.com/en-us/102445).

## Included

- Suggest panel and app-owned successor queues with native transition timing, recovery and a duration endpoint.
- Detached agent workflow: no model polling is needed while the queue plays.
- EQ, filter, short exchange, echo-out and reverb-exit transitions.
- Precise decoded-audio boundaries, LUFS normalization and independent effects tails.
- Bundled control CLI, self-contained DJ skills and compact indexed library/control discovery.
- New macOS icon based on the PulseDeck header mark; verified beat-analysis models bundled for offline inference.

Spotify library access and native playback require separate sign-ins through Settings. Spotify audio, user music, library databases and OAuth credentials are not bundled. Recording of streaming-service material is unavailable.

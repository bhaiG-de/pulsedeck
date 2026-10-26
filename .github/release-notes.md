## PulseDeck 0.2.8

- Import YouTube Music playlists and Liked videos using your own Google Desktop OAuth client. Play matched local files or Spotify recordings; YouTube audio is never downloaded or played directly.
- Browse matched tracks in the library, load them onto either deck and use controller browsing. Matching checks title, artists, duration and recording versions, and caches results.
- Combine the YouTube Music integration with typed DJ performance scores, native transition forecasts, receipts and guarded deck controls.
- Bundle the MCP control client and updated DJ skills. YouTube collection reads and matching are available through MCP; sign-in stays in Settings.
- Open Google Cloud, YouTube Music and Spotify developer links in the default browser.

YouTube playlists are not Suggest or automatic queue scopes. Live Google OAuth and Spotify catalog matching require account-based validation.

## Install PulseDeck on macOS

Download **PulseDeck_0.2.8_aarch64.zip** under **Assets**, unzip it, move **PulseDeck.app** to Applications, and open it. No installer or build tools are needed. Apple Silicon (M1 or newer) and macOS 12 or newer are required. `SHA256SUMS.txt` contains the download checksum.

The app includes its audio engine, offline analysis models, control CLI and DJ skills. For live agent control, also download and extract **pulsedeck-marketplace-0.2.8.zip**, then follow the [plugin installation guide](https://github.com/bhaiG-de/pulsedeck/blob/v0.2.8/docs/AGENT_PLUGIN.md#install-locally). The plugin connects the bundled MCP server and three DJ skills. The welcome screen and **Settings → Agent skills** still support personal skill copies, with backups of existing PulseDeck packages.

This build is ad-hoc signed and **not Apple-notarized**. If macOS blocks it, dismiss the warning and use **System Settings → Privacy & Security → Security → Open Anyway** for **PulseDeck.app**, then confirm **Open**. On macOS 12, use **System Preferences → Security & Privacy → General**. Approve only a download you trust from the official release. See [Apple’s instructions](https://support.apple.com/en-us/102445).

Spotify library access and native playback require separate sign-ins through Settings. Spotify audio, user music, library databases and OAuth credentials are not bundled. Recording of streaming-service material is unavailable.

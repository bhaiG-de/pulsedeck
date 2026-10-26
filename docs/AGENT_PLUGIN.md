# PulseDeck agent plugin

The plugin packages the art, library and control skills with the installed app's
MCP connection. Both Codex and Claude Code use the same skills and native server.
The macOS app remains responsible for audio, Spotify sign-in, source preparation
and sample-clock execution. No account files, music or separate engine binary are
included in the plugin.

## Build and develop

```sh
python3 tools/package-agent-plugin.py
python3 tools/test-agent-plugin.py
```

The canonical instructions remain in `.agents/skills`; the packager rebases their
shared references into a self-contained `skills/` tree. Do not edit generated
copies. It derives the plugin version from `package.json`, emits matching Codex
and Claude manifests, and refuses to overwrite an existing output. Use a
fresh `--output` directory when rebuilding a development version.

The launcher resolves its own `skills/` directory at runtime and passes it to the
bundled MCP server. Connection instructions report absolute paths to the three
skill entrypoints and waveform reasoning guide. Moving the plugin therefore
does not depend on an old personal skill directory. A missing required guide
fails initialization with a repair message rather than silently omitting it.

The output is `target/agent-plugin-marketplace`, with both marketplace catalogs,
`plugins/pulsedeck`, plugin/marketplace ZIPs and SHA-256 checksums. The plugin ZIP
can be unpacked as a standalone plugin. Unpack the marketplace ZIP to install in
either host using the commands below, replacing the path with the extracted folder.
Open the current `/Applications/PulseDeck.app` before using the tools.

## Install locally

Codex:

```sh
codex plugin marketplace add ./target/agent-plugin-marketplace
codex plugin add pulsedeck@pulsedeck-local
```

Claude Code:

```sh
claude plugin marketplace add ./target/agent-plugin-marketplace
claude plugin install pulsedeck@pulsedeck-local
```

Start a fresh agent session after installation. Confirm that the plugin exposes
`performance_observe`, `deck_control`, `track_inspect` and `spotify_liked`, then
make one read-only observation of the installed app. Only after that check,
disable any manually registered duplicate PulseDeck MCP server and back up/remove
duplicate personal DJ skills. Existing sessions keep their loaded integrations;
do not change a running performer's tools or skill package.

A development update is a fresh package, marketplace refresh and plugin update,
followed by a new session. Test discovery and recovery before a live performance.
Do not infer API compatibility solely from an app version: older app builds may
share the same version while lacking the typed performance tools. Missing tools
require an app/connection update, not a shell fallback.

The package is a local desktop integration. Publishing it to a public marketplace
is a separate step; these commands do not publish anything.

Format references: [Codex packaging](https://developers.openai.com/plugins/build/plugins)
and [Claude Code manifests](https://code.claude.com/docs/en/plugins-reference).

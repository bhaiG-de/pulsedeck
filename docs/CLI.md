# CLI and agent execution

[← Back to PulseDeck](../README.md#agent-workflows)

The installed app includes `pulsedeckctl`. Define `pd` for the examples below; the desktop app must be running:

```sh
pd() { /Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl "$@"; }
pd status --view compact --json
```

If installed under your home folder, use `~/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl` instead. For development, build with `bash tools/rust-env.sh cargo build -p pulsedeck-cli` and use `target/debug/pulsedeckctl`.

```sh
# Bounded analysed opener shortlist; no repeated Spotify page fetches.
pd call mix.openers --args '{"playlist":"spotify:liked","provider":"spotify","min_bpm":118,"max_bpm":126,"limit":8}' --json

# Optional autonomous selection after loading/playing the opener.
pd mix queue start --playlist spotify:liked --provider spotify --energy-direction keep --duration-seconds 3600 --request-id my-set-1 --wait-ready --json

# Explicit status/stop when requested or an actionable outcome needs attention.
pd mix queue status --queue-id QUEUE_ID --json
pd mix queue stop --queue-id QUEUE_ID --json

# Cheap discovery; request one full schema only when needed.
pd commands list --prefix mix. --json
pd commands describe mix.queue.start --json
```

`--wait-ready` waits on startup events until the first successor is armed or preparation fails, then returns. Acceptance alone does not establish readiness. It does not follow a healthy queue for the rest of the set. Collection queues prepare unanalysed candidates when the analysed pool has no feasible move.

`mix.options` defaults to a compact shortlist with evidence/warnings; `detail:true` exposes full passage/guard data. `commands list` returns concise names/descriptions; `--detail` opts into the full catalog. Request IDs make uncertain retries idempotent. Read-only field selection, event cursors and source/control revision guards are shared with the UI.

Native events are available to background integrations. An event subscription alone does not automatically resume an agent session; do not simulate that integration by repeatedly waking a model to read an idle CLI. Agents can follow individual moves or use direct native controls when making their own musical decisions. Interactive `--follow` and focused event/state reads are available for performing and observing.

## Skills and workflow references

- [All three skills](../.agents/skills/README.md)
- [Continuous sets](../.agents/skills/pulsedeck-control/references/continuous-sets.md)
- [Collections, tags and playlists](../.agents/skills/pulsedeck-library/references/collections.md)
- [Saved cues and beatgrids](../.agents/skills/pulsedeck-library/references/track-knowledge.md)

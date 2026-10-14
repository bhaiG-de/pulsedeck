# PulseDeck DJ skills

These seven repository-local skills teach an AI agent with shell access to operate the existing `pulsedeckctl` CLI. PulseDeck supplies observations and executes controls; the agent makes musical choices. There is no MCP server, new musical analysis subsystem, or fixed DJ decision engine.

| Skill | Use it for |
| --- | --- |
| [pulsedeck-track-inspection](pulsedeck-track-inspection/SKILL.md) | Inspect analysis and timestamped waveforms; propose entries/exits; mark requested cues without moving playback |
| [pulsedeck-set-preparation](pulsedeck-set-preparation/SKILL.md) | Select and order local tracks for a set; explain choices and duration assumptions |
| [pulsedeck-prepare-next-track](pulsedeck-prepare-next-track/SKILL.md) | Prepare an incoming deck while protecting current playback |
| [pulsedeck-transition](pulsedeck-transition/SKILL.md) | Plan or execute a two-track transition using evidence, guarded controls and native ramps |
| [pulsedeck-session-dj](pulsedeck-session-dj/SKILL.md) | Repeat selection/preparation/transition/verification, recover from failures, and yield to takeover |
| [pulsedeck-recording-and-export](pulsedeck-recording-and-export/SKILL.md) | Record eligible mixes and migrate library/cue metadata with rekordbox XML |
| [pulsedeck-setup-and-troubleshooting](pulsedeck-setup-and-troubleshooting/SKILL.md) | Diagnose and configure audio, cue routing, MIDI, jobs and authentication |

## Install and use

Open an agent in this checkout with shell access. Clients supporting repository-local `.agents/skills` can discover each folder's `SKILL.md`. Otherwise give the agent the relevant file path and ask it to read that skill. For example: “Use pulsedeck-track-inspection at .agents/skills/pulsedeck-track-inspection/SKILL.md to inspect deck A and suggest three exits.” Explicit `$pulsedeck-transition` invocation is also available in clients that support skill names. Restart/rescan the client's skill catalog when needed.

Build the client and updated desktop, open PulseDeck, and bind the intended socket as described in [CLI reference](references/cli.md). The agent needs an executable client, access to that private socket, and paths available on the machine running PulseDeck. A normal agent must inspect the requested live session; isolated fixture bindings are for validation only.

```sh
bash tools/rust-env.sh cargo build -p pulsedeck-cli
npm run package -- --debug
```

To install standalone copies in a client-managed skill directory, run `python3 tools/install-dj-skills.py --destination /path/to/agent/skills`. The installer refuses existing skill folders and includes shared references inside each copy. It does not install software, alter accounts, start playback, or configure MCP. Repository-local use needs no copying.

## Route requests

“Explain the transition and wait” uses planning mode and read-only observations. “Mix from A to B now” authorizes the transition procedure; don't ask again for each already-authorized step. “Get B ready but don't begin” authorizes preparation, not master-output playback. “Play this playlist and handle transitions” authorizes repeated performance within the requested scope, with continuous verification. On a takeover request, stop issuing automated changes and verify cancellation/state instead of restoring a cached plan over the user's controls.

Read only the relevant skill and shared references. [Musical evidence](references/musical-evidence.md) explains confidence, grids, phrase hypotheses and waveform limits; [CLI reference](references/cli.md) centralizes discovery, identity guards, completion stages, errors, jobs and cancellation. Concrete example IDs/times must be replaced by selected, observed values. No skill claims waveform inspection is listening, that a grid proves a downbeat, or that shell polling provides exact phrase-timed transport.

## Trial and validation

One dedicated agent authored and trialled each skill against isolated generated tracks, then revised it from actual results. Trials leave master and headphone gains muted. The generated signals test software observations/controls, not real musical taste.

```sh
npm test
bash tools/test-dj-skills.sh
```

The second command uses the built client/bundle and a temporary isolated profile, executes the combined two-track shell workflow, validates durable cues/playlist order/readiness/ramps/recording and takeover, and prints the retained report path. It starts the default audio device with both buses muted; hardware listening is not part of the test. Skill frontmatter/link checks and detailed results are described in [validation results](../../research/dj-skills-validation.json).

Audible review remains necessary for meter/downbeat/phrase feel, vocal or harmonic clashes, perceived level, bass balance, effect tails, pitch artifacts and real output/headphone routing. A full-length autonomous set and physical MIDI/live Spotify require separate trials with the user's music, account or hardware.

Fresh blind agents also tested natural-language tasks using only these skills and CLI discovery, with no app-source context. The first timed set exposed cross-exec clock and control-latency issues; timing guidance was revised and a fresh retest measured its actual duration and incomplete track coverage. Preparation and takeover checks passed; a 30-minute Afro house request stopped before playback because genre identity/material were unavailable. See the validation JSON for failures, bounded retest results and audible limitations.

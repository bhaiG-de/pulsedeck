# PulseDeck CLI

`pulsedeckctl` controls the running PulseDeck desktop process. The client does not create another audio engine or open another library database. UI controls, MIDI and CLI commands use the existing native application services.

Build and run:

```sh
bash tools/rust-env.sh cargo build -p pulsedeck-cli
target/debug/pulsedeckctl --help
target/debug/pulsedeckctl status --json
```

Open PulseDeck first. `npm run cli -- ...` also runs the client from the repository. To install a standalone client using your standard Rust installation, run `cargo install --path crates/pulsedeck-cli`. The executable is named `pulsedeckctl` because `pulsedeck` is the desktop executable.

The default endpoint is `/tmp/pulsedeck-<uid>/control.sock`, in a directory owned by the current user with permissions 0700. An exclusive lock prevents two runtimes owning the same endpoint. Set `PULSEDECK_SOCKET` when starting the app and use `--socket PATH` on the client to select a separate instance.

## Discover all commands

```sh
pulsedeckctl commands list --json
pulsedeckctl commands describe engine.command --json
pulsedeckctl commands describe controller.action --json
pulsedeckctl ui state --json
```

The catalog exposes 81 product methods, all 52 native engine variants (49 original actions plus explicit-time hot cues and native ramp controls), the complete controller action schema and live UI targets/actions. Argument examples and schemas are returned with the methods. Unsupported controls and future capabilities are explicitly listed.

Every catalog method can be called directly:

```sh
pulsedeckctl call engine.command --args '{"type":"set_fx","deck":"A","echo":0.3,"reverb":0.1}' --json
pulsedeckctl call controller.action --args '{"action":{"type":"pad_mode","deck":0,"mode":"beat_jump"}}' --json
pulsedeckctl call controller.action --args '{"action":{"type":"pad","deck":0,"mode":"beat_jump","pad":1,"pressed":true,"shifted":false}}' --json
```

Human commands use decks A/B, hot cues A-H and sampler slots 1-16. Native JSON uses zero-based controller decks/pads/sampler slots. `engine.command` accepts A/B or 0/1 for a top-level deck. Time arguments explicitly support `s` and `ms`; ramp duration also supports `beats`. Gain/EQ are 0..2 with unity at 1; channel faders/cue mix are 0..1; filter/crossfader are -1..1 with neutral at 0. Numeric limits are enforced by the native command validator and reflected in discovery.

## Common actions

```sh
pulsedeckctl library import --path /music/song.flac
pulsedeckctl library tracks --search ambient --sort bpm --json
pulsedeckctl deck A load --track local:42
pulsedeckctl deck B load --path /music/next-song.wav
pulsedeckctl deck A play
pulsedeckctl deck A pause
pulsedeckctl deck A seek --at 32s --snap beat
pulsedeckctl deck A cue set --at 8s
pulsedeckctl deck A cue return
pulsedeckctl deck A hotcue H set --at 120s
pulsedeckctl deck A hotcue H trigger
pulsedeckctl deck A hotcue H clear
pulsedeckctl deck A memory save
pulsedeckctl deck A memory next
pulsedeckctl deck A loop set --beats 8
pulsedeckctl deck A loop off
pulsedeckctl deck A jump --beats -4
pulsedeckctl deck A tempo set --percent -2
pulsedeckctl deck A pitch set --semitones 2
pulsedeckctl deck A keylock on
pulsedeckctl deck A sync on
pulsedeckctl deck A master
pulsedeckctl deck A eq set --low 0.5
pulsedeckctl deck A filter set -0.3
pulsedeckctl deck A fader set 0.7
pulsedeckctl mixer crossfader set 0
pulsedeckctl mixer master-gain set 0.8
pulsedeckctl sampler slot 16 load --path /music/sample.wav
pulsedeckctl sampler slot 16 trigger
pulsedeckctl sampler slot 16 stop
pulsedeckctl sampler list --json
pulsedeckctl recording start --path /music/mix.wav
pulsedeckctl recording stop
```

Play/pause setters are deterministic. Toggle, press/release and pad gestures preserve the existing hardware semantics. Triggering a hot cue starts playback; ordinary seek preserves the play state. A cue press while paused previews the cue until release, while a cue press during playback stops and returns to the cue. Pair raw cue/jog/pad press commands with releases. `automation stop` stops ramps and releases held cue/jog state. A load rejects a playing deck by default; `--replace-playing` makes replacement explicit. Input/output paths resolve relative to the CLI's working directory.

## Durable cue metadata and playlists

```sh
pulsedeckctl playlist create --name "Work session"
pulsedeckctl playlist add local:7 --track local:42
pulsedeckctl playlist reorder local:7 --tracks '[42,61,12]'
pulsedeckctl playlist rename local:7 --name "Evening set"
pulsedeckctl call cues.get --args '{"source":"local:42"}' --json
pulsedeckctl call cues.set --args '{"source":"local:42","kind":"hot","index":0,"seconds":32,"label":"Intro","expected_revision":3}' --json
pulsedeckctl call cues.set --args '{"source":"local:42","kind":"memory","index":0,"seconds":64,"label":"Break"}' --json
pulsedeckctl library export-rekordbox --path /music/library.xml
pulsedeckctl library import-rekordbox --path /music/library.xml
```

Cues are stored by canonical source identity for local and Spotify tracks. An edit commits its database revision and updates loaded copies without seeking or changing playback. Hot cues have 8 slots; memory points have 10 and are ordered by time. Annotations follow memory points when the order changes. Supplying `seconds:null` explicitly clears a point; omitting `seconds` in a single-point edit is an error. A `cues` object can update explicit cue arrays while preserving omitted metadata fields. Unknown fields are rejected.

Loaded decks sharing a source share committed cue metadata. Cue revision guards reject stale edits. `--expected-load-revision N` protects native commands against a changed deck source; read the load revision from a successful load or `status`. Local playlists hold unique local track IDs, and replacement/reorder uses a database transaction. Removing a library track removes its database entry and playlist membership; its audio file remains on disk.

Recording and XML export protect existing files. Pass `--overwrite` for intentional replacement. Source recording restrictions continue to apply, including native Spotify sources.

## UI presentation commands

```sh
pulsedeckctl ui theme set anatomy-light
pulsedeckctl ui app source spotify
pulsedeckctl ui app browser true
pulsedeckctl ui drawer height 44
pulsedeckctl ui app zoom 64
pulsedeckctl ui app snap true
pulsedeckctl ui app search ambient
pulsedeckctl ui app selection local:42
pulsedeckctl ui deck-A pads memory
pulsedeckctl ui deck-A grid-editor true
pulsedeckctl ui deck-A grid-draft --value '{"bpm":124,"offset":0.1}'
pulsedeckctl ui deck-A grid-apply
pulsedeckctl ui app settings true
pulsedeckctl ui app settings-tab spotify
pulsedeckctl ui spotify-settings client-id YOUR_PUBLIC_CLIENT_ID
pulsedeckctl ui mixer fx-view reverb
```

`ui state` lists mounted targets, available actions and committed view state. Targets include `app`, `drawer`, `mixer`, `deck-A/B`, `analysis-A/B` and the separate Spotify browser/settings views. Some targets exist only after loading a track or opening the corresponding panel. Presentation commands wait for the view's post-render acknowledgment and return stage `rendered`. Unmounted targets return `ui_unavailable` with available targets. The UI bridge invokes the actual setters/handlers used by the interface. Numeric/boolean/object values are JSON values; use `--value '"120"'` when text consisting of digits must remain a string.

## Machine output and actionable failures

`--json` produces one response envelope on stdout; diagnostics remain on stderr. Watch and stdio session modes use newline-delimited JSON. Errors carry a stable code, explanatory message, retryability, hint and relevant correction details. Unknown flags are rejected, typos receive suggestions, and bad values report allowed values/types/ranges. Discovery provides corrected command examples and input schemas.

Example wrong option:

```json
{"ok":false,"error":{"code":"invalid_arguments","message":"Unknown or inapplicable option --precent for this command","retryable":false,"details":{"argument":"--precent","suggestions":["percent"],"valid_options":["--percent"],"usage":"deck A|B ACTION [arguments]","example":"pulsedeckctl deck A tempo set --percent -2 --json"}}}
```

Exit codes: 2 invalid input/protocol; 3 runtime/transport unavailable; 4 state conflict or unsupported operation; 5 operation/persistence failure; 6 timeout; 7 cancellation. A timeout does not prove that the operation failed or was cancelled. Inspect state/job progress, and reuse the same request ID when retrying an uncertain transport result. The runtime retains 256 recent mutation receipts for duplicate suppression and reports a request-ID conflict if arguments differ. Query failures explain which state/action is required next.

Stages distinguish `accepted`, native `applied`, load `ready`, metadata `persisted` and view `rendered`. Native fences acknowledge queue application after a published snapshot, including when audio is stopped. Readiness for pitch/key-lock preparation can be awaited separately:

```sh
pulsedeckctl deck A wait
pulsedeckctl state watch --interval 250ms
pulsedeckctl events watch --count 10
pulsedeckctl session --stdio
```

## Batches and native ramps

```json
[
  {"command":"deck.load","args":{"deck":"A","track":"local:42"}},
  {"command":"engine.command","args":{"type":"play","deck":"A"}},
  {"command":"automation.ramp","args":{"target":{"type":"crossfader"},"value":1,"clock":"A","beats":16,"quantize":4}},
  {"command":"engine.wait","args":{"ramps":true}}
]
```

```sh
pulsedeckctl batch validate --file set.json
pulsedeckctl batch run --file set.json --json
pulsedeckctl jobs wait JOB_ID
pulsedeckctl jobs cancel JOB_ID
pulsedeckctl automation stop
pulsedeckctl mixer crossfader ramp --to 1 --over 16beats --clock A --quantize 4
```

Batches validate syntax before starting and stop at the first runtime failure. Results show executed steps and partial application; audible actions are not rolled back. `--no-wait` on long load/import/UI/wait operations returns a job ID. Cancellation stops future steps and interrupts engine waits; cancelling a provider preparation also has the explicit `deck.load.cancel` operation. Native ramps advance in the audio renderer with fixed storage, so shell startup/tool latency does not set the ramp duration. Manual movement of a ramped control takes over that control. Beat-clock ramps cancel on pause, seek or source replacement. Beats use the declared BPM/grid; phrase/section/downbeat inference is a future capability.

## Verification and remaining scope

Run the full Rust/frontend suite, strict linting, production build and coverage checks with `npm test`. For desktop integration, build the application and client, then run the isolated harness:

```sh
npm run package -- --debug
bash tools/rust-env.sh cargo build -p pulsedeck-cli
bash tools/test-desktop.sh --audio
bash tools/rust-env.sh cargo run -p deck-engine --example native_smoke
```

The desktop harness starts a temporary profile with separate library, preferences, credential storage and socket; it then restarts the process to check persistence. `--audio` adds muted playback and WAV recording on the default output device. Omit it to test without starting audio. Test profiles and JSON reports are retained under the printed temporary directory for inspection.

`tools/control-coverage.json` maps every one of the 54 pre-existing desktop handlers to its CLI equivalent. `python3 tools/check-control-coverage.py` checks the registry, dispatcher, all engine variants and all 13 MIDI action kinds. Rust tests cover protocol, real socket/CLI round trips, guards, durable metadata, atomic playlist edits and native ramp behavior. Frontend tests cover bridge validation/dispatch and existing rendering/navigation. `tools/cli_smoke.py` exercises an isolated running app, optionally using muted real CoreAudio and WAV recording, and verifies persistence after restart.

Physical controller/routing verification and live Spotify authorization require the corresponding hardware/account. Parsed controls with no software implementation (mic gain, Android mono, Smart Fader Next and shifted loop-in/out) remain explicitly unsupported. Spotify playlist writes, embedded-widget transport, musical-section detection, scheduled native transport and an MCP server remain future work. The CLI supplies the control, metadata, job and diagnostic foundation for that work.

For musical operation through shell agents, see the [seven DJ skills](../.agents/skills/README.md). `track.inspect` provides bounded timestamped overview/range views of existing analysis, with full metadata and cue revisions, without seeking or reanalyzing.

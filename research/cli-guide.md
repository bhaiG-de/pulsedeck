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

The catalog exposes product methods, native engine variants, controller actions and live UI targets, with current schemas and examples. It includes full-model analysis, grid correction and native queued launch; use discovery for the installed build's complete inventory. Unsupported controls and future capabilities are listed explicitly.

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
pulsedeckctl deck B analyze
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

Play/pause setters are deterministic: synced Play can queue the next master beat, so an acknowledgment alone does not prove playback has started. Toggle and press/release gestures preserve hardware semantics. A hot cue trigger starts playback; ordinary seek preserves play state. A cue press while paused previews until release; during playback it stops and returns to cue. Pair raw cue/jog/pad presses with releases. `automation stop` cancels ramps and pending launches and releases cue/jog holds. A load rejects a playing deck by default; `--replace-playing` explicitly requests replacement. Paths resolve relative to the CLI's working directory.

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
pulsedeckctl ui theme set ivory
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

Batches validate syntax before starting and stop at the first runtime failure. Results show executed steps and partial application; audible actions are not rolled back. `--no-wait` on long operations returns a job ID. Cancellation stops future steps and interrupts engine waits; provider preparation also has `deck.load.cancel`. Native ramps advance in the audio renderer with fixed storage, so shell latency does not set their duration. Manual movement takes over the ramped target. Beat-clock ramps cancel on pause, seek or source replacement. Their effective grid may use variable neural beat timestamps. Downbeats are model predictions or manual corrections; musical phrase/section selection remains an evidence-based DJ decision.

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

Physical controller/routing verification and live Spotify authorization require the corresponding hardware/account. Parsed controls with no software implementation (mic gain, Android mono, Smart Fader Next and shifted loop-in/out) remain explicitly unsupported. Spotify playlist writes, embedded-widget transport, musical-section detection, general wall-clock transport scheduling and an MCP server remain future work. The CLI supplies the control, metadata, job and diagnostic foundation for that work.

For musical operation through shell agents, see the [seven DJ skills](../.agents/skills/README.md). `track.inspect` provides bounded waveform and beat-event range views, compact neural metadata and cue revisions, without seeking or reanalyzing.

Song tags are free-form strings stored in the local library. Edit them inline in the Tags column (Enter to add, × to remove), or run `pulsedeckctl library track tags local:42 --tags '["Afro house","chill vibe"]' --json`. Supplying `[]` clears tags. Library search includes tags, and `track.inspect` returns them as `metadata.tags`. Tags survive reimport/reanalysis and do not alter audio files.


## Beatgrid correction and queued launch

PulseDeck analyzes decoded PCM in memory with full FP32 Beat This! `final0` and native BSD madmom-style DBN/HMM postprocessing. Default decoding compares whole-track hypotheses of three, four, five or seven beats per bar over 55–300 BPM; it enforces temporal/bar continuity while retaining locally varying decoded timestamps. Arbitrary meter changes and musical phrases are outside that decoder policy. Mean BPM remains a summary. Scores are uncalibrated; rekordbox parity is unproven. A nonnull `rhythm_error` requires manual validation/correction before automated beatmixing, even when an older estimate remains available.

`track.inspect` returns bounded `beat_events.events`, with `seconds`, `beat_index`, effective anchor-relative `beat_position` and `downbeat`. Its view includes counts/truncation, model frame resolution 0.02 seconds, `source`, `manual` and `uniform`; events are range-filtered and capped at `points` (maximum 512). With `uniform:false`, indices identify decoded neural events; with `uniform:true`, they identify the effective constant-tempo lattice, including any lead-in indices. Narrow a truncated range before counting progression. `metadata.neural_rhythm` retains original model/BPM/counts/scores/time with `scores_calibrated:false`, so its counts can differ from edited effective events. Loaded-deck flags are authoritative; an unloaded cache with `bpm_source:manual` is inferred to use a uniform override.

Use `pulsedeckctl deck B grid set --offset 0.125s --expected-load-revision N` to set a known bar origin, or **Set bar 1 here** in the grid editor. Both invoke `set_beatgrid`; neither moves transport or cues. Correct base BPM with `deck B bpm set BPM` when necessary; change playback tempo with `deck B tempo set --percent PERCENT` for matching another song. The editor offers 1 ms draft nudges and half/double BPM. Manual grid edits persist through reload/reanalysis. `beatgrid_manual` distinguishes an explicit override; `beatgrid_downbeat` can also reflect model-predicted downbeats, so it does not mean a manual edit occurred. A bar origin remains separate from a musical phrase.

Explicit manual BPM correction selects a constant-tempo grid for the entire source, including half/double edits; it is not merely a display change or a stretch of existing neural markers. Offset-only realignment preserves neural intervals when that mode is active and leaves a uniform override uniform. Manual bar-origin numbering retains detected meter, falling back to four if unknown. Inspect `beatgrid_uniform` and `beat_events.uniform` to distinguish modes. The UI applies BPM only when its draft changed by more than 0.01 BPM. Playback-rate/tempo matching preserves the mode.

For an authorized analysis refresh, `pulsedeckctl deck B analyze --expected-load-revision N --json` runs full analysis on the loaded source; wait for completion, then reinspect identity, error/provenance and events. An inspection-only request remains read-only. Source guards protect identity, not every concurrent knob or seek.

After choosing B's arrival event and entry-to-arrival beat count, count backward from A's handover and select its actual future grid-event timestamp. Use `pulsedeckctl deck B launch --clock A --clock-revision A_REV --at A_SECONDSs --expected-load-revision B_REV`, substituting observed revisions/times. The renderer snaps B's paused entry to its nearest effective beat and queues a sample-clock start on A's chosen timestamp. A must be playing; both grids must be valid, effective tempos must match within 0.01 BPM, neither deck may have a loop/jog hold, incoming preparation must be ready, and audio must run. Mean-BPM timestamp arithmetic can miss a variable grid; use inspected event times. Omitting `--at` selects the next boundary; `--quantize 4`/`32` counts beats, with bar/phrase interpretation requiring separate evidence.

Inspect `launch_pending`/`launch_at_seconds`, wait with `pulsedeckctl call engine.wait --args '{"deck":"B","launches":true}' --expected-load-revision B_REV --timeout 30s --json`, then verify B playing and both source revisions. Compute phase from simultaneous native `beat_position` values, wrapping the difference to `[-0.5,0.5)` beats; do not use a uniform BPM/offset formula. Keep B isolated until phase is checked and monitor drift during overlap. A cancelled launch also clears pending state, so wait completion alone does not prove success. Source/timeline/tempo/grid changes and manual takeover cancel stale launches. `deck B launch cancel` or `automation stop` cancels pending launch. Synced Play queues the next master beat; unsynced Play is immediate. Native timing cannot choose a phrase or prevent vocal/bass clashes. See [DJ control reference](../.agents/skills/references/cli.md#correct-a-grid-and-queue-a-musical-launch).

Section analysis and next-track suggestions extend the existing Beat This! clock. Use `analysis.settings.get`, `analysis.settings.set`, `library.analysis.update`, `library.analysis.cancel`, `deck.sections.vocabulary`, `library.suggest_next` and `library.history.clear` through `pulsedeckctl call`. The `beat_model_spotify` compatibility setting enables Spotify section analysis; core beat detection remains part of source preparation.

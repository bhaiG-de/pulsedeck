# PulseDeck shell control reference

Use this for endpoint setup, source guards, completion stages, jobs, ramps and errors. Skills supply musical judgment; the runtime executes controls. No MCP is required.

## Bind the actual runtime

From the repository, build `bash tools/rust-env.sh cargo build -p pulsedeck-cli` and open the current PulseDeck desktop bundle. To use an installed client, choose its absolute path instead. Use the socket belonging to the requested session; inspect status before any mutation.

```sh
export PULSEDECK_CLI="$PWD/target/debug/pulsedeckctl"
export PATH="$(dirname "$PULSEDECK_CLI"):$PATH"
# Default endpoint; override for a separately started instance.
export PULSEDECK_SOCKET="/tmp/pulsedeck-$(id -u)/control.sock"
pd() { "$PULSEDECK_CLI" "$@" --socket "$PULSEDECK_SOCKET" --json; }
pd status
pd commands list
pd commands describe track.inspect
pd commands describe engine.command
pd commands describe automation.ramp
```

For isolated trials, `python3 tools/skill-trials/profile.py start NAME --audio` creates generated tracks, copies the built app, isolates credentials/library/preferences/socket and mutes master AND cue gain. Bind `PULSEDECK_CLI` and `PULSEDECK_SOCKET` to the returned manifest values, not the default endpoint. Stop only that profile with `python3 tools/skill-trials/profile.py stop NAME`. Omit `--audio` for metadata-only work. Real audio trials require an output device; muted software output does not prove hardware isolation.

Concrete IDs, paths, revisions, timestamps and BPM in examples must come from discovery/current observations. A skill's illustrative local ID is a parameter, not a claim that your library contains that ID. Store substantial JSON in temporary files and read the needed fields; don't flood model context with full waveforms.

## Observe and identify

`status.result` includes `engine`, compact `metadata`, `controller`, `audio`, `midi`, `recording`, source jobs and analysis activity. `engine.load_revisions[0/1]` identify current deck source epochs. Deck state contains `playing`, `position_seconds`, `position_revision`, BPM/rate, preparation state, EQ/filter/FX/fader/cue and loop settings. Inspect full state/catalog for exact current field names.

`library.tracks` is paginated (`tracks`, `total`, `next_offset`) and sorts title/BPM/duration. Playlist filtering preserves membership, not saved order. `playlist.list` returns saved `track_ids`; use those to reconstruct the playlist order. Local source IDs are `local:ID`; playlist create returns an integer `id`, and reorder takes unique integer track IDs. Spotify playlist writes are unsupported.

```sh
pd library tracks --sort bpm --limit 100 --offset 0
pd playlist list
pd call track.inspect --args '{"deck":"A","points":64}'
pd call track.inspect --args '{"deck":"A","start_seconds":8,"end_seconds":24,"points":128}'
pd call cues.get --args '{"deck":"A"}'
```

The range example requires a loaded track lasting at least 24 seconds. `track.inspect` takes exactly one of `source` or `deck`, optional `start_seconds`/`end_seconds` and `points` 1..512 (default 64). Result: canonical `source`, optional `load_revision`, compact `metadata`, cues/revision, requested range, stored waveform resolution/bands/bins and bounded `beat_events`. Each event has `seconds`, `beat_index`, effective anchor-relative `beat_position` and `downbeat`. The event view reports `total_in_range`, `truncated`, `timing_resolution_seconds:0.02`, `source`, `manual` and `uniform`. With `uniform:false`, indices identify neural events; with `uniform:true`, they identify the manual constant-tempo lattice and must not be labeled model detections. Events are range-filtered and capped at `points`; narrow a truncated range before counting progression. `metadata.neural_rhythm` summarizes original model/BPM/counts/scores/time with `scores_calibrated:false`; its counts can differ from effective edited events. More points do not interpolate neural detections or exceed stored waveform resolution. Waveform boundaries can straddle the range, peaks are maxima, bands aggregate shared-scale RMS, and null bands are missing evidence. This reads analysis, not musical sections. Save full metadata to a file when needed.

## Apply and verify

Human CLI decks A/B, hot cues A-H, samples 1-16; JSON controller decks/pads/samples are zero-based. Top-level native deck accepts A/B or 0/1. Cue source revision and deck load revision protect different things: `cues.set.args.expected_revision` guards saved markers, `--expected-load-revision N` guards deck source identity. Neither guards manual fader/tempo/position changes; re-observe those between phases.

For `deck B launch --clock A`, `--clock-revision` is A's **load revision** from `engine.load_revisions`, not A's `position_revision`. The incoming `--expected-load-revision` guards B. Keep the source guard and musical timing/bar checks separate; a valid launch can still put the tracks in different parts of their bars.

Responses have `ok`, `request_id`, `stage`, optional `result` and structured `error`. A success with no result is valid. Inspect stderr/exit code AND JSON. Stages: `accepted` queued/job started; `applied` command fence published; `ready` source load complete; `persisted` cue metadata committed; `rendered` UI acknowledgment. `applied` is not proof that 8ms audio control smoothing has settled or pitch/key-lock preparation has completed. `deck B wait` waits preparation; `call engine.wait --args '{"ramps":true}'` waits all active ramps. A native cue-edit response may be `applied` with `cue_revision` even when persistence succeeded; verify `cues.get` explicitly.

Watch-style CLI calls such as `jobs wait` can emit several newline-separated JSON envelopes. Parse/retain each line and inspect the terminal envelope plus process exit code; `json.loads(all_stdout)` is not a valid general parser. A parser exception after `batch.run` acceptance does not imply no controls applied: inspect the job and native endpoints before retrying.

For a batch that starts several ramps, wait for its job to reach `completed` and inspect all step results **before** waiting for native ramp completion. A concurrent `engine.wait` only covers ramps already submitted; it can return before the batch's later steps are dispatched and can delay those steps. Then inspect endpoints and active ramps. A batch accepted response, a completed job and completed ramps are three different states.

Ordinary seek preserves play state, but hotcue trigger starts playback. Explicit-time cue edits preserve playhead. Preserve existing markers and user labels unless editing is requested. A load refuses playing replacement unless `--replace-playing` is explicit; do not use that flag for normal next-track preparation. Loading itself is not a CAS against every possible concurrent manual change.

## Batches, timing and takeover

Batch files contain an array of full request objects (`command`, `args`, optional `expected_load_revision`). Validate before run. Batch returns accepted `job_id`; `jobs get` inspects even a failed job, whereas `jobs wait` exits 5 for failed, 7 for cancelled, 6 if its wait times out. Inspect executed step results and final native state; failures don't roll back earlier audible actions. Job history/receipts are bounded.

```sh
pd jobs list
pd automation stop
```

Only run stop on requested takeover or when abandoning automation you started. `jobs cancel` prevents future batch steps and interrupts engine waits; it is not universal cancellation of an active load/import/UI request or provider operation. `automation stop` cancels jobs, native ramps, pending launches and cue/jog holds; it does not pause both decks, stop recording, restore prior knobs, or prove every async operation ended. Inspect jobs, source jobs, pending launches and active ramps afterward; `deck.load.cancel` and `spotify.analysis.cancel` are explicit operation-specific capabilities. Manual movement of a ramped knob takes over that target. Yield on unexplained manual control/source/position changes; do not fight the user or blindly resume.

Native ramps run on rendered frames with fixed storage; audio must run for them to advance. Beat clock ramps use the clock's declared BPM/grid at start, cancel on clock pause/seek/source change and freeze while audio rendering is stopped. Be conservative if BPM is changed mid-ramp. They change the specified engine control; do not assume they reproduce Smart Fader/Smart CFX compound behavior. Keep cross-command elapsed time in a verified common clock domain: persist a UTC baseline, or rendered-frame/sample-rate baseline while the same renderer runs continuously. Process-local monotonic readings may have different origins across separate exec sessions; verify before comparing them. Native `deck launch` queues a source-guarded start on the audio clock; ordinary unsynced play still starts immediately. Shell polling/sleeps can approximate an intervention window but cannot promise exact beat/downbeat/phrase starts. Quantization applies to ramp start on the declared grid, not the separate play command.

## Diagnose before retrying

Errors expose code/message/retryability and details such as hint, expected types/ranges, suggestions and corrected examples. Fix invalid syntax from the error/discovery, don't guess commands. After source/cue conflicts, re-read identity and revision. After a timeout, inspect state/job before retrying: the operation may already have applied. Reuse the same request ID for an uncertain identical mutation; the runtime keeps 256 receipts and rejects reused IDs with different arguments. Don't repeatedly retry non-idempotent play/cue/toggle operations with new IDs.

Recording requires running audio and eligible loaded sources. Source policy remains authoritative. Existing output files are protected; replacement requires `--overwrite`. Verify writer final status (`active`, `frames_written`, `dropped_frames`, `error`) and the output file. XML migration changes library/cue metadata, not audio PCM files. Live Spotify/hardware verification requires the corresponding account/device and user authorization.

Source reference: [repository CLI guide](../../../research/cli-guide.md), current `commands list/describe`, and `src-tauri/src/control.rs`. When installed outside the checkout, return to the repository for these source references.


## Correct a grid and queue a musical launch

Full Beat This! `final0` inference uses a native BSD madmom-style DBN/HMM decoder by default, comparing whole-track hypotheses of three, four, five or seven beats per bar over 55–300 BPM. This imposes temporal/bar continuity; it does not infer arbitrary meter changes or phrases. In neural mode, the engine uses decoded local timestamps rather than a mean-BPM lattice. Inspect `metadata.beatgrid_source`, `rhythm_error` and bounded events. Effective transport/deck inspection flags are authoritative: `beatgrid_valid` separates a usable grid from an unknown zero fallback; `beatgrid_downbeat` can reflect predicted downbeats or a manual origin; `beatgrid_manual` distinguishes the origin edit; `beatgrid_uniform` identifies a constant-tempo override. Read native `beat_position` for phase in either mode. Scores are uncalibrated and do not establish musical correctness or rekordbox parity. Failed analysis requires manual validation before automated beatmixing.

The UI's **Set bar 1 here** and the following CLI setter use the same native `set_beatgrid` operation. Prefer a paused, isolated incoming deck at a listened/user-confirmed downbeat; do not manufacture bar one from a quiet waveform bin. Model-predicted bar starts are useful evidence but still need passage-level validation.

After reading the actual deck source and revision, set its grid origin with `pd deck B grid set --offset SECONDS --expected-load-revision REV`. Set a corrected base tempo separately with `pd deck B bpm set BPM --expected-load-revision REV` when needed. Grid edits preserve transport position, playback and cue markers; they persist per source and survive reload/reanalysis. Reinspect `status` and `track.inspect` to check the effective offset/provenance and unchanged transport. This changes musical grid metadata, so apply it when grid correction is requested or explicitly authorized, not merely during an inspection request. The UI also offers 1 ms draft nudges and half/double BPM; Apply grid commits the draft.

An explicit BPM correction switches the whole source to a constant-tempo grid, including half/double edits; it is not merely a display change or a rescaling of the original event list. Offset-only **Set bar 1 here** preserves neural intervals when the neural grid is active and leaves an existing uniform mode uniform. Manual bar-origin numbering retains detected meter, falling back to four if unknown. The UI sends a BPM edit only when its draft changed by more than 0.01 BPM. Loaded `beatgrid_uniform`/effective events take precedence over original neural provenance; an unloaded source with `bpm_source:manual` is inferred to have a uniform override. Rate/tempo setters are for beatmatching without changing this grid mode.

For an authorized refresh of existing analysis, use `pd deck B analyze --expected-load-revision REV`, wait its completion/job and reinspect `rhythm_error`, model/provenance, actual events and source revision. This invokes full model inference; an inspection-only request does not request reanalysis. Prefer preparation while the incoming deck is isolated and allow for measured analysis latency. Resetting a warning is not proof of a musically correct grid.

For a prepared incoming deck at its chosen entry, `pd deck B launch --clock A --clock-revision A_REV --at A_SOURCE_SECONDS --expected-load-revision B_REV` queues the start at an explicit future beat timestamp in A's source timeline. Replace every placeholder from observation. Choose the exact outgoing `seconds` from effective `beat_events`; a timestamp inferred from mean BPM can miss a variable grid and be rejected. Omit `--at` to use the next boundary; optional `--quantize 4` or `32` counts clock-grid beats from its origin, and only denotes bars/phrases when that origin and grouping were established musically. B's selected entry is snapped to its nearest grid beat: verify that position is still the intended musical entry. B must be paused, A playing; both need valid grids, effective tempos matching within 0.01 BPM, no jog holds/loops, ready incoming pitch preparation, running audio and sufficient source time. Matching average tempo is only a launch precondition; verify phase and local drift during the blend.

Acknowledgment means queued, not played: inspect `launch_pending` and `launch_at_seconds`, use `pd call engine.wait --args '{"deck":"B","launches":true}' --expected-load-revision B_REV --timeout 30s`, then verify B is playing with the same source/revision and a small difference between both native `beat_position` values before opening its fader. A cancelled launch also becomes nonpending, so wait completion alone does not prove playback. Native scheduling runs within the output sample clock; CLI/model latency cannot move an already queued boundary. A missed explicit timestamp, clock/source/position/tempo/grid change or manual takeover cancels the launch instead of starting stale audio. `pd deck B launch cancel` cancels only B's pending launch; `automation stop` cancels pending launches and ramps/jobs, leaving existing playback running. Clicking a queued UI Play button cancels it.

When SYNC is enabled and the master is playing, UI, keyboard and CLI Play use the next native beat boundary. This aligns beats; it does not choose a musical phrase or identify vocals. An unsynced Play command remains immediate. For phrase mixing choose the entry and arrival event first, count backward, and queue that specific future clock timestamp while the channel stays isolated.

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

The range example requires a loaded track lasting at least 24 seconds. `track.inspect` takes exactly one of `source` or `deck`, optional `start_seconds`/`end_seconds` and `points` 1..512 (default 64). Result: `source` canonical identity, optional `load_revision` for deck queries, `metadata` without waveform arrays, `cues:{revision,cues}`, requested range, original `raw_bin_seconds`/`raw_bins`, band availability and timestamped `bins`. Each bin covers `[start_seconds,end_seconds)` from the original stored bin grid: requested boundaries can be straddled, and requesting more points cannot invent finer resolution. Peaks are maxima; low/mid/high are RMS aggregation of existing shared-scale band RMS values. Missing bands are null. This is a bounded read of existing analysis, not a section detector. `engine.metadata` and `library.track` retain original arrays if full detail is needed.

## Apply and verify

Human CLI decks A/B, hot cues A-H, samples 1-16; JSON controller decks/pads/samples are zero-based. Top-level native deck accepts A/B or 0/1. Cue source revision and deck load revision protect different things: `cues.set.args.expected_revision` guards saved markers, `--expected-load-revision N` guards deck source identity. Neither guards manual fader/tempo/position changes; re-observe those between phases.

Responses have `ok`, `request_id`, `stage`, optional `result` and structured `error`. A success with no result is valid. Inspect stderr/exit code AND JSON. Stages: `accepted` queued/job started; `applied` command fence published; `ready` source load complete; `persisted` cue metadata committed; `rendered` UI acknowledgment. `applied` is not proof that 8ms audio control smoothing has settled or pitch/key-lock preparation has completed. `deck B wait` waits preparation; `call engine.wait --args '{"ramps":true}'` waits all active ramps. A native cue-edit response may be `applied` with `cue_revision` even when persistence succeeded; verify `cues.get` explicitly.

Ordinary seek preserves play state, but hotcue trigger starts playback. Explicit-time cue edits preserve playhead. Preserve existing markers and user labels unless editing is requested. A load refuses playing replacement unless `--replace-playing` is explicit; do not use that flag for normal next-track preparation. Loading itself is not a CAS against every possible concurrent manual change.

## Batches, timing and takeover

Batch files contain an array of full request objects (`command`, `args`, optional `expected_load_revision`). Validate before run. Batch returns accepted `job_id`; `jobs get` inspects even a failed job, whereas `jobs wait` exits 5 for failed, 7 for cancelled, 6 if its wait times out. Inspect executed step results and final native state; failures don't roll back earlier audible actions. Job history/receipts are bounded.

```sh
pd jobs list
pd automation stop
```

Only run stop on requested takeover or when abandoning automation you started. `jobs cancel` prevents future batch steps and interrupts engine waits; it is not universal cancellation of an active load/import/UI request or provider operation. `automation stop` cancels jobs, cancels native ramps and releases cue/jog holds; it does not pause both decks, stop recording, restore all prior knobs, or prove every async operation ended. Inspect jobs, source jobs and active ramps afterward; `deck.load.cancel` and `spotify.analysis.cancel` are explicit operation-specific capabilities. Manual movement of a ramped knob takes over that target. Yield on unexplained manual control/source/position changes; do not fight the user or blindly resume.

Native ramps run on rendered frames with fixed storage; audio must run for them to advance. Beat clock ramps use the clock's declared BPM/grid at start, cancel on clock pause/seek/source change and freeze while audio rendering is stopped. Be conservative if BPM is changed mid-ramp. They change the specified engine control; do not assume they reproduce Smart Fader/Smart CFX compound behavior. Keep cross-command elapsed time in a verified common clock domain: persist a UTC baseline, or rendered-frame/sample-rate baseline while the same renderer runs continuously. Process-local monotonic readings may have different origins across separate exec sessions; verify before comparing them. Native scheduled transport is unsupported. Shell polling/sleeps can approximate an intervention window but cannot promise exact beat/downbeat/phrase starts. Quantization applies to ramp start on the declared grid, not the separate play command.

## Diagnose before retrying

Errors expose code/message/retryability and details such as hint, expected types/ranges, suggestions and corrected examples. Fix invalid syntax from the error/discovery, don't guess commands. After source/cue conflicts, re-read identity and revision. After a timeout, inspect state/job before retrying: the operation may already have applied. Reuse the same request ID for an uncertain identical mutation; the runtime keeps 256 receipts and rejects reused IDs with different arguments. Don't repeatedly retry non-idempotent play/cue/toggle operations with new IDs.

Recording requires running audio and eligible loaded sources. Source policy remains authoritative. Existing output files are protected; replacement requires `--overwrite`. Verify writer final status (`active`, `frames_written`, `dropped_frames`, `error`) and the output file. XML migration changes library/cue metadata, not audio PCM files. Live Spotify/hardware verification requires the corresponding account/device and user authorization.

Source reference: [repository CLI guide](../../../research/cli-guide.md), current `commands list/describe`, and `src-tauri/src/control.rs`. When installed outside the checkout, return to the repository for these source references.

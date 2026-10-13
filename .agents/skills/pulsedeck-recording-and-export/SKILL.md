---
name: pulsedeck-recording-and-export
description: Record eligible PulseDeck mixes to explicit WAV paths, finalize and verify recordings, and import or export local library and cue metadata through rekordbox XML. Use for capture and migration requests; use transition/session skills to choose or perform the mix.
---

# Recording and export

Use the existing CLI. Capture the requested performance without changing its musical strategy, mixer balance, routing, or source eligibility. Planning-only requests stay read-only. An authorized recording or migration request authorizes its necessary steps; do not ask again for each command.

Read [shared CLI reference](../references/cli.md) for the `pd` shell function, endpoint binding, completion stages, errors, source guards, and cancellation. Read [musical evidence](../references/musical-evidence.md) only when interpreting a track or explaining the recorded mix. Never claim listening from metadata or a silent trial.

## Observe before writing

Bind the requested runtime and discover current capabilities:

```sh
pd status
pd commands list
pd commands describe recording.start
pd commands describe recording.stop
pd commands describe recording.status
pd commands describe library.import_xml
pd commands describe library.export_xml
pd recording status
```

Record the instance/socket, `metadata.tracks` identities and `recording_allowed`, `engine.load_revisions`, audio running state/device/sample rate/routing, current recording destination/status, and requested absolute output path. Check the destination's parent exists and is writable, available space is plausible for stereo float PCM (roughly sample_rate × 8 bytes × seconds plus header), and whether the file exists. A `.wav` extension describes this writer's output; it does not transcode the loaded files.

Both loaded decks matter: a source with `recording_allowed:false` prevents recording even if paused, muted, or behind a closed fader. Loading such a source during capture can end the capture. Preserve the restriction. Do not change source policy, disguise a service source, record through a workaround, or unload the user's deck merely to pass eligibility. Explain which loaded source blocks capture and offer an eligible local-file session when requested. The runtime check remains authoritative, including if the source changes after observation.

Audio must run to generate recorded frames. If it is stopped, use the setup skill to select the requested device/routing before starting it; discovery is `pd audio devices`. Starting audio is not permission to raise gains, reroute headphones, or alter the live mix. Zero master gain produces a silent master recording; headphone cue gain/routing is not a way to create a separate headphone recording.

## Record and finalize

Use an explicit user-selected destination. These examples use paths chosen for an isolated practice session; substitute the requested absolute paths and bind the correct endpoint first:

```sh
pd recording start --path /tmp/pulsedeck-practice.wav
pd recording status
# Run only when capture should finish, not immediately after starting it.
pd recording stop
pd recording status
```

A start response does not prove useful audio was captured. Verify `active:true` and matching `path`, then inspect later that `frames_written` increases while audio runs. Monitor `error`, `dropped_frames`, source eligibility and unexpected source changes during a longer capture. Recording counts rendered output frames, not musical quality or track playback duration. Paused/silent transport can still produce frames of silence.

Stop joins/finalizes the writer. Verify `active:false`, the intended `path`, `error:null`, and a plausible positive `frames_written`; report `dropped_frames` exactly. Duration is `frames_written / sample_rate`. With no frames or an error, report an empty/failed capture; with drops, report an imperfect capture even if the WAV finalized. Inspect the file independently: existence and nontrivial size, readable RIFF/WAVE header, stereo 32-bit IEEE float format, complete data frames and header/data size consistency. A file can be structurally correct and musically silent. Audible balance, clipping perception, timbre, routing and artifacts require listening/hardware verification.

Existing destinations are protected. If replacement is explicitly requested:

```sh
pd recording start --path /tmp/pulsedeck-practice.wav --overwrite
```

`--overwrite` truncates the existing file; do not use it to resolve a collision without authorization. Choose a new requested filename instead. If a timeout makes start/stop uncertain, inspect `recording status` before retrying; never start a second file blindly. An already active recording needs an explicit stop, not a new start.

## Local library and cue migration

XML migration writes library/cue metadata and references local paths; it does not edit, copy, retag, or transcode audio PCM files. Export covers the local library and playlists, not a service playlist write or a recording's audio. Read the requested XML and current library first when deciding whether an import is appropriate; check referenced file URLs resolve locally.

```sh
pd library tracks --limit 100 --offset 0
pd playlist list
pd library export-rekordbox --path /tmp/pulsedeck-library.xml
pd library import-rekordbox --path /tmp/pulsedeck-library.xml
pd status
```

These export/import examples are separate workflows: do not import an export back into a performance just because both commands are shown. Export defaults to protecting existing output. Replacement requires the user's intent:

```sh
pd library export-rekordbox --path /tmp/pulsedeck-library.xml --overwrite
```

Verify the exported file parses as `DJ_PLAYLISTS`, track paths and record counts match the intended library, ordered playlist membership is represented, and requested hot/memory cue timestamps are present. Export currently uses generic cue names rather than source annotation labels; its tempo marker writes `Metro="4/4"` and `Battito="1"`. This serialization does not establish actual meter/downbeat, and it is not a full annotation round trip. Main cue is not a separate exported position marker.

Import is not a transactional replace/merge with full fidelity: successful records can coexist with failures, matching file records receive XML metadata/cue arrays, and playlist nodes can create additional playlists. Missing XML markers can clear library hot/memory cue fields. Back up/export current library metadata before an authorized migration where existing cues matter; a repeat import is not automatically harmless. Do not overwrite unrelated cues simply to force the trial's expected result.

An `ok:true` import returns successful track records, not proof every XML entry imported. Immediately inspect `status.result.analysis` (`total`, `completed`, `errors`, `active`) and compare returned records with XML collection entries. There is no separate import-status method. Identify skipped/missing paths and retained successes; do not call the result complete or roll back successful imports unless requested. If a job was used, inspect its final step results and the same analysis errors.

Verify imported BPM, grid offset, hot/memory timestamps and playlist order with `library.track`/`playlist.list`. For each relevant discovered local ID, also verify the saved source cues:

```sh
# local:1 / track 1 are illustrative; substitute the imported record's id.
pd library track get 1
pd call cues.get --args '{"source":"local:1"}'
```

Current boundary demonstrated in the trial: XML changes a library record's markers while an existing source-backed `cues.get` row can retain older markers. Export reads library fields, whereas later cue operations/observers can use source-backed fields. Loaded deck state is another observation, not proof of import synchronization. Report any discrepancy instead of claiming complete cue migration. If the user asks to reconcile, apply only the intended markers through `cues.set` with the freshly read `expected_revision`, preserve other markers/labels, then re-read both representations and the loaded deck. Do not automatically pause/unload a live deck to perform migration; prefer an idle session or an explicitly authorized handover when reloading is necessary.

## Failure and manual takeover

Inspect structured errors and current state before targeted retries. `file_exists` means choose a new destination or use authorized overwrite; `audio_not_running` means configure/start the requested audio session; eligibility refusal is a source restriction. Missing/unwritable parents require correcting the explicit path. Disk/write errors and dropped frames require reporting actual capture integrity, not calling a finalized header success.

`jobs cancel` may stop future steps while an active import/load/provider operation continues. `automation stop` cancels DJ jobs/ramps and releases holds; it does **not** stop recording. On “stop automation; I'm taking over,” yield controls and report whether capture remains active. On “stop recording,” call recording stop and verify finalization. Do not stop the user's capture just because a transition or job failed. After manual source/control changes, re-observe and stop issuing automation mutations; continue only the specifically requested capture/finalization behavior.

## Expected response and examples

For “Can this mix be recorded?” report loaded sources/policies, audio/capture state, destination feasibility if supplied, and the proposed capture procedure. No start, overwrite, transport, routing or cue mutations.

For “Record my local-file practice mix to this WAV path,” verify eligibility and destination, start capture, verify active/path, and report capture started. Let the transition/session skill or user perform the mix; do not invent a playback sequence. For “Stop recording and verify it finished,” report final path, actual duration/frames, drops/error, WAV integrity, and what has not been audibly checked.

For “Export my updated library and cues to rekordbox XML,” export to the specified path, verify XML records/paths/markers/playlists, and disclose annotation/meter limitations. For import, report successful/failed entry counts and metadata fidelity checks, including source/library cue discrepancies. Separate planned, applied, still active, finalized, partially imported, and incomplete actions explicitly.

The isolated generated-fixture trial exercised actual command discovery, two eligible loaded tracks, finalized silent float WAV recording, existing-file protection/explicit overwrite, XML export/import, a missing-file partial import, unchanged source WAV hashes, and the source/library cue synchronization boundary. See `artifacts/skill-trials/recording-and-export.json` in the checkout for evidence; it does not establish audible quality, physical routing, or live service-policy behavior.

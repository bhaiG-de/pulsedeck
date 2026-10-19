# Record and verify a WAV mix

Capture the requested performance without changing its musical strategy, routing, levels or source eligibility. XML metadata migration belongs to `$pulsedeck-library`. A request to assess recordability stays read-only; a recording request authorizes capture, and a bounded requested capture includes finalization at its endpoint.

## Check before starting

Read status and `recording status`; discover unfamiliar recording methods. Record both loaded source identities, load revisions and `recording_allowed`, actual renderer state/device/sample rate/routing, any active writer and the explicit absolute destination. Check parent existence/writability, plausible space and file collision. Stereo float PCM requires approximately `sample_rate * 8 * seconds` bytes plus header.

Both loaded decks matter. A source with `recording_allowed:false` blocks capture even while paused/muted. Loading a restricted source during capture can end it. Preserve runtime restrictions; do not disguise a source, unload the user's deck solely to bypass eligibility or capture through another route. Explain the actual blocker and use eligible material only within the requested session scope.

Rendering must run to produce frames. If it is stopped, use the authorized setup workflow for the intended device/routing. Do not change gain or headphones to force capture. Zero master gain produces a silent master recording; cue routing does not create a separate headphone recording.

## Start, observe and finalize

```sh
pd recording start --path "$RECORDING_PATH"
pd recording status
# Run at the requested endpoint, not immediately after start.
pd recording stop
pd recording status
```

Set `RECORDING_PATH` to the requested absolute destination. Verify start has matching path and `active:true`; verify later that `frames_written` grows. During longer capture monitor error, dropped frames, source eligibility and unexpected source changes. Rendered silence also counts as frames; a positive count does not establish music was captured.

Stop joins/finalizes the writer. Verify `active:false`, expected path, `error:null`, positive plausible frame count and exact `dropped_frames`. Duration is `frames_written / sample_rate`. Empty/error captures are incomplete; drops make capture imperfect even if finalized.

Inspect the resulting file independently: existence/nontrivial size, readable RIFF/WAVE, stereo 32-bit IEEE float, complete frames and consistent header/data sizes. Python's standard `wave` module may reject float WAV despite a valid writer; use an appropriate RIFF parser or float-capable library. Structural correctness cannot establish audible balance, musical content or sound quality.

Existing files are protected. `--overwrite` truncates and requires replacement intent; do not resolve a collision by adding it silently. If no destination was specified, obtain the output-path preference before capture while independently checking eligibility. An active writer must be finalized before a different capture begins.

On uncertain start/stop, inspect status before retrying; do not blindly start a second file. Report disk/write errors and drops. An automation or transition failure is not automatically permission to stop the user's ongoing capture.

## Stop semantics

`automation stop` and session `op:stop` do not finalize the writer. “Stop recording” requires `recording stop` and file verification. For control handover, report whether recording remains active. If the capture was explicitly bounded by the set endpoint, finalize it there; otherwise respect the independent capture request.

Return destination, started/active versus finalized status, measured duration/frames, drops/error, file integrity and any material unverified listening result.

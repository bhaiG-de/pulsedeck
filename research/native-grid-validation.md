# Native grid validation — 14 October 2026

The packaged debug app, with the full Beat This! final0 model and default 55–300 BPM DBN decoder, passed a muted two-deck integration trial. Generated fixtures have known source tempos but do not supply human musical downbeat annotations; this tests control behavior, not detector accuracy.

Checks passed: bundled model inference (analysis v5); bounded actual beat/downbeat inspection; grid edits preserve transport and cues; sample-clock queued launch; completion wait; cancellation when the clock is sought; manual grid persistence across reload and reanalysis. The same-snapshot phase difference after launching was −0.00194 beat (about 0.97 ms at 120 BPM). This is phase measured against detected grids, not timing accuracy against recorded transients. Native unit tests separately check the exact launch sample at 44.1/48/96 kHz and nonuniform grids.

Visual and native UI checks passed: grid editor exposes bar-one-at-playhead, ±1 ms origin adjustment and Apply; adjusting 0.125 to 0.126 seconds left the paused playhead at zero, retained model timing (`beatgrid_uniform:false`) and persisted a manual origin. Layout was inspected in the native app.

The render allocation audit passed while loading, replacing and retiring neural grids; the audio callback allocates and frees no assets. Reanalysis remaps saved anchors by original source time when the decoder changes event counts. Manual half/double tempo overrides have a complete constant-tempo grid across the track, with separate unit and frontend regression tests.

Detailed compact results are in [native-grid-validation.json](native-grid-validation.json). The inference/decoder comparisons and accuracy limits are in [beat-detector-validation.md](beat-detector-validation.md). All validation was muted; no listening or attached-controller test is claimed.

Additional full-source Spotify checks and the continuous-sync regression/fix are documented in [spotify-grid-validation.md](spotify-grid-validation.md).

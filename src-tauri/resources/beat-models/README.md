# Pinned Beat This! inference assets

Install once from the repository root:

```sh
python3 tools/ensure-beat-models.py
python3 tools/ensure-beat-models.py --check
```

The installer reads `manifest.json` and verifies both file size and SHA-256 before
replacing any asset. The full FP32 `final0` model is about 83 MB. ONNX files are
ignored by Git; license notices and the provenance manifest remain tracked.
Run the installer before packaging the app so its configured resource directory
contains both model files. These assets are not downloaded during playback.

`PULSEDECK_BEAT_MODEL_DIR` overrides the asset directory. The development fallback
is this folder, resolved from the audio-analysis crate. The packaged app must set
the override to its resolved resource folder before initializing neural analysis.
`PULSEDECK_BEAT_THREADS` sets a private RTen model pool (1–8 threads, default 2).
It does not change process-global Rayon/RTen configuration. Loading and inference
must run on background workers; successful loading is retained and inference is
serialized across decks. Missing assets are retryable after installation.

Audio stays in memory: decoded stereo PCM is downmixed into 8192-sample chunks,
resampled to 22,050 Hz by the port, then processed as log-mel frames. No provider
PCM is written to files. This wrapper accepts 3-second to 2-hour source durations;
streaming bounds extra PCM allocation, while mel/event storage scales with time.

The upstream [Beat This! project](https://github.com/CPJKU/beat_this#license)
explicitly releases its code **and published model weights** under MIT. The Rust
port is also MIT. Both notices are included here. Full model weights are the
port's `model-large` release export of upstream `final0`; the mel model is fetched
from the exact port commit recorded in the manifest. Hash verification makes even
a changed release asset fail instead of silently changing the detector.

Predicted beat/downbeat events retain their individual source timestamps and can
reflect varying tempo. Default postprocessing uses a BSD madmom-style DBN with
competing **whole-track 3/4/5/7 meter hypotheses**, 55–300 BPM, 50 fps, 60 tempi,
transition lambda 100, observation lambda 16, threshold 0.05 and correction on.
It constrains tempo and meter, so unusual meter or faster music may need manual
review; it does not infer changing meter. The port-specific ±1-frame energy search is disabled. PulseDeck now refines
concave beat-logit maxima by at most half a frame (10 ms), with downbeats sharing
the same adjusted beat event; the labeled validation reports its measured effect. No madmom BLSTM model weights are included or used.
See [decoder provenance and memory limits](../../../crates/audio-analysis/src/dbn/README.md).
The model front end runs at 50 Hz (20 ms frames). This is neither sample-resolution
inference nor proof of parity with rekordbox. Confidence fields remain separate
mean sigmoid activations around selected event peaks, not calibrated accuracy.
`PULSEDECK_BEAT_POSTPROCESS=minimal` bypasses DBN only for benchmark/reference
comparisons; its returned model identifier explicitly says `minimal`.
Displayed BPM summarizes the median source-time slope across sliding 32-beat
windows (16/8/4/1 for short tracks). This reduces single-interval frame-bin tempo
bias without replacing the refined event timeline with a uniform lattice.

After installation, a small opt-in full-model smoke test can be run with:

```sh
bash tools/rust-env.sh cargo test -p audio-analysis --release neural::tests::full_model_inference_smoke -- --ignored --nocapture
```

This verifies the inference path on synthetic rhythm; accuracy requires annotated
held-out real music, including syncopation, pickups, weak percussion and tempo
changes. The full model is intentionally used instead of the port's smaller
`small1` model. There is no spectral fallback inside `analyze_neural_rhythm`.

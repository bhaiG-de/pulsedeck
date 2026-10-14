# Beat This! spike

Feasibility code behind [research/beat-model-spike.md](../../research/beat-model-spike.md): export the Beat This! small model to ONNX, run it from Rust with tract, and compare every stage against the Python reference. This is spike code, not part of the app build. The Rust front end, chunking, peak picking and grid fit in `rust/src/main.rs` are the starting point for the feature 1 build in `crates/audio-analysis`.

| File | Purpose |
| --- | --- |
| `reference.py` | Exports `models/beat_this_small0.onnx` and writes the Python reference data for each track to `ref/` |
| `variants.py` | Exports the grouped-attention model (`export 1`) and times ONNX Runtime CPU and CoreML (`ort-all`) |
| `weights_f16.py` | Stores the weights as float16 with float32 compute |
| `kick_alignment.py` | Checks where kick-drum onsets fall relative to the model grid and the current DSP grid |
| `rust/` | Standalone Rust crate (its own workspace): decoder, resampler, log-mel, tract inference, peaks, grid fit and comparisons |

`models/`, `ref/`, `venv/`, `beat_this/` and `results*.json` are ignored. `ref/` holds decoded audio derived from your own tracks, and the results contain local paths; keep both off the repository.

## Setup

Python 3.12 and Rust are required. Run everything from this folder. The Python environment takes about 900 MB.

```bash
python3.12 -m venv venv
```

```bash
venv/bin/pip install torch torchaudio einops rotary-embedding-torch soxr numpy onnx onnxruntime onnxscript soundfile
```

```bash
git clone --depth 1 https://github.com/CPJKU/beat_this.git && venv/bin/pip install -e ./beat_this
```

Download the MIT-licensed small checkpoint (about 8 MB) and check its hash:

```bash
mkdir -p models && curl -L -o models/small0.ckpt https://cloud.cp.jku.at/public.php/dav/files/7ik4RrBKTS273gp/small0.ckpt
```

```bash
shasum -a 256 models/small0.ckpt
```

The expected hash is `6074be2c4d490c5f6101fcc374a1ec72ae93456e23bb6019783b849f5dc7d47b`.

## Run

1. Reference data and the plain export (pass any number of audio files):

   ```bash
   venv/bin/python reference.py ~/Music/track1.mp3 ~/Music/track2.mp3
   ```

2. The grouped-attention model and its float16-weight copy:

   ```bash
   venv/bin/python variants.py export 1 && venv/bin/python weights_f16.py
   ```

3. Build and run the Rust comparison with the spike's final configuration:

   ```bash
   cd rust && cargo build --release && cd ..
   ```

   ```bash
   SPIKE_GAPLESS=1 SPIKE_MODEL=beat_this_small0_g1_w16.onnx SPIKE_THREADS=4 rust/target/release/beat-model-spike > results.json
   ```

4. Kick-onset alignment for the same tracks:

   ```bash
   venv/bin/python kick_alignment.py results.json
   ```

`SPIKE_GAPLESS=1` trims MP3 encoder delay so the timeline matches the Python decoder; leave it unset to use the app's own timeline. The other switches (`SPIKE_ROOT`, `SPIKE_F16`, `SPIKE_ONLY`, `SPIKE_RS_CHUNK`, `SPIKE_MODE=infer-only`) are listed at the top of `rust/src/main.rs`. To measure peak memory, prefix a run with `/usr/bin/time -l`.

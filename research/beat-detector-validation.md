# Beat detector validation — analysis version 5 — 14 October 2026

The default pipeline now uses the full Beat This! final0 model followed by a
BSD madmom-style temporal/meter decoder. The minimal peak-picker measurements
below remain as historical results, separately labeled in the JSON.

## Current DBN default

PulseDeck uses competing **whole-track 3/4 meter hypotheses**, 55–300 BPM,
50 fps, 60 tempo candidates, transition lambda 100, observation lambda 16,
threshold 0.05 and corrected event peaks. It does not force event times into a
uniform lattice or infer arbitrary changing meter. Port-specific extra
±1-frame/parabolic refinement is disabled. Only decoder source/support types
and BSD notices are vendored; no madmom feature extractor or neural weights
are used. The model identifier explicitly says `+ DBN 3/4 55-300 BPM`.

| Clip | Published BPM | Historical minimal beats / downbeats | DBN beats / downbeats | DBN BPM | NN + DBN | DBN only |
|---|---:|---:|---:|---:|---:|---:|
| GiantSteps 1030011 | 127 | 252 / 106 | 252 / 63 | 125.984 | 4.618 s | 0.231 s |
| GiantSteps 1068430 | 174 | 361 / 91 | 350 / 88 | 174.863 | 4.628 s | 0.237 s |
| GiantSteps 5377710 | 129 | 253 / 66 | 256 / 64 | 128.000 | 4.630 s | 0.231 s |
| Swing port sample | No independent label | 575 / 144 | 577 / 145 | 227.488 | 5.553 s | 0.318 s |

All successive DBN downbeats are four selected beats apart in these four
examples. This resolves the excessive minimal downbeat count observed on the
first EDM clip. Counts and regularity alone do **not** establish which musical
bar is correct: the EDM references contain BPM labels, not annotated beats or
downbeats. The same-model swing reference is also not human ground truth.
Model peak scores remain separate, uncalibrated responses.

### Tempo-range comparison

Upstream Beat This! DBN uses 55–215 BPM. On the fast swing example this range
selected a third-time interpretation: 75.949 BPM, 192 beats and 48 downbeats.
Expanding to 55–300 BPM recovered 227.488 BPM, 577 beats and 145 downbeats. All
three EDM event arrays are exactly identical under the 215 and 300 caps.
PulseDeck therefore uses **55–300 BPM** by default, a documented DJ-specific
policy change rather than claiming identical upstream settings. Music outside
this range or outside 3/4 meter needs review/manual correction.

### Independent implementation on identical model activations

Saved exact Rust model logits were decoded by Python madmom **0.16.1** in an
isolated Python 3.11 environment, with the same 55–300 DBN configuration and
Beat This!'s f64 sigmoid/epsilon adapter. Rust narrows to the port's f32
activation/state representation. Across all four examples, every beat and
every downbeat matched, with equal counts: precision/recall/F1 are **100% at
70, 20 and 10 ms**. Maximum timestamp deviation was **0.0037 ms** for the
120-second clips and **0.0073 ms** for swing, caused by f32 event timestamps.
Python with the original 215 cap also returned swing's 192 / 48 events,
confirming that interpretation came from tempo policy, not the Rust port.

This isolates decoder parity from model/audio/resampling differences. It is
not independent learned-detector accuracy, a large held-out benchmark, or
comparison to rekordbox. Compatibility shims for old collections/NumPy aliases
and explicit object-dtype result aggregation affect only the isolated Python
oracle, not its Viterbi/transition/correction algorithms.

### Memory and testing

256-frame Viterbi traceback blocks bound the largest default backpointer buffer
to about **6.1 MB** (5,980 four-beat states). Boundary score checkpoints and
final path/density storage still grow with duration (~1.15 MB of four-beat score
checkpoints per 120 seconds; ~67 MB for two hours). Whole-track path recovery
uses a forward pass plus exact per-block replay; blocks do not reset musical
phase independently. The entire operation stays off the audio callback and is
serialized with neural inference.

Regression fixtures cover regular three/four-beat meters, excessive weak
raw downbeat peaks, changing tempo, 228 BPM fast music, silence, invalid logits,
source-bounded sorted events and retained nonuniform decoder timing. Analysis
version 5 invalidates cached minimal results. Manual corrections remain a
separate engine/UI concern.

### Reproduce the DBN comparison

`benchmark_rhythm` optionally exports **derived logits only**, without source
PCM. `compare_dbn` reuses those exact activations at different tempo limits:

```sh
bash tools/rust-env.sh cargo build -p audio-analysis --release --examples
target/release/examples/benchmark_rhythm /tmp/pulsedeck-1030011.LOFI.mp3 \
  --tempo 127 --activations /tmp/1030011-logits.json
target/release/examples/compare_dbn /tmp/1030011-logits.json 300 120
```

The Python reference is opt-in validation tooling, not an app dependency:

```sh
uv venv /tmp/pulsedeck-dbn-reference --python 3.11
uv pip install --python /tmp/pulsedeck-dbn-reference/bin/python \
  numpy==1.26.4 scipy==1.15.3 Cython==0.29.37 mido==1.3.3 'setuptools<81' wheel
uv pip install --python /tmp/pulsedeck-dbn-reference/bin/python \
  --no-build-isolation madmom==0.16.1
/tmp/pulsedeck-dbn-reference/bin/python \
  crates/audio-analysis/examples/reference_dbn.py \
  /tmp/1030011-logits.json /tmp/1030011-python.json --max-bpm 300
```

These tools never use madmom BLSTM model weights; any package assets in the
isolated oracle environment are not redistributed with PulseDeck. The decoder
source revision, original checksum, complete author/upstream BSD notices and
configuration are recorded in the model manifest and vendored decoder folder.

---

# Historical minimal-postprocessing validation — 14 October 2026

The full-model integration passed same-model Python event parity at ±70 ms on
one real music example. On three tempo-labeled electronic previews, the model
selected the published tempo octave in all three cases; the older spectral
detector returned half tempo in two. These are exploratory integration checks,
not proof of beat/downbeat accuracy on unseen music or parity with rekordbox.

Measured on an Apple M5 Pro, arm64, Rust 1.99.0, optimized release, private RTen
pool of two threads, ffmpeg 9.0.2. Each example invocation is a fresh process, so
its reported model wall time includes first loading and SHA-256 verification.
Audio decoding time is excluded. No provider/Spotify PCM was exported. All
downloaded music remains in `/tmp`, outside this repository.

## Real music paired with Python final0

The [pinned Rust port](https://github.com/danigb/beat-this-rs/tree/1ae768e78f1ad83b0ed3886241dc29ffde853c40)
provides a 155.716-second MP3, “It Don't Mean A Thing - Kings of Swing”, and
`tests/fixtures/golden_full.json` generated with upstream Python Beat This! 1.1.0,
commit `b95c8ab0c58c2d9fcfd40508ae8dffbc05ac4f5c`, full `final0`, minimal
postprocessing at 50 Hz. Both outputs contain **575 beats and 144 downbeats**.
The reference is another implementation of the same learned model, not human
beat annotations. Different MP3 decoders/resamplers can move a peak by a frame.

| Events | Strict tolerance | Matched / reference | Precision = recall = F1 | Matched mean / p95 / max error |
|---|---:|---:|---:|---:|
| Beats | 70 ms | 575 / 575 | 100.00% | 3.619 / 20.001 / 20.005 ms |
| Beats | 20 ms | 524 / 575 | 91.13% | 2.024 / 19.999 / 20.000 ms |
| Beats | 10 ms | 471 / 575 | 81.91% | 0.0017 / 0.0055 / 0.0073 ms |
| Downbeats | 70 ms | 144 / 144 | 100.00% | 3.612 / 20.001 / 20.004 ms |
| Downbeats | 20 ms | 132 / 144 | 91.67% | 2.122 / 20.000 / 20.000 ms |
| Downbeats | 10 ms | 118 / 144 | 81.94% | 0.0015 / 0.0055 / 0.0073 ms |

Metrics use sorted one-to-one matching. Primary scores use the stated strict
tolerance. At the 20 ms boundary, f32 timestamp conversion produces deviations
such as 20.005 ms. A separately labeled diagnostic allows **10 microseconds**
for this representation rounding: all 575 beats and all 144 downbeats then match
at 20 ms. This does not improve the 10 ms score or imply finer model resolution.
The 70 ms mean/p95/max characterize the complete matched set; the tighter-window
means exclude mismatches and must not be interpreted as overall timing accuracy.

First model beats are `0.30, 0.58, 0.86, 1.12, 1.38, 1.66` seconds; first
downbeats are `0.30, 1.38, 2.46, 3.56, 4.66, 5.76`. Updated inference takes
5.263 seconds for the 155.716-second source (~0.034 times playback duration).

## Tempo-labeled electronic previews

Three clips were chosen before inference: the official README's example
`5377710`, plus the first two lexically sorted nonzero v2 annotation filenames,
`1030011` and `1068430`. The
[official GiantSteps repository](https://github.com/GiantSteps/giantsteps-tempo-dataset/tree/d51ab2422e76abacfaa86616a57054bc222ec9fd)
provides v2 tempo labels and expected MD5s. Its current download script uses the
JKU backup. All three downloaded 120-second clips matched the official MD5s.
The historical Beatport URL for `5377710` returned 404; the official backup worked.

| GiantSteps clip | Published BPM | Spectral BPM | Neural BPM before | Neural BPM after | After error | Runtime |
|---|---:|---:|---:|---:|---:|---:|
| 1030011 | 127 | 126.00 | 125.000 | 125.984 | 0.800% | 4.322 s |
| 1068430 | 174 | 87.49 | 176.471 | 175.182 | 0.680% | 4.328 s |
| 5377710 | 129 | 64.00 | 130.434 | 128.000 | 0.775% | 4.323 s |

The model is within 1% of all three published labels. The old detector's 87.49
and 64.00 estimates land near the half-tempo octave. A BPM annotation provides
no beat/downbeat timestamp reference, so these checks cannot score placement or
identify which predicted downbeats are musically correct. The clips are a tiny
deterministic sample and should not be described as a representative held-out
accuracy benchmark. Published model training coverage must be checked before
any held-out claim.

`1030011` produced 252 beats and 106 downbeats; `1068430` produced 361 and 91;
`5377710` produced 253 and 66. The JSON records the observed numbers of beats
between successive downbeat predictions. Some are irregular; the implementation
preserves them instead of forcing four-beat bars. No listening/downbeat ground
truth check was performed. Model activation scores remain uncalibrated.

## Fix identified by this benchmark

A median of adjacent 20 ms quantized beat intervals can bias the displayed DJ
BPM and therefore tempo matching: at 174 BPM an interval quantized to 0.34 s
shows 176.47 BPM. The summary now takes the median of **32-beat sliding-window
source-time slopes**, with 16/8/4/1-beat fallbacks on short sources. This improves
the BPM summary while retaining every original event timestamp. All beat and
downbeat arrays are exactly unchanged in all four before/after measurements.
The swing example's summary changed from 230.767 to 227.488 BPM; it has no
independent published tempo label here.

Tests cover this quantization issue using 126/129/133/174 BPM event fixtures,
nonuniform timing retention, irregular bars, source boundaries, silence,
separate confidence, thread safety and non-reusing benchmark event matching.

## Reproduce

The example accepts arbitrary local audio, optional Python-style golden JSON
(`beats`, `downbeats`) and optional BPM label. ffmpeg decodes to memory only;
ffmpeg is a benchmark requirement, not an app inference dependency.

```sh
python3 tools/ensure-beat-models.py --check
bash tools/rust-env.sh cargo build -p audio-analysis --release --example benchmark_rhythm
git clone https://github.com/danigb/beat-this-rs.git /tmp/pulsedeck-beat-this-rs
git -C /tmp/pulsedeck-beat-this-rs checkout 1ae768e78f1ad83b0ed3886241dc29ffde853c40
PULSEDECK_BEAT_POSTPROCESS=minimal PULSEDECK_BEAT_THREADS=2 target/release/examples/benchmark_rhythm \
  "/tmp/pulsedeck-beat-this-rs/test_files/It Don't Mean A Thing - Kings of Swing.mp3" \
  --golden /tmp/pulsedeck-beat-this-rs/tests/fixtures/golden_full.json
```

For the three tempo previews, clone the official repository at the revision
above, fetch only the selected clips from
`https://www.cp.jku.at/datasets/giantsteps/backup/<id>.LOFI.mp3`, verify each
against `md5/<id>.LOFI.md5`, then run, for example:

```sh
target/release/examples/benchmark_rhythm /tmp/pulsedeck-1030011.LOFI.mp3 --tempo 127
```

Full provenance, audio SHA-256s, model hashes, first 12 predicted events, observed
bar-gap counts and all metrics are in [beat-detector-validation.json](beat-detector-validation.json).

# Beat This! feasibility spike

Run 14 October 2026 on an Apple M3 Pro. This is the go/no-go spike for feature 1 in [the AI features scope](ai-features-scope.md). It establishes that the model can run from Rust and reproduce the reference implementation, and it measures speed, memory and size. It does **not** establish accuracy against human annotations; the 32-track annotated evaluation remains the ship gate.

**Verdict: go.** Rust reproduces the Python reference, and the model's grid lines up with kick drums on all six test tracks. The costs are about 25 MB of app size (accepted) and about 1.8 s of background analysis per minute of audio. The code and setup steps are in [tools/beat-model-spike](../tools/beat-model-spike/README.md).

## Setup

| Item | Value |
| --- | --- |
| Model | Beat This! `small0` checkpoint, MIT code and weights. 2,099,960 parameters; transformer width 128, 6 layers. |
| Checkpoint | 8,451,101 bytes, SHA-256 `6074be2c4d490c5f6101fcc374a1ec72ae93456e23bb6019783b849f5dc7d47b` |
| Export | PyTorch 2.14.1 TorchScript exporter, ONNX opset 17, static input `[1, 1500, 128]` (one 30 s chunk). ONNX Runtime matches PyTorch within 1e-5. |
| Rust runner | tract 0.23.8 (pure Rust, Apple AMX kernels), `multithread-mm` feature |
| Front end | Independent Rust port: mono mix, rubato 5 FFT resampler to 22,050 Hz, centred reflect-padded STFT (1024-point periodic Hann, hop 441 = 50 fps), magnitude / sqrt(1024), 128 Slaney mel bands 30 to 11,000 Hz without area normalisation, ln(1 + 1000x) |
| Inference | Upstream chunking: 1500 frames, 6-frame borders, keep-first overlap, last chunk aligned to the end |
| Peaks | Upstream "minimal" post-processing: ±3-frame local maxima above logit 0, adjacent-peak merge, downbeats snapped to the nearest beat |

Test tracks: Detroit Swindle "The Wrap Around" (house), Kendrick Lamar "Prayer" (hip-hop), FILV "Coca Cola" (EDM), Fred again.. "Hannah (The Sun)" (electronic), an untitled melodic drill beat, and Mr. Belt & Wezol "Keep It Exciting (Extended Mix)" (house, long intro). All are 44.1 kHz MP3 files.

## Stage equivalence

| Stage | Result |
| --- | --- |
| Mel filterbank, Rust vs torchaudio | max abs error 1.3e-5 |
| Log-mel from the same 22,050 Hz signal | max abs error 1.2e-4 |
| tract vs PyTorch frame logits, same spectrogram | max abs error 1.5e-4 |

The Rust front end and tract inference are numerically equivalent to the reference. Remaining end-to-end differences come only from decoding and resampling.

## End to end vs the Python reference

Final configuration: grouped attention (below), float32 compute, 4 threads, single-block FFT resampler. For this comparison only, MP3 encoder delay is trimmed so both timelines start at the same sample (see side finding A).

| Track | Duration | Beats Rust / Python | Beat F ±20 ms | Beat F ±70 ms | Downbeat F ±20 ms | Model time |
| --- | --- | --- | --- | --- | --- | --- |
| Detroit Swindle | 7:16 | 875 / 875 | 0.96 | 1.00 | 0.95 | 12.1 s |
| Kendrick Lamar | 5:21 | 442 / 438 | 0.98 | 1.00 | 0.97 | 8.1 s |
| FILV | 4:11 | 462 / 461 | 0.94 | 1.00 | 0.95 | 6.0 s |
| Fred again.. | 3:17 | 401 / 402 | 0.97 | 1.00 | 0.95 | 8.1 s |
| Drill beat | 2:16 | 310 / 296 | 0.92 | 0.96 | 0.97 | 5.0 s |
| Mr. Belt & Wezol | 4:02 | 499 / 499 | 0.99 | 1.00 | 0.97 | 6.5 s |

The drill and hip-hop tracks are sensitive to resampling: Python's own drill beat count ranges from 239 to 297 across soxr quality settings, and Rust falls inside that range. Their beats rely on hi-hats near the model's 11 kHz upper band, so the resampler must keep a steep anti-aliasing filter. rubato's `Fft::new` splits into 16 sub-blocks by default, which lowers the cutoff and cost the drill track 15% agreement. One sub-block (`new_custom(..., 1, ...)`) fixed it.

## Constant grid and comparison with the current analysis

The model's beats are fitted to the app's existing constant grid. The fit searches BPM within ±4% of the median beat interval in 0.005 BPM steps, picks the phase that puts the most beats within 35 ms of a grid line, refines by least squares over those beats, and places the offset on the majority downbeat phase.

| Track | Model BPM | Beats on grid | Median grid error | Downbeat phase agreement | Current BPM | Current phase |
| --- | --- | --- | --- | --- | --- | --- |
| Detroit Swindle | 121.91 | 83% | 14 ms | 81% | 121.89 (ambiguous) | none |
| Kendrick Lamar | 73.50 | 81% | 14 ms | 65% | none | none |
| FILV | 112.00 | 77% | 12 ms | 88% | 112.01 | 80 ms late |
| Fred again.. | 125.00 | 96% | 8 ms | 65% | 124.99 | 105 ms late |
| Drill beat | 142.03 | 87% | 11 ms | 97% | 71.0 (half) | none |
| Mr. Belt & Wezol | 126.00 | 100% | 6 ms | 88% | 125.97 (ambiguous) | none |

**Kick-onset check.** A low-band (34 to 155 Hz) onset envelope was averaged at each grid's beat times shifted by -250 to +250 ms. Every model grid peaks between -10 and -35 ms; the envelope's 58 ms window accounts for roughly -25 ms of that. The two current-analysis grids peak at -90 and -125 ms, so they sit 80 and 105 ms later than the model's relative to the kicks.

Model frame times are quantised to 20 ms. Sub-frame peak interpolation should reduce the median grid error further. Downbeat agreement of 65% on the hip-hop and Fred again.. tracks reflects irregular bars or metric changes that a single constant grid cannot follow; those remain out of scope for version 1.

## Speed

Milliseconds per 1500-frame (30 s) chunk:

| Runner | 1 thread | 4 threads | Peak memory |
| --- | --- | --- | --- |
| tract, original export | 1,402 | 753 | 813 MB |
| tract, grouped attention | 1,270 | 705 | 325 MB |
| tract, grouped attention, float16 compute | 1,455 | 606 | 190 MB |
| ONNX Runtime CPU, original | 1,073 | 440 | 741 to 786 MB (Python process) |
| ONNX Runtime CPU, grouped attention | 1,093 | 537 | 573 to 578 MB (Python process) |
| ONNX Runtime CoreML | not tested | 4,316 | 3.5 GB or more |

End to end (resample, mel, model and peaks) the final tract configuration takes **1.77 s per minute of audio** at 4 threads, about 9 s for a five-minute track. A 1,000-track library would take about 2.5 hours of background work. 8 threads gave almost no further gain. ONNX Runtime CPU is about 1.6 times faster but needs more memory and a large native library. CoreML is ruled out.

## Memory

The original export peaks at 813 MB because the frontend's time-direction attention runs over all 32 frequency rows at once: 32 × 1500 × 1500 scores. Exporting that attention one row at a time (`beat_this_small0_g1.onnx`) is mathematically identical (max difference 9e-6) and drops peak to 325 MB, of which 79 MB is the loaded model and runtime. Float16 compute halves the transient to about 190 MB peak without changing any beat on the six tracks, but its logits differ by up to 1.6, so it needs the full evaluation before adoption.

## Size

| Component | Size |
| --- | --- |
| tract ONNX runtime, stripped | 17.3 MB (16.4 MB with `opt-level = "s"` for tract crates) |
| tract NNEF-only loader, stripped | 13.8 MB; exporting to NNEF also needs a serializer for the exact GELU op |
| Model, float32 weights | 12.0 MB |
| Model, float16 weights cast to float32 at load | 8.0 MB; logits within 0.003; identical beats on the checked tracks |

The likely addition is **about 25 MB** (runtime plus the float16-weight model), above the originally agreed 10 to 20 MB. The alternatives were downloading the model on first launch (about 17 MB in the app) or pursuing the NNEF loader (about 22 MB, with more work).

**Decision (14 October 2026):** accept about 25 MB and ship the model inside the app, so beat analysis works offline from first launch.

## Side findings

**A. MP3 encoder delay.** The app decodes with `FormatOptions::default()` ([decode.rs:69](../crates/deck-engine/src/decode.rs)), which keeps MP3 encoder delay and padding. Its timeline is therefore about one model frame (20 to 25 ms) later than decoders that trim it, including the libsndfile path used by the reference. Analysis stays consistent inside PulseDeck because the model runs on the same PCM as playback. Positions exchanged with other software, such as rekordbox XML cues and grids, may differ by that amount. Changing it would move existing saved MP3 cues, so it needs its own migration decision.

**B. Current beat phase.** The DSP phase estimator produced no grid origin for four of the six tracks and no BPM for the hip-hop track. Where it did produce one, the grid sat 80 to 105 ms after the kicks. Two tracks are not a benchmark, but the bias deserves a separate investigation because the DSP path remains the fallback.

**C. Half/double tempo.** On the drill beat, the current analysis reports 71.0 BPM and the model reports 142.03 BPM, the conventional reading for drill. The build keeps both as candidates when they differ by a factor of two.

## Build decisions taken from the spike

- Runner: tract, pure Rust, 4 threads at background priority. One track at a time; deck loads keep priority.
- Model: grouped time attention, static `[1, 1500, 128]`, float16 stored weights, float32 compute. Ship in the app resources with a recorded hash.
- Run on the same PCM as playback so beats share the app's timeline.
- Resampler: rubato FFT with a single sub-block.
- Tracks shorter than 30 s are zero-padded into one chunk, which differs slightly from upstream; check this on samples and loops.
- New dependencies: `tract-onnx` 0.23.8, `tract-linalg` (`multithread-mm`), `rubato` 5, `audioadapter-buffers` 5.

## Limits

Six tracks. Agreement with the reference implementation is not ground truth, and the kick check covers phase only. The annotated 32-track evaluation in the scope decides whether feature 1 ships.

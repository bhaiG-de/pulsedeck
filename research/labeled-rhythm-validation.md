# Labeled beat/downbeat validation — analysis version 11

The Take Five meter hypothesis is now supported by acoustic bar recurrence rather than forced into the neural model's preferred 4-beat bar. Local tempo is authoritative for sync, and inferred pulse repairs remain visible for review. The current results **do not establish Rekordbox parity**. Strict timing improves on labeled music, while several meter/phase failures and small regressions remain.

## Implementation

- Full pinned Beat This! final0 FP32 model; BSD DBN hypotheses 3/4/5/7 at 55–300 BPM. A bounded quadratic refinement moves a selected beat by at most 10 ms; its downbeat shares the same timestamp.
- An independent beat DBN and beat-synchronous mel recurrence test bar periods 2–8. Distinctive sustained odd-meter evidence can override conflicting neural downbeats. Exceptionally strong recurring 3/4/5/7 material also qualifies; integer-multiple repetition is treated as an alias.
- A guarded half-beat phase replacement requires stable tempo and agreeing bass/mid transient evidence. Recurring bass accents may determine bar phase, with neural evidence as fallback. These are heuristics, not universal musical rules.
- Supported half-pulse runs may gain midpoint events. `bar_evidence.pulse_repairs` records the passage, inserted count and before/after tempo. Both surrounding pulses must agree within ±15% of the dominant period, and at least 60% of intervals must support that level. Only individual long intervals are split, preserving normal beats in mixed boundary blocks. Strong acoustic slow-bar structure preserves a real slowdown; an additional fixture covers returning to the original tempo. Repaired or phase-adjusted passages retain a review warning.
- Sync uses source-local grid periods, including while preparing a paused deck. Paused sync preserves the selected entry position; pending launches hold preparation rates. Live tempo correction is smoothed over 30 ms. Guarded launch acceptance understands synced local tempo instead of rejecting solely on the track's median BPM.
- Manual bar numbering follows detected meter, including a manual uniform grid. Reanalysis preserves human anchors, including when an explicit model phase replacement appears. UI and CLI inspection expose meter, local tempo and inferred-event review evidence. Manual origin/BPM controls remain available through both UI and `pulsedeckctl`.

## Human-labeled corpus and protocol

The [model authors](https://github.com/CPJKU/beat_this) state that final0 excludes GTZAN from training. We used their [published GTZAN spectrogram archive](https://zenodo.org/records/13922116) and [v1.1 beat annotations](https://github.com/CPJKU/beat_this_annotations/releases/tag/v1.1), pinned to commit `890407d158078527ab396b49fea3c8a83e5734ee`. Archive MD5: `39a7dfe6a6b0a5279a94d770506db879`; per-source and annotation hashes are retained in JSON.

There are 999 available sources. Before first scoring, IDs were partitioned by SHA-256 modulo 5 into 219 development and 780 evaluation clips. 776 evaluation clips have usable downbeat labels. GTZAN contains [documented duplicates and labeling faults](https://github.com/boblsturm/GTZAN); this partition is not proven recording-independent. The original version-6 policy was frozen before the first evaluation. Subsequent Spotify-driven changes reused this partition, so version-11 results are **regression evaluation on model-training-excluded material**, not a fresh held-out policy test.

[mir_eval 0.8.2](https://mir-eval.readthedocs.io/latest/api/beat.html) scores one-to-one event F1 at 70, 20 and 10 ms, plus CMLt/AMLt. All values below are macro means per clip with **no 5-second trimming**. Separate trimmed results and individual failures are included in JSON. Tempo Acc1 requires ±4% at the annotated beat level; Acc2 additionally permits half/double tempo. Baseline is the original frame-based same-logit 3/4 DBN at the same 300 BPM cap. Neither baseline nor current scores are Rekordbox measurements.

| Metric | Baseline | Version 11 | Change, percentage points |
|---|---:|---:|---:|
| Beat F1, 70 ms | 88.04% | 87.98% | −0.06 |
| Beat F1, 20 ms | 63.26% | 64.86% | +1.60 |
| Beat F1, 10 ms | 36.09% | 36.94% | +0.85 |
| Downbeat F1, 70 ms | 78.54% | 78.38% | −0.16 |
| Downbeat F1, 20 ms | 56.16% | 57.51% | +1.35 |
| Downbeat F1, 10 ms | 31.71% | 32.32% | +0.61 |
| Exact tempo Acc1 | 83.08% | 83.33% | +0.26 |
| Half/double-tolerant tempo Acc2 | 93.08% | 93.33% | +0.26 |

This is a timing improvement with a small broad-tolerance regression, **not an overall accuracy win on every metric**. Beat CMLt is 80.48%, downbeat CMLt 74.88%. The gap between Acc1 and Acc2 shows that metrical tempo interpretation remains a real issue.

Odd-meter support is sparse: the evaluation partition contains one source with modal meter 5, `gtzan_rock_00064`; it improves from downbeat F1 22.86% / predicted 4 to 100% / predicted 5. Development `gtzan_classical_00081` is still predicted as 3 despite modal reference meter 5 (downbeat F1 26.32%). These two clips cannot establish reliable general odd-meter detection.

## Full PCM and packaged-app checks

Published spectra do not exercise audio decoding or feature extraction. An additional 100 original local GTZAN clips, ten per genre, were analyzed from WAV through ffmpeg-to-memory and the real Rust PCM pipeline. The [mini-corpus distribution](https://github.com/TempoBeatDownbeat/gtzan_mini) is also referenced by [mirdata's GTZAN loader](https://github.com/mir-dataset-loaders/mirdata/blob/master/mirdata/datasets/gtzan_genre.py); archive MD5 is `44f7f23af8363d96c59663a987f29a4c`. It overlaps the main corpus and is not an additional independent dataset. Its 80 evaluation-partition clips give:

| Metric | Baseline | Version 11 |
|---|---:|---:|
| Beat F1, 70 / 20 / 10 ms | 88.15 / 64.07 / 38.48% | 88.37 / 65.70 / 39.04% |
| Downbeat F1, 70 / 20 / 10 ms | 73.42 / 50.20 / 30.46% | 73.46 / 52.13 / 29.35% |
| Exact tempo Acc1 | 86.25% | 87.50% |

The 10 ms PCM downbeat regression is retained. Scoring uses the actual stored PCM result, including acoustic evidence; saved logits alone cannot reproduce the mel-dependent policy.

Ten preselected first clips, one per genre, were also loaded through the rebuilt **packaged version-11 app**, exercising Symphonia decoding and native metadata publication. Beat F1 is 90.78 / 67.78 / 35.25%; downbeat F1 is 87.09 / 60.31 / 30.91% at 70 / 20 / 10 ms. These are integration checks on overlapping clips with mixed split membership. Both outputs were muted and transports paused.

Independent generated audio adds sample-indexed 4/5/7-beat references, kick/piano accents and offbeat hats. Version 11 selects all three meters correctly, and the 173 BPM seven-beat source scores beat F1 99.74%, downbeat F1 100% at 70 ms. **The 123 BPM four- and five-beat sources still lock to offbeat hats: beat/downbeat F1 are 0% at 70 ms.** The dual-band guard does not correct these. Their data and generator are retained rather than excluded. The feature-level unit fixture separately checks that the guard requires both bands and leaves a correctly phased grid alone; passing that unit test does not override the audio failure.

## Spotify status and limits

Native version-8 and version-10 Spotify runs changed Take Five from a 4-beat grid to 5, but retained the half-pulse solo. The final **version-11 Spotify run** retains five-beat bars and reconstructs 142 missing pulses across approximately 158.48–263.85 seconds. Beat/downbeat counts change from 774/154 to 916/183. Median local tempo changes from 167.86 to 171.14 BPM; the 5th–95th percentile of 32-beat local tempo changes from **86.95–176.25 to 166.33–176.46 BPM**. These are declared midpoint reconstructions, not newly observed neural events or human-labeled accuracy. Normal intervals inside boundary blocks are preserved. Five sampled windows verify that affected passages carry `inferred_events_in_range:true` and `pulse_review_required:true`; the source warning remains active. Authentication is restored.

Take Five's [5/4 composition](https://blogs.loc.gov/nls-music-notes/2020/05/song-stories-time-out-for-take-five/) is a meter reference; it is not timestamped beat ground truth for the specific Spotify release. Spotify checks inspect runtime metadata and grid-relative transport, not independent acoustic beat/downbeat accuracy. No provider PCM or mel features are exported, and provider recording remains disabled.

The final version-11 sweep covers twelve preselected Spotify style cases spanning approximately 77–211 BPM, with all processing-integrity checks passing. The three-beat waltz and five-beat Take Five hypotheses match their composition-level references. Exact acoustic alignment remains unverified. See [the current source and transport report](spotify-rhythm-v11-validation.md), [Take Five before/after evidence](spotify-take-five-v11-validation.json), and the historical [version-5 report](spotify-bpm-genre-validation.md).

## Reproduction and artifacts

- [Fixed split](labeled-rhythm-split.json), [policy history/source hashes](labeled-rhythm-policy.json).
- [Development results](labeled-rhythm-development.json), [780-clip regression results](labeled-rhythm-evaluation.json), [80-clip full PCM results](labeled-rhythm-pcm-evaluation.json).
- [Packaged-app results](labeled-rhythm-native-evaluation.json), [controlled audio failures](labeled-rhythm-controlled-fixtures.json).
- [Decoder implementation/provenance](../crates/audio-analysis/src/dbn/README.md), [manual-grid agent workflow](../.agents/skills/pulsedeck-track-inspection/SKILL.md).

Build verified model assets and benchmark workers with `python3 tools/ensure-beat-models.py` and `bash tools/rust-env.sh cargo build -p audio-analysis --release --examples`. Use a Python environment containing NumPy and mir_eval 0.8.2. Download the official spectra and annotations separately; source audio, raw spectra and activations are not checked into this repository.

```sh
python tools/infer-labeled-rhythm.py --archive /path/to/gtzan.zip \
  --manifest research/labeled-rhythm-split.json \
  --worker target/release/examples/labeled_worker --cache /tmp/rhythm-cache --workers 4
python tools/validate-labeled-rhythm.py --cache /tmp/rhythm-cache \
  --annotations /path/to/beat_this_annotations/gtzan/annotations/beats \
  --worker target/release/examples/labeled_worker \
  --spectrograms /tmp/rhythm-cache/gtzan.npz --split evaluation --output /tmp/evaluation.json
python tools/validate-labeled-pcm.py --manifest research/labeled-rhythm-split.json \
  --audio-root /path/to/gtzan_mini-main/genres --cache /tmp/rhythm-pcm --workers 4
python tools/validate-labeled-rhythm.py --cache /tmp/rhythm-pcm \
  --annotations /path/to/beat_this_annotations/gtzan/annotations/beats \
  --worker target/release/examples/labeled_worker --split evaluation --output /tmp/pcm-evaluation.json
python tools/generate-labeled-rhythm-fixtures.py --output /tmp/rhythm-fixtures
```

Version-11 workspace tests and strict Clippy passed; frontend 49 tests passed; seven DJ skills and 37 shell examples validate. The full model is exercised by the PCM checks, although its opt-in unit integration test remains ignored in the normal workspace suite. The rebuilt app remains launched with Spotify access restored. Twelve final-build Spotify sources and five queued-launch overlaps passed runtime checks; the fifth uses Take Five's repaired solo. Original Into You / Calmy sources and positions are restored with both decks paused, gains 0.8, and no pending launches/jobs.

To substantiate Rekordbox-level or better, the next accuracy gate requires independent recordings with substantial 3/5/7 and variable-meter support, annotated difficult passages, and the same-audio Rekordbox beatgrid export as a comparator. These measurements show why that gate is still necessary.

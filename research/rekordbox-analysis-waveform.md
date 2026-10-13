# BPM, musical key and three-band waveform investigation

Reference build: installed rekordbox **7.2.19.0342**, `/Applications/rekordbox 7/rekordbox.app`. Main executable SHA-256 rechecked against retained analysis: `07dcecbaf304f777e435539c83c7dcc02e41de1eaf2e0a75e8a9852e4411a244`. Investigated 13 October 2026.

**Result:** the installed program has independent beat/key jobs and a genuine filtered three-band waveform pipeline. Focused static inspection recovers useful waveform filter, envelope, sampling and palette details. It does **not** establish that an independent implementation already matches rekordbox's BPM/key accuracy, nor recover its complete key/beat algorithms or proprietary model weights.

## Evidence and limits

Reused REA Evidence:

- Build plist `ev_12b451406e1004bfaef99ec5f6c547a0dd4f36b3a983c2778e4cd6e2725bc650`.
- Main Mach-O `ev_0cb56fe857156135bf806b67c3ff43ac5eb91abe354136a7ecf74dc5d45d27ad`.
- Artifact inventory `ev_72038f3cc108669838b0efd22a835302e12f27a5e7aa7f24185b268659c0c534`.

These establish target identity, dependencies and model/resource presence. They do not establish runtime reachability. Their complete retained outputs are under `research/evidence/rekordbox-7-installed`.

The prior REA session has no configured native decompiler provider; no REA MCP function tools were exposed to this worker. Instead of repeating readiness or installing a disassembler, this pass reused the matching shell symbol inventory and applied Apple LLVM ARM64 disassembly to selected functions. **All function/constant findings below are shell static observations and manual interpretations, not REA decompiler Evidence.** No native app UI was operated or process launched, no credentials were read, and no vendor models/assets were copied into implementation.

Retained focused outputs:

- [Symbol selections](evidence/rekordbox-analysis-waveform-symbols.json).
- [Disassembly identity manifest](evidence/rekordbox-analysis-waveform-disassembly-manifest.json), which hashes all selected outputs.
- [Selected analysis/generation functions](evidence/rekordbox-analysis-waveform-arm64.asm) and [named direct calls](evidence/rekordbox-analysis-waveform-direct-calls.json).
- [Wave input/filtering](evidence/wave-read-data-arm64.asm), [key pipeline](evidence/analysis-get-key-arm64.asm), [key version dispatch](evidence/key-wrapper-arm64.asm).
- [Three-band renderer](evidence/wave-three-band-render-arm64.asm), [vertex construction](evidence/wave-three-band-vertices-arm64.asm), [decoded constant ledger](evidence/rekordbox-waveform-constant-ledger.json).

Coverage is selected ARM64 functions, manually interpreted local instructions and named direct calls. Indirect calls, transitive reachability, some undecoded instructions, active settings and actual processed results remain outside this pass. This is not original source or a whole-program reconstruction.

## BPM and beat-grid boundary

At `0x102327c80`, `AnalyzeJobBeatDetector::Detect()` branches to separate `highPrecisionBeatAnalyze()` and `legacyBeatAnalyze()` methods. Their bodies provide stronger evidence than names alone:

- High-precision path (`0x102327e50`) calls `BALibWrapper::GetBpm`, beat count/offset/tempo-variation accessors, and **`madmom::analyze(...)`**. It then reads JSON result fields, calls beat-position refinement accessors and `convertToMicrosec(...)`.
- Legacy path (`0x102328684`) calls the wrapper for BPM, beat count/list, then timestamp conversion and first-beat handling.
- Other declared methods include BPM doubling/halving and beat renumbering. They are declared operations; their callers were not established here.

The analysis stage therefore produces a beat sequence and timing structure, not merely one BPM number. The existing resource inventory also contains TensorFlow beat models under `models/detect_beat/model7` and `model8`. **Unknown:** which model is selected in each case, model tensor contract, decoder parameters, exact preprocessing, postprocessing, latency/accuracy and the relationship between AlphaTheta's `madmom::analyze` and upstream madmom. The compiled name does not prove it is an unchanged upstream implementation.

The official manual documents Normal mode for relatively constant tempo, Dynamic for changing tempo, Auto only with high-precision BeatGrid analysis, and selectable BPM ranges. It says higher precision is slower, and permits separate BPM/Grid and KEY analysis selection. This corroborates the feature boundary without revealing the algorithm. The consulted manual is 7.2.14, not this exact installed build. [Official manual, printed pp. 14 and 230–231](https://cdn.rekordbox.com/files/20260409151936/rekordbox7.214_manual_EN.pdf).

## Musical key boundary

`AnalyzeJobKey::Detect()` (`0x101b09f38`) passes detailed beat data, PCM-related storage and parameters to `getKey(...)`; it stores a nonnegative returned value as job result. The selected `getKey` body (`0x101b0a1a8`) directly calls `CutBeats`, `PitchDetect`, multiple `GetKey` passes, and `KeyJudge` on `BALibWrapper`.

`BALibWrapper::GetKey` (`0x101f54850`) checks a version field and dispatches to **BeatAnalyzer_1_0::BA_GetKey** or **BeatAnalyzer_2_0::BA_GetKey**; missing/unsupported state returns zero. This establishes compiled alternative engines, **not** which version is used for any specific track. The underlying pitch features, profiles, tonal segmentation, confidence calibration and exact key-ID mapping remain unresolved.

The result must remain separate from playback key shift/master-tempo state and from file metadata. The manual allows detected key to be written to ID3; the investigation did not change tags. AlphaTheta's 7.1.4 release notes explicitly report improved key-analysis accuracy, so historical benchmarks or older private algorithms cannot establish this build's quality. [Official 7.1.4 release notes](https://rekordbox.com/en/2025/07/rekordbox-v714-release-information/).

## Three-band waveform signal path

`WaveCreator::createWave(...)` (`0x100d54974`) directly invokes separate whole, color and three-band creation paths. For the three-band input path, `readWaveData(...)` uses its own embedded `BandPassFilter` state; it does **not** call the unrelated `ThreeBandSeparate` helper found elsewhere in the symbol table.

The filter coefficient formulas in the WaveCreator constructor, matched to state offsets used by `readWaveData` at `0x100d56f84–0x100d56fdc`, support this **static reconstruction**:

| Envelope | Filter chain | Anchors |
|---|---|---|
| Low | Second-order low-pass around **300 Hz** | Constructor reuses 300-Hz trig values at state `0x4a8`; read path calls that state into its first band buffer. |
| Mid | Second-order high-pass **250 Hz**, then low-pass **1200 Hz** | States `0x500` and `0x558`; constants/trig formulas at `0x100d54360–0x100d54420`. |
| High | Second-order high-pass **1200 Hz**, then low-pass **9000 Hz** | States `0x5b0` and `0x608`; setup at `0x100d54438–0x100d544e8`. |

The denominator uses a `sin(theta)/sqrt(2)` factor, consistent with second-order Butterworth coefficient construction. These are overlapping filters with rolloff, not hard spectral masks, equal FFT thirds, mixer-EQ crossover definitions or stem separation. The unrelated `ThreeBandSeparate` class contains 284/4649-Hz filter arguments; **do not use those as the waveform crossovers**.

The actual low/mid/high input buffers are reduced by **maximum absolute amplitude** over small time bins, converted to saturated Q15-like positive values (up to 32767), and stored as three 16-bit words. Relevant peak/max instructions and stores are at `0x100d57060–0x100d57228`. It is therefore an envelope waveform, not a spectrogram or a display of instantaneous signed PCM samples.

Constructor count arithmetic (`0x100d53d80–0x100d53db8`) is consistent with about **1000 source envelope bins/second** and **150 detailed display points/second**. This rate interpretation assumes the reader's rate and frame-count fields have their inferred meanings. It is not a runtime format measurement.

`create3BandsZoomWaveData()` (`0x100d55fbc`) allocates three bytes per output point. Its per-band look-back spans are 300, 200 and 100 source bins, with immediate attack on larger values and release multipliers 0.99, 0.98 and 0.97. Under the nominal source-rate interpretation, these spans are 300/200/100 ms. Boundary handling and point resampling are visible, but not fully reconstructed into executable source.

With default gains of 100%, the first two output amplitude bytes use approximately **Q15 peak / 256**; the third uses **64 × (1 − cos(π × Q15 peak / 32768))**. This is a nonlinear amplitude transform, **not an observed logarithmic/dB quantizer**. Integer conversion truncates before byte stores. Gain, smoothing, whole-wave normalization and boundary behavior matter if visual matching is required; simply recoloring a mono peak envelope will not reproduce this structure.

Full-track preview and detailed/scrolling waveform have separate creation/storage/rendering paths. Their entire reduction and normalization policies were not recovered here.

## Rendering and palette

`WaveView::draw3BandsWave()` creates these exact RGBA colors in the inspected CPU path (`0x10218f7d8–0x10218f878`):

| Role inferred from conventional band order | Hex |
|---|---|
| Low blue | `#0055E1` |
| Mid orange | `#FFA600` |
| High white | `#FFFFFF` |
| Additional overlap/intersection palette | `#B4690A`, `#D2DCFA`, `#FFF0D7`, `#F5EBD7` |

The color values themselves are directly observed; band-role assignment and full shape policy are manual interpretation. The method compares per-band extents and chooses colors before drawing vertical segments (`0x10218f924–0x10218fa78`). This supports **overlaid envelopes with intersection coloring**, rather than summing heights into a cumulative stacked-area chart. Exact tie ordering, all display modes, half/full views and Metal/shader equivalence are unresolved. A separate vertex-building function also exists; that alone does not prove which rendering path is active.

The official CDJ-3000 page corroborates that 3Band presents high/mid/low band volume using different colors and analyzed waveform data. The official FAQ also explains that older analyses may lack three-band data and require additional analysis. [Product explanation](https://www.pioneerdj.com/en/product/dj-players-turntables/cdj-3000/), [rekordbox FAQ](https://rekordbox.com/en/support/faq/rekordbox6/#faq-4787).

## Independent Rust reconstruction recommendations

These are **design recommendations**, not claims about recovered source:

1. Decode supported local files to an explicitly defined normalized PCM format. Run analysis in cancellable workers with file fingerprints and versioned caches. Share decoded chunks among BPM/key/waveform jobs without putting analysis or allocations in the audio callback.
2. For the waveform, independently implement the traced filter/envelope approach. Keep true separate low/mid/high arrays, overview/detail caches and stable sample-to-time conversion. Preserve float or Q15 envelopes internally and quantize only at the cache/display boundary. Start with the recovered nominal filters and verify using single-band tones, swept tones, mixed impulses, silence and antiphase stereo. Verify input fold-down rather than assuming an unqualified stereo average.
3. Render each column from the three real envelopes about a common baseline. Sort/clip extents so smaller bands remain visible, then add intersection colors. Benchmark scrolling and zoom with precomputed levels; a webview should receive visible ranges or shared/binary buffers, not full-track arrays every frame.
4. BPM accuracy needs onset/beat evidence, phase alignment and half/double-tempo disambiguation. A useful independent baseline combines multiband spectral flux, autocorrelation/tempogram candidates and a beat-sequence decoder, with constant-tempo and dynamic-grid modes. Spectral-flux onset strength is documented by [librosa](https://librosa.org/doc/0.11.0/generated/librosa.onset.onset_strength.html). A higher-quality tier can use an independently licensed beat-activation model plus DBN/HMM decoding; upstream [madmom's beat code](https://github.com/CPJKU/madmom/blob/main/madmom/features/beats.py) documents that architecture. This does not identify AlphaTheta's exact model or authorize reuse of its weights.
5. Key accuracy needs pitch-class features, tuning correction, harmonic/noise suppression and multiple tonal windows. CQT/HPCP/chroma with major/minor profile scoring is a baseline, not a parity guarantee. [Librosa's chroma explanation](https://librosa.org/doc/main/auto_tutorials/01-intro/04-harmony.html) explains octave aggregation and CQT's frequency-dependent resolution. Expose confidence/unknown states, alternatives and manual correction; keep detected key provenance separate from imported tags and playback transposition.
6. Evaluate the engine on the same full files as the reference app with recorded analysis settings, plus an independently annotated holdout corpus. Report raw BPM error, half/double errors, beat timing/downbeat alignment and exact/related-key errors separately. Matching a vendor label is a compatibility metric, not necessarily ground truth. [mir_eval tempo metrics](https://mir-eval.readthedocs.io/stable/api/tempo.html) support explicit tempo tolerance; beat/key tasks need separate metrics.

## Coverage checklist

| Requested question | Status |
|---|---|
| Identify BPM/key analysis boundaries | Partially answered by compiled direct calls and docs. |
| Recover exact accuracy algorithm/model contract | Unresolved; no full native decompilation, model execution or accuracy benchmark. |
| Establish real three-band waveform content | Answered for the selected static generation path. |
| Recover nominal band filters, bin/detail rates and quantization | Partially answered by local instruction/constant reconstruction; runtime verification pending. |
| Recover palette/composition policy | Palette directly observed; overlap strategy partially inferred; exact renderer parity pending. |
| Prove independent implementation accuracy/visual parity | Unresolved; requires generated signal tests and reference-file comparisons. |

No Spotify protocol, credential path or application setting was inspected or modified in this subtask.

## Independent implementation and live acceptance, 2026-10-13

PulseDeck analysis v2 now computes distributed spectral rhythm estimates and tuning-aware pitch-class key candidates off-thread. The independent waveform uses the recovered nominal frequency ranges and blue/orange/white palette, 4096 aligned RMS envelopes and a neutral peak reference. This uses RMS reduction rather than an exact reconstruction of the vendor's Q15 peak/attack/release/quantization pipeline. Main and overview plots were visually verified with real Spotify tracks resident in RAM.

The first live reference, Waterstone/Amanda Wilson “It's Over Now — Extended Vocal Mix” (6:17), produced a leading 122.22 BPM candidate and B minor/10A. The published [Beatport entry](https://www.beatport.com/track/t/16682970) gives 122 BPM/B minor. This exposed an overly strict agreement gate that hid the BPM despite useful evidence; the support-based soft agreement correction now returns credible estimated BPM while retaining ambiguity. Cubic lag interpolation improved the fractional synthetic fixtures; subsequent live tracks displayed 93.32 and 119.99 BPM. “FALLING FOR YOU” produced 93.32 BPM and Bb minor/3A candidates, without an independently verified reference. Native Reanalyze preserved A at 1:57 and B at 0:00. [Sanitized live evidence](evidence/analysis-v2-live.json) contains metadata only, without provider audio or credentials.

Synthetic tests and one catalog reference are insufficient to claim rekordbox accuracy parity. Constant/dynamic beat phase alignment, a larger independently annotated dataset and hardware/long-run performance measurements remain necessary.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

# Rekordbox beat grids, downbeats and manual correction

Investigated 14 October 2026, Asia/Kolkata. Target: installed `/Applications/rekordbox 7/rekordbox.app`, build **7.2.19.0342**. The main executable SHA-256 still matches the retained REA analysis: `07dcecbaf304f777e435539c83c7dcc02e41de1eaf2e0a75e8a9852e4411a244`.

The shipped high-precision analysis path explicitly combines TensorFlow inference with a bar/downbeat Hidden Markov Model and Viterbi decoding. Its grid editor separately changes beat timestamps, beat-in-bar numbering and BPM, then forwards edited beat information into the engine. This supports implementing independent detection, editable grid timing and explicit downbeat metadata in PulseDeck. It does not establish exact proprietary preprocessing, weights, parameter values or accuracy parity.

## Reused REA evidence and current limitation

The [prior analysis](rekordbox-analysis-waveform.md) and [architecture report](installed-rekordbox-architecture.md) were reused rather than rescanning the bundle:

| Evidence ID | Observation |
|---|---|
| `ev_12b451406e1004bfaef99ec5f6c547a0dd4f36b3a983c2778e4cd6e2725bc650` | Plist/build identity. |
| `ev_0cb56fe857156135bf806b67c3ff43ac5eb91abe354136a7ecf74dc5d45d27ad` | Main Mach-O identity and dependencies. |
| `ev_72038f3cc108669838b0efd22a835302e12f27a5e7aa7f24185b268659c0c534` | Packaged resource inventory, including TensorFlow beat models. |

No REA MCP tools were exposed to this worker. A cached public `rea` **6.4.0** launcher was available; [current capabilities](evidence/rea-beatgrid-capabilities.json) reports `open:false`, no selected provider, and unavailable Ghidra (not configured), Hopper (launcher missing), and IDA (registration absent). No client configuration or engine was installed or changed. No native session was opened, so no session required closing.

The focused function bodies below were decoded with Apple's existing `llvm-objdump`, not a REA decompiler. They are **shell static observations**, with source/output identities and exact direct calls retained in [the static manifest](evidence/rekordbox-beatgrid-static-manifest.json). A compiled call establishes that the branch exists; it does not prove that a particular loaded track executed it. No reference playback, library access or grid mutation was performed.

## Detection: evidence beyond model filenames

1. Reused `AnalyzeJobBeatDetector::Detect` at `0x102327c80` selects high-precision or legacy analysis. In the retained high-precision body, `0x10232803c` directly calls the JSON-returning `madmom::analyze` overload.
2. [JSON analysis body](evidence/rekordbox-beat-analyze-json-arm64.asm), entry `0x10230a820`, directly calls the PCM overload at `0x10230abb0`. It later calls `madmom::get_beat_list` and `madmom::get_bpm` at `0x10230b70c` and `0x10230b718`.
3. [PCM analysis body](evidence/rekordbox-beat-analyze-pcm-arm64.asm), entry `0x102307f8c`, directly calls `TFWrapper::Model::run`, e.g. `0x102308444`, and `madmom::downbeat_tracking`, e.g. `0x102308540`. Multiple branches contain these calls; their active selection is unresolved.
4. [Downbeat tracking body](evidence/rekordbox-downbeat-tracking-arm64.asm), entry `0x102300d6c`, constructs `BarStateSpace`, `BarTransitionModel`, `RNNDownBeatTrackingObservationModel` and `HiddenMarkovModel<BarTransitionModel,RNNDownBeatTrackingObservationModel>` at `0x102301104–0x102301148`. It calls `threshold_activations` at `0x102301254` and the HMM's `viterbi` method at `0x102301304`.
5. [First-beat postprocessing](evidence/rekordbox-detection-first-beat-arm64.asm), entry `0x102328a00`, reads a timestamp and a separate beat-number field, uses the local `convFirstBar` table and cycles a stored number across 1–4. This is evidence for distinct timing and bar-position data; exact structure field definitions and all special cases remain inferred.

**Interpretation:** inference yields activation data; a bar-state decoder imposes rhythmic continuity and selects a beat/downbeat sequence. This architecture is substantially richer than estimating global BPM and placing the first gridline at the beginning of a file. Names do not establish an unchanged upstream madmom implementation. No vendor weights were copied or executed, and the full preprocessing and decoder parameter contract remains unknown.

## Grid editing: timestamp, numbering and engine propagation

| Exact compiled path | Observed operation and implication |
|---|---|
| `BeatGridAdjustment::setFirstBeatOn`, `0x1010c1180` | Reads `WidgetWaveView::getBeatPosition`, begins/executes JUCE undo transactions, updates displayed beat positions, then calls `updatePlayerBeatInfo` at `0x1010c14d4`. A displayed grid correction participates in engine state updates. [Body](evidence/rekordbox-grid-set-first-arm64.asm). |
| `SetFirstBeat::perform`, `0x1010c2c70` | Copies beat-array state for undo and calls `renumberBeat` at `0x1010c2d18` with final argument `1`. [Body](evidence/rekordbox-grid-first-beat-arm64.asm). |
| `renumberBeat`, `0x1010bb878` | The forward loop at `0x1010bb948–0x1010bb97c` cycles the lower beat-number field modulo 4 and preserves the other packed word/timestamp values. Beat-in-bar phase is separately editable. [Body](evidence/rekordbox-grid-renumber-arm64.asm). |
| `MoveBeatPos::perform`, `0x1010c24b4` | Copies beat-array state and calls `moveBeatPos` at `0x1010c2560`. A separate operation moves timing. Full helper semantics were not traced. [Body](evidence/rekordbox-grid-move-arm64.asm). |
| `applyNewBpm`, `0x1010c056c` | Validates the proposed BPM, reads beat positions and total duration, constructs a JUCE undoable action and performs it. Exact regenerated-grid arithmetic remains untraced. [Body](evidence/rekordbox-grid-bpm-arm64.asm). |
| `updatePlayerBeatInfo`, `0x1010c06a4` | Constructs `BeatInfo` from edited positions, calls `DjEngineIF::updateBeatPosInfo` at `0x1010c0714`, then posts a common message. Display and transport must consume the same edited grid. Exact asynchronous delivery guarantees remain unknown. [Body](evidence/rekordbox-grid-update-player-arm64.asm). |

## Official behavior corroboration

The [official 7.2.14 manual](https://cdn.rekordbox.com/files/20260409151936/rekordbox7.214_manual_EN.pdf) is an earlier version than the installed build. It documents Normal analysis for stable tempo, Dynamic for changing tempo, and Auto with high-precision analysis (pp. 14, 230). Its grid editor sets the playhead as a bar's first beat, accepts BPM, shifts timing in 1 ms steps, doubles/halves BPM, supports whole-track or forward-only adjustment, and offers undo/redo and a synchronized metronome (pp. 75, 135, 152). Analysis Lock protects corrected data; reanalysis otherwise replaces the grid (pp. 91–92). Beat Sync aligns tempo **and beat positions**; Quantize snaps cue/loop actions to the grid (pp. 154, 162). These documented actions do not establish that an ordinary Play command waits for a phrase boundary. Phrase editing is a separate control group.

The [official feature overview](https://rekordbox.com/en/feature/overview/) describes proprietary BPM/grid analysis, manual BPM/grid correction, and separate phrase analysis. Neither source publishes complete algorithm details.

## Independent implementation recommendations

These are PulseDeck design choices, not recovered vendor source:

- Treat tempo, beat timing, bar/downbeat phase and musical phrases as distinct information. A correct BPM can have the wrong timing offset; perfectly aligned beats can still be several bars out of phrase.
- Keep one authoritative effective grid for waveform rendering, transport sync, quantization, loops and the CLI. Maintain detected data plus a persisted manual override with provenance, so reanalysis cannot silently discard a user's correction.
- Offer a stopped-deck UI action that places a bar downbeat at the exact playhead, plus small timing nudges, explicit BPM, half/double BPM and reset. A separate bar-number action can correct a wrong bar phase without moving correctly aligned transient timestamps. Label these distinctions plainly.
- Expose the same engine commands to the CLI, with exact seconds or grid indices rather than UI-relative guesses. Return effective tempo, anchor/downbeat and provenance after mutation. Editing a currently audible deck needs an explicit policy; preparation should ordinarily use the isolated incoming deck.
- A phrase-aligned launch needs an explicit chosen target boundary in the master deck and a selected entry boundary in the incoming deck. Queue the start inside the audio engine, using its sample/frame clock; shell polling and sequential CLI invocations cannot guarantee synchronous starts. Retain actual launch time and postlaunch phase error for verification.
- Test grid offset and bar-phase correction separately; verify UI/CLI agree, overrides survive reload/reanalysis, and a quantized launch aligns rendered gridlines with the engine's sample positions. Use nonzero audio lead-in, pickup bars, half/double tempo ambiguity and tempo drift fixtures.
- Neural beat/downbeat activations plus HMM/DBN decoding are an independently implementable quality tier with appropriately licensed models. The observed vendor path supports that design, but detection quality still requires independently annotated audio and measurements.

## Coverage

| Question | Status |
|---|---|
| Does shipped high-precision detection use explicit downbeat decoding? | Answered for selected static path: TensorFlow inference, bar-state HMM and Viterbi calls. |
| How does manual realignment reach display and engine? | Partially answered: undoable grid edits, renumbering, waveform update and engine BeatInfo update. |
| What UI controls are offered? | Answered from official earlier-version manual; no live UI mutation. |
| Does Beat Sync align phase, and does Quantize imply phrase alignment? | First documented; second unsupported. Phrase choice must remain separate. |
| What are the exact model inputs, weights, selected branch and decoder parameters? | Unresolved. |
| Does PulseDeck now match reference downbeat/phrase quality? | Not established by this read-only reference investigation. |

## Implementation follow-through in PulseDeck

The independent implementation now uses full MIT-licensed Beat This! `final0` inference plus a BSD madmom-style native DBN/HMM decoder. It compares whole-track hypotheses of three or four beats per bar over 55–300 BPM, selecting a temporally consistent beat/bar sequence while preserving local timestamp variation. Arbitrary meter changes and phrase/arrangement recognition remain outside that decoder policy. No vendor weights or madmom NC pretrained weights are used. [Model and decoder evidence](model-candidates.md), [measured validation](beat-detector-validation.md).

One effective grid feeds native clock math, sync, quantization, queued launch, waveform markers and bounded CLI events. Offset-only manual origin realignment preserves active neural intervals. Explicit manual BPM/half/double correction switches to a constant-tempo grid for the whole source; an offset correction does not switch that mode back. Manual bar-origin numbering uses four beats per bar. UI applies a BPM mutation only for a draft change greater than 0.01 BPM. Source-specific overrides survive reload/reanalysis.

Loaded `beatgrid_uniform`, `beatgrid_manual` and `beat_position` are the authoritative effective state. `track.inspect.beat_events` reports bounded effective timestamps and `uniform`/manual provenance, while its neural metadata summary retains original counts and scores. Consequently original-source/cache data may differ from a loaded edited grid. `deck.analyze` refreshes analysis through CLI `deck B analyze`; no `track.reanalyze` method is introduced. Scores are uncalibrated, and a failed-analysis warning requires manually validated timing before automated beatmixing.

Native source-guarded launch queues an incoming start at a selected future outgoing grid event using the audio renderer's clock. The agent chooses the arrival event and counts backward to its musical entry, verifies both native phases while incoming audio is isolated, then opens the channel and monitors drift/arrangement. The decoder's regular bar spacing cannot establish that a selected transition is well phrased or free of vocal/bass clashes. Current reference-decoder parity and limited EDM tests do not prove rekordbox accuracy parity.

Local captures are excluded from the public repository; evidence links refer to the investigator's local workspace.

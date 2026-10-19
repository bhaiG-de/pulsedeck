# Mixxx architecture and PulseDeck improvements

Investigation date: 19 October 2026. Scope: playback engine, Auto DJ, tempo/grid, analysis, effects, library and control workflow. The aim is to improve reliable human and agent operation without replacing working components speculatively.

## Evidence and version boundaries

The official download page identifies **2.5.6 as stable**, 2.6 as beta and 2.7 as development. This investigation inspected the official source at stable tag `2.5.6`, commit `3ebac449e7e5fe2a0186596657696e87ce8b0e56`, and development main `44dc9ed7e8640b1d69b2d63f847d940b5ad688b1` (16 October). Development-only behavior is identified below. [Official releases](https://mixxx.org/download/)

The REA skill routes complete source repositories to normal source inspection. No Mixxx application was present in `/Applications/Mixxx.app` or `~/Applications/Mixxx.app`; this is a bounded search, not a claim that no other installation exists. No Mixxx binary was downloaded, decompiled or executed. Source evidence establishes implementation, not measured runtime quality. The retained [identity manifest](../artifacts/mixxx-learning-2026-10-19/upstream-evidence.json) records SHA-256 identities and pinned URLs for 14 inspected files at each revision. There are no REA binary Evidence IDs because no binary provider was used.

## Architecture: useful boundaries

The audio device drives `EngineMixer`, which processes channels and produces main, headphone, booth and recording/broadcast mixes. Deck playback passes through `EngineBuffer` and its rate, cue, loop, quantize and sync controls. `ReadAheadManager` handles source traversal across loops/seeks and translates scaler consumption back to source frames. `CachingReader` and its worker decode chunks outside the audio callback, using bounded caches and hints around likely future play/cue locations. This separates the file clock from variable-rate audio output. [Stable mixer](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/engine/enginemixer.cpp), [stable reader](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/engine/cachingreader/cachingreader.h), [player architecture](https://github.com/mixxxdj/mixxx/wiki/Developer-Guide-Engine-Player)

PulseDeck already shares several of these principles: off-thread decoding, immutable RAM assets, bounded native commands, source/grid/control revision guards and render-owned scheduling. Its main resource difference is decoding complete sources into RAM before use. A five-minute stereo float source at 48 kHz occupies about 110 MiB before analysis and extra processor state. Mixxx's chunk cache is a useful future streaming design, especially for long files and more decks. Replacing PulseDeck's RAM model now would involve seek/loop prefetch, cache-miss recovery, provider ownership, analysis sharing and real deadline benchmarks; it is not a small decoder substitution.

### Tempo processing and synchronization

Mixxx puts interchangeable scalers behind a common interface. Stable 2.5.6 exposes SoundTouch and, when compiled in, Rubber Band Faster/Finer choices. Rate/pitch, input-frame accounting, latency and seek resets remain engine responsibilities. Development main also contains an additional Rubber Band short-window choice; do not present that as a stable 2.5 feature. [Stable scaler choices](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/engine/enginebuffer.h), [development choices](https://github.com/mixxxdj/mixxx/blob/44dc9ed7e8640b1d69b2d63f847d940b5ad688b1/src/engine/enginebuffer.h)

Mixxx sync has leader/follower roles and an internal clock, including dynamic-tempo operation. PulseDeck already follows nonuniform detected grids with bounded rate correction and checks future passage drift. The remaining lesson is explicit policy: expose the tempo leader and distinguish a steady target from following a variable source. Neither product can turn an erroneous beatgrid into correct synchronization by enabling key lock. [Mixxx sync source](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/engine/sync/enginesync.cpp), [documented dynamic sync](https://manual.mixxx.org/2.5/en/chapters/djing_with_mixxx.html#sync-lock-with-dynamic-tempo)

PulseDeck already vendors MIT-licensed Signalsmith Stretch and uses it through `native/stretch.cpp` and `processor.rs`; it is not implementing time stretching from scratch. There is no evidence here that switching libraries alone would fix timing, poor song choices or missing successor preparation. Those are orchestration and evidence problems. A replacement would require equal-condition transient/vocal/bass listening comparisons, CPU-tail measurements and seek/loop stress tests on actual local songs.

### Effects and routing

Mixxx separates built-in and optional Audio Unit/LV2 backends from effect-chain routing. Engine messages keep heap ownership outside the callback. Its main and headphone paths process effects with different fader context, allowing cue preview and post-fader main processing. This supports useful echo/reverb tails even as the dry source is faded. [Stable backend registration](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/effects/backends/effectsbackendmanager.cpp), [message ownership](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/effects/effectsmessenger.h), [routing](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/engine/enginemixer.cpp)

PulseDeck currently renders deck echo/reverb inside `Deck::render`, then applies channel/crossfader gain in `Mixer::next_frame`. Closing that fader also closes the tail. This is the concrete blocker for honest `echo_out` and `reverb_exit` recipes. The capability registry already marks those unavailable; enabling labels without a separate tail bus would be misleading. A future implementation needs dry/send/return separation, source-independent bounded tail lifetime, limiter/headroom and cue routing, then native recipe guards and real-song renders. Merely adding another effect algorithm is insufficient.

## What Auto DJ actually does

Stable Auto DJ is a queue controller with explicit idle/loading/fading/disabled states, source-load callbacks and transition thresholds. Its four modes use full intro/outro, an outro-start fade, a fixed full-track fade or a fixed fade that skips silence. Development main adds a centered-start skip-silence mode. It loads the next source, skips unusably short tracks and deals with failed/ejected sources through queue progression. Transition calculations avoid half-loaded metadata and check the *following* transition, not only the immediate overlap. [Stable Auto DJ interface](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/library/autodj/autodjprocessor.h), [stable implementation](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/library/autodj/autodjprocessor.cpp), [queue controls](https://manual.mixxx.org/2.5/en/chapters/library.html#auto-dj-automate-your-mix)

Its crossfade progress is updated from source position callbacks. The inspected `playerPositionChanged` path writes crossfader progress; it is not evidence of a general sample-accurate recipe scheduler. PulseDeck should retain its native sample-clock launches and ramps. Mixxx provides strong operational lessons without proving advanced phrase, vocal, energy or genre-aware autonomous selection. The inspected Auto DJ path contains no general learned technique chooser. Uninspected plugins or external scripts are outside that negative claim.

The practical architecture for PulseDeck is therefore:

1. A person or agent chooses a collection, energy direction and acceptable option.
2. A shared planner returns evidence, timing, uncertainty and successor runway.
3. One accepted app-owned move prepares and validates the source, then arms native execution.
4. Native receipts drive lifecycle feedback; a human or agent decides the next track after completion.
5. A future opt-in queue owner can prepare the following source immediately when a deck is released, skip bounded source failures and request a decision before the remaining runway expires.

Step 5 is a remaining product gap. `mix.session.start` currently owns the duration endpoint, not repertoire or queue advancement. Silently granting an individual move permission to choose a different song would change the user's decision contract. A durable queue should show ordering, source readiness, failure/skip reasons, cancellation and takeover, and use the same move executor as Suggest. It should not create a second fader controller.

## Analysis and library

Mixxx has separate silence, waveform, beat, key and gain analyzers, scheduled on worker threads. Its silence analyzer records first/last PCM sound against a fixed −60 dB amplitude threshold; intro/outro cues are distinct editable musical boundaries. Its EBU R128 analyzer uses libebur128 to estimate integrated loudness and derive ReplayGain relative to −18 LUFS. These are distinct measurements with different uses. [Silence analysis](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/analyzer/analyzersilence.cpp), [loudness analysis](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/analyzer/analyzerebur128.cpp), [analysis scheduler](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/src/analyzer/trackanalysisscheduler.cpp)

PulseDeck's versioned analysis cache, content fingerprints, neural rhythm and indexed scoped library already avoid several common pitfalls. Its current useful-audio endpoint is inferred from 4096 band-energy bins. This is a conservative planning approximation, not a sample-accurate silence marker. TrackAnalysis also has no integrated LUFS/true-peak field or validated automatic loudness matching. The master protector limits overload; it does not normalize perceived loudness. These are worthwhile analysis additions, independently versioned and computed off-thread without replacing confirmed user cues.

## Prioritized gaps and delivered improvements

The unassisted Spotify observation corroborates the lifecycle defect in two forms: a terminal invalidation followed by an actual arm, and `mix-1982-32` (Keanler — Falling) becoming ready at `11:51:09.347855 UTC`, invalidated about 4 ms later, then failed with `not_ready: Mix is not ready to arm`. The second form prevented native submission entirely. This evidence comes from the unchanged running build, recorded by the root observer in [the live event log](../artifacts/single-call-moves-2026-10-19/luna-new-live-observation.jsonl); the investigation did not control those decks. The regression exercises the same metadata-free transport shape with both minimal and actual local-song snapshots.

| Priority | Evidence-backed issue | Result or implementation boundary |
| --- | --- | --- |
| P0 | Feedback compared full prepared metadata to metadata-free fast transport, falsely emitting terminal invalidation | **Fixed.** Compare authoritative load/grid/seek/deck/routing revisions in fast feedback. Full identity checks remain before arming. |
| P0 | Native submission began while the operation still appeared ready; an observer could invalidate it before the arm receipt | **Fixed.** Reserve `arming` before native submission and keep the same operation identity through acknowledgement. Capture feedback transport after locking preparations. |
| P1 | A paused deck's direct candidate bypassed session play-history filtering and could suggest the just-played carrier | **Fixed.** Both preloaded and indexed candidates share the successor/history predicate. Clearing history restores intentional replay eligibility. |
| P1 | No app-owned repertoire queue advances past source failures while the agent is occupied | **Remaining.** Explicit queue state, early successor prep, bounded skips, takeover and deadline feedback as described above. Existing single-move fallback stays within the chosen song. |
| P1 | Waveform-bin useful end is approximate | **Remaining.** Persist independently versioned PCM audible bounds with threshold and source fingerprint; use them for runway while retaining conservative fallback for old analysis. |
| P1 | Loudness jumps can survive close BPM and key matches | **Remaining.** Evaluate libebur128 or an equivalent maintained EBU R128 implementation; expose measurement and bounded gain/headroom policy before automatic application. |
| P2 | Echo/reverb exit recipes lack independent post-fader tails | **Remaining.** Separate dry/send/return lifetime and route cue/main correctly; validate tail continuity and clipping before enabling recipes. |
| P2 | Whole-file RAM preparation has memory/load latency proportional to duration | **Remaining.** Benchmark actual file lengths first; chunked decode/prefetch requires a coherent engine change, not a wrapper. |
| P2 | Limited effect/plugin and controller ecosystems | **Remaining.** Mixxx offers broader mappings, samplers and optional AU/LV2 integration. Prioritize concrete hardware/effects needs; do not port an entire framework speculatively. |
| P3 | Four-deck/DVS/broadcast workflows | **Remaining parity items.** Mixxx documents them; this bounded inspection did not establish runtime quality or prioritize them above reliable two-deck transitions. |

The three delivered fixes use original PulseDeck code and existing interfaces; no Mixxx implementation was copied. A new abstraction layer, engine replacement, dependency and live configuration change were unnecessary.

## Adoption constraints and recommendation

Mixxx is GPL v2-or-later. Embedding or adapting its implementation is a licensing/product-distribution decision, distinct from learning design boundaries. Rubber Band offers GPL or a commercial license. Signalsmith Stretch is already MIT-licensed in this repository. libebur128's own license is MIT, making it a concrete candidate for the loudness gap, with normal build/ABI and redistribution review. Optional plugin hosts also need deterministic latency and failure isolation testing. [Mixxx license](https://github.com/mixxxdj/mixxx/blob/3ebac449e7e5fe2a0186596657696e87ce8b0e56/LICENSE), [Rubber Band license](https://breakfastquay.com/rubberband/license.html), [Signalsmith](https://github.com/Signalsmith-Audio/signalsmith-stretch), [libebur128 license](https://github.com/jiixyj/libebur128/blob/master/COPYING)

Recommended order: ship and verify the lifecycle/history fixes; add an explicit successor queue; add PCM bounds and EBU R128 measurements; implement post-fader tail routing; then benchmark alternative scalers or streaming architecture against a concrete quality/resource shortfall. Each stage reaches the UI, control API, skills and offline actual-song fixtures through the shared executor.

## Verification and limits

Focused checks cover fast transport with absent metadata, changed load/grid/seek/deck/routing/engine revisions, expired launch, stopped audio and session-history eligibility. The CLI fixture includes `arming` and confirms one execute submission followed only by lifecycle waits. Actual local snapshots use **Monolink — Return to Oz (ARTBAT Remix)** and **Andrew Meller — Born Slippy (Reincarnation Mix)**; files are verified present, metadata-free transport remains valid and played-source rejection is checked alongside native-window/runway revalidation. These tests open no audio device and touch no live IPC or library/provider state.

The running Spotify observation was left unchanged. Source tests do not prove that the running older binary contains these fixes. No Mixxx binary behavior, listening comparison, new loudness measurement, streaming benchmark or additional mix technique was claimed as validated. Detailed commands/results are retained in `artifacts/mixxx-learning-2026-10-19/validation.json`.

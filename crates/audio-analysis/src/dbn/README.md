# BSD DBN decoder provenance

`decoder.rs` is a targeted copy of
`creightonlinza/madmom-beats-port` revision
`b5052828507dfee6f5329a9f1a9852732ac38990`, file
`rust/madmom_beats_port_core/src/dbn.rs`. It is derived from madmom's BSD-2-Clause
source. Original author and JKU/OFAI notices are retained in LICENSE/NOTICE and
copied to packaged model resources. No upstream BLSTM model, weights, feature
extractor or complete analysis entry point is included.

Local changes: imports point to a minimal supporting type/config module;
port-specific ±1-frame/parabolic refinement is disabled so corrected DBN event
indices remain at the original 50 Hz frame. Its optional per-event confidence is
discarded in favor of Beat This!'s separate, uncalibrated model peak scores.
Decoder algorithm, state space, observation/transition models and blocked
Viterbi path recovery are otherwise retained.

Defaults: 50 fps; competing **whole-track** 3/4/5/7 meter hypotheses; 55–300 BPM;
60 tempo candidates; transition lambda 100; observation lambda 16;
activation threshold 0.05; event correction enabled. This improves temporal and
bar consistency but constrains meter and tempo. It does not infer arbitrary
changing meter, and music faster than the tempo range may select half tempo.
Selected events retain nonuniform source-time peaks. PulseDeck then applies its own bounded ±10 ms beat-logit refinement; downbeats reuse the adjusted beat timestamps. The upstream decoder itself remains frame-based.

Viterbi uses 256-frame traceback blocks. Default 4-beat state space has 5,980
states, so one u32 traceback block is about 6.1 MB, instead of a whole-track
frame-by-state matrix. Boundary score checkpoints, activation densities and final
path still scale with duration (~1.1 MB of f64 score checkpoints per 120 seconds
for the four-beat hypothesis, ~67 MB for two hours). This is not constant total
memory. A forward pass and per-block replay recover the exact best path without
independently resetting at each block. Inference remains outside the render
callback and serialized by the neural model's mutex.

PulseDeck expands the upstream 55–215 range to 55–300 BPM for DJ material. The
215 cap selected 75.949 BPM on the fast swing validation example; 300 recovered
227.488 BPM without changing any events on the three selected EDM previews.
Python madmom configured with the same 300 cap matched all four Rust event sequences
within 10 microseconds of f32 timestamp rounding. This is decoder parity, not
independent beat accuracy.


Analysis version 11 additionally compares an independent one-beat DBN clock with
beat-synchronous mel recurrence at periods 2–8. Sustained distinctive odd-meter
evidence can replace a conflicting neural meter; exceptionally strong repeated
3/4/5/7 patterns also qualify. Integer-multiple repetition is treated as an alias.
This is a whole-source hypothesis, not a meter-change tracker. The thresholds are
heuristics, and `bar_evidence` records their uncalibrated evidence.

For qualifying repeated material, a half-beat phase replacement requires stable
tempo and agreeing bass/mid transient evidence on at least 75% of events. Bar
phase can use recurring bass accents, falling back to neural downbeat evidence.
This guard still misses offbeat locks in independently generated 4/5-beat audio;
see the labeled validation report. Bass accents do not universally establish
musical downbeats.

Half-pulse passages are reconstructed only with acoustic bar corroboration or a
moderate/weak slow-bar pattern bracketed by the dominant pulse (both flanks within ±15%, at least 60% of source intervals at the dominant level). Only individual 1.6–2.4× intervals are split; already normal intervals in boundary blocks remain untouched. A strong slow-bar pattern
preserves an intentional slowdown. Inserted events are midpoints of detected
intervals, explicitly recorded in `pulse_repairs`; they are not fresh neural
observations. Corrections retain review warnings. The fixture test checks both
missing pulses and a real tempo halving. Real variable-meter/tempo material
remains a limitation.

The earlier Rust/Python DBN parity measurements above apply to the unrefined
same-activation 3/4 decoder. They do not validate these additions. Current
accuracy and regressions are in
[the labeled validation report](../../../../research/labeled-rhythm-validation.md).

Published bar recurrence/windows describe the independent clock before pulse
reconstruction, after any guarded phase replacement. `pulse_repairs` then
describes modifications to that clock; recurrence is not recalibrated confidence
for the final repaired timeline.

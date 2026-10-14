# Offline musical analysis, version 11

`analyze_neural_rhythm` additionally runs the pinned full Beat This! `final0` model
through a pure Rust RTen runtime. It returns complete beat/downbeat timelines,
median BPM, separate uncalibrated model activation scores, provenance and elapsed
analysis time. Missing assets, silence and invalid/fewer-than-four event results
return an explicit error. Default BSD DBN postprocessing selects competing
whole-track 3/4/5/7 meter hypotheses at 55–300 BPM; it preserves nonuniform selected
event times but does constrain meter/tempo. Concave beat-logit maxima are refined by at most half a frame (10 ms); all downbeats share the same refined beat events. This improves measured timing without claiming sample precision. No madmom model weights are used.
Independent acoustic bar recurrence can override a conflicting meter on sufficiently supported material. Explicit half-pulse reconstructions and guarded phase replacements remain visible in `bar_evidence` and require review. These heuristics have measured failures; see [labeled validation](../../research/labeled-rhythm-validation.md).
Benchmark-only minimal mode leaves raw peak timing unconstrained. See
[model setup and limits](../../src-tauri/resources/beat-models/README.md).

This crate implements independent public DSP for decoded stereo PCM. It does not
load rekordbox models, call proprietary analysis libraries, read provider tokens,
write PCM, or run inside the native render callback.

Tempo uses positive log-spectrum novelty from bass, mid and treble bands, local
background subtraction, short temporal smoothing, normalized autocorrelation and
harmonically weighted lag scoring. Up to three 24-second excerpts are distributed
through a longer track. A fractional BPM search refines integer lag resolution;
segment support contributes to the heuristic confidence. Support is measured near
the aggregate winner relative to each segment's strongest local candidate; different
local accent/metre winners lower confidence and flag ambiguity instead of vetoing
a credible aggregate estimate. Cubic correlation interpolation reduces integer-lag
bias before the fractional BPM search. When a two-beat period
and its shorter fundamental have virtually equal support, the shorter supported
period is preferred and the alternatives remain visible. This cannot establish
which metrical level a human or vendor considers the beat. Candidates and
`bpm_ambiguous` expose that choice rather than concealing half/double tempo aliases.

Key uses up to 64 distributed 8192-sample windows at 11,025 Hz. Spectral peak
interpolation, a global tuning estimate, compressed locally whitened peak salience
and octave folding form pitch-class profiles. Major/minor tonal-hierarchy
correlation and temporal support produce the displayed candidate and confidence.
Uncertain, chromatic, single-tone, silent and noise input can return no key while
keeping useful alternative candidates. The profiles are conventional
Krumhansl/Kessler values. Neither confidence nor candidate score is a calibrated
probability, and this is not a learned key model.

Waveform analysis scans the actual PCM once. A stereo peak envelope and 4096
aligned low/mid/high RMS bins share one amplitude scale. Independent second-order
Butterworth filters use LP 300 Hz, HP 250 Hz → LP 1200 Hz, and HP 1200 Hz → LP
9000 Hz, with Q = 1/√2. These frequency ranges were informed by the project's
static REA findings; the coefficient choice and implementation are ours and
have not been validated against a running vendor application. The bands overlap.
Changing waveform colors does not affect audible mixer EQ.

Beat phase uses attack envelopes from low, mid and high bands, with bass
emphasis. One FFT per excerpt splits the bands with zero-phase complementary
second-order Butterworth magnitudes and forms each band's analytic signal; its
squared magnitude is band energy without carrier ripple. Positive log-energy
steps between 5 ms frames, timestamped at the frame boundary, mark where attacks
start, and a symmetric 0.5 s local mean removes the background of small steps
from sustained notes and noise. Nothing in this path delays the envelope.

Earlier versions used causal 5 ms RMS rises. A 5 ms window is shorter than one
cycle of a 40-60 Hz kick body, so it reported a rise on every body cycle; on
real kicks, whose sub builds for tens of milliseconds and sustains, most of that
rise mass sat 80-130 ms after the attack and the grid followed it. Short, fast
decaying synthetic kicks did not show the effect.

Circular folding against the detected period includes each excerpt's absolute
sample-time offset, rejects weak periodicity, and checks agreement across
excerpts. An excerpt agrees when it supports a beat within 25 ms of the chosen
phase, and a strict majority must agree: BPM rounding alone (0.02 BPM at 112
BPM) drifts sharp attack phase 27 ms over 150 s, which is not conflicting
evidence. Concentration is measured inside each 24 s excerpt for the same
reason. With a slightly wrong BPM, the grid sits on the beats mid-track and the
drift grows toward the ends. The stored offset is a phase
origin modulo one beat period, extrapolated against track zero; it does not claim
that a beat was audible during a silent intro, identify the musical downbeat, or
infer the actual first bar. A four-beat display is relative to this grid origin.
Changing-tempo or inconsistent-phase material can have no uniform phase estimate.
Confidence is heuristic. Real music and syncopated bass need independent beat
annotations and listening checks before any fidelity/parity claim.

Version 2 cached BPM/key/waveforms remain usable when their audio identity,
sample rate, duration and shapes match. Loading their actual PCM computes only
the new phase and advances the record to version 3. A manually supplied phase is
preserved. Version 3 records written by the RMS-rise estimator are reused as
they are, so an analysed grid origin cached before the attack-envelope change
can still sit 80-130 ms late until that track is re-analysed. Manual grids are
kept either way. BPM, key and colored waveform analyses are not needlessly
recomputed.
The phase provenance field distinguishes analysis, tag-tempo reference, manual
input, and known synthetic-source phase. Unknown phase remains null.

Embedded, valid BPM tags are preserved with `bpm_source = "tag"`; an estimated BPM
uses `bpm_source = "analysis"`. Track reanalysis updates cached display metadata
from the existing RAM source without resetting play state, transport position,
cues, manual native BPM or beatgrid. Service recording restrictions survive.

Validation uses fractional rhythm fixtures at 65.2, 93.7, 127.3 and 174.5 BPM;
quarter/eighth metrical aliases; delayed rhythm and antiphase stereo; detuned,
harmonic major/minor chords and a chord progression; unpitched noise, silence,
chromatic content; and measured low/mid/high frequency dominance. Shifted fractional pulses, loud offbeat hi-hats,
85-second distributed excerpts, click-plus-sub kicks with a slow sustained sub,
and pitch-swept kicks under a sidechained bass with offbeat noise hats recover
known fixture phase within 8 ms; a 0.05 BPM tempo error over 200 s keeps the
grid within 8 ms mid-track. Silence, noise and inconsistent section phase do not
invent a grid. Engine tests
also cover ID3 tempo provenance, provider RAM decoding, unchanged reanalysis
transport state and zero callback allocation/deallocation. Synthetic checks do
not establish accuracy across commercial music or parity with rekordbox; use
held-out tracks with independently verified references for that assessment.

## Neural beat stage

`track_beats` uses the existing pinned full Beat This! final0 model and RTen runtime. `stage_from_rhythm` derives compatibility metadata from an already analyzed `NeuralRhythm`, without a second inference pass. The optional constant-grid fit is diagnostic only: it never replaces the native beat/downbeat event clock, BPM provenance or manual edits. The application derives section features from its decoded RAM audio. The incoming small-model/tract spike remains historical research; its weights and runtime are not shipped.

## Song sections and automatic cues

`detect_sections_on_bars` cuts sections on actual native bar timestamps, including variable tempo and 3/4/5/7 meter. A section spectrogram supplies the timbre and energy features. `detect_sections` remains a constant-grid compatibility helper. Every
full bar from the grid's beat 1 gets 16 timbre bands plus overall, bass and
treble level and bass onset strength. Silent bars at either end are dropped.
Dynamic programming over the z-scored bar vectors minimises within-section
variance plus a cost per section; lengths in whole 8-bar phrases are free, 4-bar
sections and odd lengths cost more, and fragments under four bars at the ends
join their neighbour. Sections are only cut from model or manual grids: the DSP
phase marks a beat, not beat 1.

Labels use each section's energy, groove (bass level and how far bass stands
out from the mix, so a noise riser does not read as a drop) and repetition.
**Dance** (chosen when bass drops out mid-track and later returns): drops are
loud with strong groove, breakdowns lose the groove, build-ups lead into a drop
and either rise or are short and lighter, and plain phrases before the first or
after the last labelled section extend the intro and outro. **Hip-hop**: short
edge sections are the intro and outro, the repeating group of roughly 8-bar
sections is the hook, and longer sections are verses. The DJ can switch
vocabulary per track without re-reading audio. Neighbouring sections with the
same label display and cue as one; the stored sections stay fine-grained.

`plan_auto_cues` fills only empty hot cue pads (first downbeat, then drops or
hooks, breakdowns or verses, build-ups and the outro, placed in time order) and
memory cues (every section start). It records what it placed: untouched auto
cues move when sections change, cues the DJ set or moved are never changed, and
auto cues the DJ cleared are not placed again on any pad.

Tests cover a synthetic EDM arrangement (intro, build-up, drop, breakdown,
build-up, drop, outro cut on exact phrases), a synthetic hip-hop arrangement
(intro, verse, hook, verse, hook, outro), silence, label extension and merging,
and the cue rules. Real tracks need the annotated evaluation in
`research/ai-features-scope.md`.

Public DSP references:

- [Ellis, Beat Tracking with Dynamic Programming](https://www.ee.columbia.edu/~dpwe/pubs/Ellis06-beattrack.pdf): onset strength and tempo autocorrelation.
- [librosa onset strength documentation](https://librosa.org/doc/0.11.0/generated/librosa.onset.onset_strength.html): positive spectral difference and frequency-local maximum references.
- [Essentia KeyExtractor documentation](https://essentia.upf.edu/reference/streaming_KeyExtractor.html) and [tonal-analysis tutorial](https://essentia.upf.edu/tutorial_tonal_hpcpkeyscale.html): spectral peaks, pitch-class profiles and tuning compensation. This project does not depend on or copy Essentia implementation code.
- [W3C Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/): independent biquad coefficient formulas.
- [RustFFT](https://github.com/ejmahler/RustFFT): MIT/Apache-2.0 FFT implementation used off-thread.

# Offline musical analysis, version 3

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

Beat phase uses causal 5 ms positive energy-rise envelopes from low, mid and high
bands, with bass emphasis. Unlike the future-looking spectral frame used for BPM,
these envelopes preserve attack timing. Circular folding against the detected
period includes each excerpt's absolute sample-time offset, rejects weak
periodicity, and checks agreement across excerpts. The stored offset is a phase
origin modulo one beat period, extrapolated against track zero; it does not claim
that a beat was audible during a silent intro, identify the musical downbeat, or
infer the actual first bar. A four-beat display is relative to this grid origin.
Changing-tempo or inconsistent-phase material can have no uniform phase estimate.
Confidence is heuristic. Real music and syncopated bass need independent beat
annotations and listening checks before any fidelity/parity claim.

Version 2 cached BPM/key/waveforms remain usable when their audio identity,
sample rate, duration and shapes match. Loading their actual PCM computes only
the new phase and advances the record to version 3. A manually supplied phase is
preserved. BPM, key and colored waveform analyses are not needlessly recomputed.
The phase provenance field distinguishes analysis, tag-tempo reference, manual
input, and known synthetic-source phase. Unknown phase remains null.

Embedded, valid BPM tags are preserved with `bpm_source = "tag"`; an estimated BPM
uses `bpm_source = "analysis"`. Track reanalysis updates cached display metadata
from the existing RAM source without resetting play state, transport position,
cues, manual native BPM or beatgrid. Service recording restrictions survive.

Validation uses fractional rhythm fixtures at 65.2, 93.7, 127.3 and 174.5 BPM;
quarter/eighth metrical aliases; delayed rhythm and antiphase stereo; detuned,
harmonic major/minor chords and a chord progression; unpitched noise, silence,
chromatic content; and measured low/mid/high frequency dominance. Shifted fractional pulses, loud offbeat hi-hats and
85-second distributed excerpts recover known fixture phase within 8 ms; silence,
noise and inconsistent section phase do not invent a grid. Engine tests
also cover ID3 tempo provenance, provider RAM decoding, unchanged reanalysis
transport state and zero callback allocation/deallocation. Synthetic checks do
not establish accuracy across commercial music or parity with rekordbox; use
held-out tracks with independently verified references for that assessment.

## Neural beat stage

`track_beats` runs the bundled Beat This! small model (`models/README.md`) over
decoded PCM: mono mix, a single-block FFT resampler to 22,050 Hz, 128-band
log-mel at 50 fps, 1500-frame chunks, then the upstream local-maximum peak
picking. It returns beat and downbeat times plus a constant-grid fit: BPM is
searched within ±4% of the median beat interval for the phase that puts the
most beats within 35 ms of a line, refined by least squares, and the offset is
placed on the bar phase most downbeats agree with. The grid is used only when at
least 16 beats exist and half of them sit on it.

Inference runs on a four-thread utility-priority pool, one track at a time
(about 250 MB transient), at roughly two seconds per minute of audio on an M3
Pro. It never runs in a render callback. The stage is versioned separately
(`BEAT_MODEL_VERSION`) so DSP analysis version 3 stays valid. Tests check the
log-mel front end against torchaudio reference values, chunking against the
upstream split, the grid fit on jittered beats with gaps, and an end-to-end run
on a synthetic drum loop. Agreement with the reference implementation is not an
accuracy benchmark; use `deck-engine`'s `beat_eval` example on hand-checked tracks.

## Song sections and automatic cues

`detect_sections` cuts a track into sections on its grid, reusing the beat
model's log-mel spectrogram, so it adds well under a second per track. Every
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

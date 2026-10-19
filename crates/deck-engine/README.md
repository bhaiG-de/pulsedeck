# Native deck engine

`deck-engine` owns decoding, the two deck transports, mixing, controller timing,
headphone routing, effects, sampler playback, and WAV recording. Tauri submits
commands and reads snapshots; the webview never schedules audio samples.

The CoreAudio stream belongs to a lifecycle thread. Its callback acquires
exclusive ownership of one preallocated DSP state through an atomic mailbox.
CPAL's stream destruction stops the native AudioUnit before the lifecycle thread
reuses the state. Control commands arrive through a bounded SPSC queue. Removed
PCM assets and recording producers move to a separate retirement queue, so the
callback never releases their final ownership. The test allocator verifies no
allocation, reallocation, or deallocation while rendering and replacing a track.

Implemented:

- Symphonia decoding of supported WAV, AIFF, MP3, FLAC, AAC, ALAC and other enabled
  formats; metadata, 4096 waveform peaks and aligned actual low/mid/high RMS bands,
  spectral tempo estimates with metrical alternatives, and tuned tonal-key estimates.
- Independent sample-frame deck clocks, cue preview, 8 hot cues, 10 memory cues,
  manual/grid loops, beat jumps, tempo, touch-held jog scratching and jog bend.
- BPM/phase sync with a selectable master, stereo mixing, trim and channel faders,
  smoothed three-band EQ, low/high-pass tone filters and constant-power crossfade.
- Separate master and headphone buses, pre-channel-fader cue selection and cue/master
  mix, with explicit zero-based output-channel maps. Four physical output channels
  are needed for independent stereo headphones and master.
- Continuous Signalsmith Stretch keylock and independent semitone pitch shift.
  Fixed native buffers process 64 output frames at a time; sync/tempo automation
  changes processor state without rebuilding an entire track or switching to
  pitch-changing original audio. Source clocks remain authoritative and declared
  FFT latency is compensated by pre-rolling future decoded RAM.
- 96-tap, interpolated polyphase windowed-sinc reads for deck/sample playback,
  sample-rate conversion and reverse scratching, with precomputed anti-alias banks.
- Six dB of bus headroom and independent stereo-linked master/headphone lookahead
  protection. Protected output has a -1 dBFS sample ceiling and 128 frames of common
  output delay (2.67 ms at 48 kHz). Float WAV receives the protected master tap.
- Feedback echo, multitap reverb, eight original synthesized one-shot samples and
  off-thread sample-file loading.
- Float32 stereo WAV recording from the limited master output. A four-second ring
  feeds a file writer; dropped frames and writer errors are visible in recording status.

## Limits that affect parity

`EngineHandle::load_source(deck, Box<dyn EncodedAudioSource>, AudioSourceInfo)`
accepts an integration-owned Read/Seek source and decodes it on the calling worker,
using the same native asset/transport path as local files. Source identity contains
display metadata only; credentials stay in the provider. A fragmented byte-reader
fixture verifies native decode, waveform and seek without a filesystem path.
This entry point decodes the complete source into RAM. It supplies no Spotify
SDK, stream authorization, capability enforcement or playback reporting; those
must be implemented in a real provider before service tracks can be loaded.

The DSP algorithms are independent implementations; EQ, effects, key analysis,
beat detection and stretching have not been calibrated against rekordbox.
Analysis references and uncertainty are documented in
[`audio-analysis/README.md`](../audio-analysis/README.md). Native time/pitch DSP
uses pinned MIT upstream headers with license notices in [`vendor/`](vendor/).
Source PCM still resides in RAM, but there are no full-track prepared copies,
cache-generation worker, or asynchronous tempo/pitch readiness delay.

Protection is a sample-peak limiter with conservative margin; it does not promise
lossless redlining, reconstruct already clipped sources, or guarantee pathological
intersample peaks. Comparative polyphonic quality and attached-hardware listening
remain measurements, not vendor-parity claims. EQ and effects remain independently
implemented. The snapshot's historical `underruns` field counts CPAL stream/device
errors, not hardware underruns. `audio_quality` separately reports pre-protection
peaks, gain reduction, limited/invalid frame counts, processor blocks/resets,
callback timing/deadline exceedances and added output delay. Existing master/cue
peaks show protected output. Callback timing is a software deadline measurement.
Recording uses classic float WAV and stops with an error before 4 GiB.
A recording filename must be new to avoid silently overwriting an existing file.

## Validation

From the workspace root:

```sh
bash tools/rust-env.sh cargo test -p deck-engine
bash tools/check-audio-allocations.sh
bash tools/rust-env.sh cargo test -p deck-engine benchmark_two_continuous_keylocked_decks -- --ignored --nocapture
bash tools/rust-env.sh cargo clippy -p deck-engine --all-targets --no-deps -- -D warnings
bash tools/rust-env.sh cargo run -p deck-engine --example native_smoke
```

Tests exercise decoded WAV input, cue/loop/EOF timing, sync and rate math,
headphone independence, source retirement, bounded callback allocation behavior,
touch scratch, sampler output, changing-rate keylock, independent pitch, latency-compensated impulses,
passband/alias rejection, linked limiting, protected output diagnostics,
and recorded-WAV decoding. The repeatable native smoke runs both decks with the master muted, checks native
clock progress and stop/restart continuity, records and decodes a valid silent WAV,
and reports its CoreAudio device/configuration. It passed locally on MacBook Pro
Speakers at 48 kHz, with a requested 256-frame buffer and zero reported stream errors.
Controller round-trip behavior and comparative latency
still require the actual DDJ-FLX4 and an independent audio capture.

## Programme level and independent effects returns

Deck loads apply the cached/full-PCM programme correction before EQ/FX, separately from the user trim. `set_normalization {deck,enabled}` smoothly bypasses/re-enables it; the mode persists across loads on that deck. Snapshots expose `normalization_enabled` and `normalization_gain_db`. Legacy sources receive current measurements during decoded-source preparation.

Echo and reverb use fixed-capacity main and headphone send/return buses. The main sends follow both channel fader and crossfader; the headphone sends are pre-fader. Closing sends, pausing, replacing or unloading a source leaves existing return audio to decay, bounded to eight seconds after the final nonzero send. Returns remain under master gain and output protection. `fx_tail_active`, `fx_tail_peak` and `restricted_tail_active` expose main-return state; service-source tails retain recording restrictions after source replacement until expiry. No allocations or source dereferences are needed to sustain a retired source's return.

Echo/Reverb are native ramp targets with guarded endpoints and recovery; existing target indices are preserved and old snapshot arrays are migrated on read. The assisted `echo_out` and `reverb_exit` recipes close their sends while clearing the dry carrier, leaving the bounded return independent of subsequent source preparation.

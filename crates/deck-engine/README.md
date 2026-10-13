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
- Worker-prepared WSOLA keylock and independent semitone pitch shift. Preparation
  can be cancelled when the target changes. The original timeline stays authoritative.
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
Analysis version 2 documents its public DSP references and uncertainty in
[`audio-analysis/README.md`](../audio-analysis/README.md). WSOLA
prepares an entire decoded track in memory, so tempo/pitch changes have preparation
latency and can temporarily use varispeed playback while `preparation_pending` is
true. It is not an instantaneous streaming keylock algorithm, and polyphonic quality
needs listening tests. Memory scales with track length and prepared copies; preparation rejects work that
exceeds its 512 MiB temporary cache budget. Varispeed playback uses linear
interpolation and needs calibrated band-limited resampling for fidelity parity. Native
latency depends on the selected CoreAudio device and buffer size. The snapshot's
historical `underruns` field counts CPAL stream/device errors, not measured hardware
buffer underruns. Recording uses classic WAV and stops with an error before 4 GiB.
A recording filename must be new to avoid silently overwriting an existing file.

## Validation

From the workspace root:

```sh
bash tools/rust-env.sh cargo test -p deck-engine
bash tools/rust-env.sh cargo clippy -p deck-engine --all-targets --no-deps -- -D warnings
bash tools/rust-env.sh cargo run -p deck-engine --example native_smoke
```

Tests exercise decoded WAV input, cue/loop/EOF timing, sync and rate math,
headphone independence, source retirement, bounded callback allocation behavior,
touch scratch, sampler output, tone-preserving stretch duration, cancellation,
and recorded-WAV decoding. The repeatable native smoke runs both decks with the master muted, checks native
clock progress and stop/restart continuity, records and decodes a valid silent WAV,
and reports its CoreAudio device/configuration. It passed locally on MacBook Pro
Speakers at 48 kHz, with a requested 256-frame buffer and zero reported stream errors.
Controller round-trip behavior and comparative latency
still require the actual DDJ-FLX4 and an independent audio capture.

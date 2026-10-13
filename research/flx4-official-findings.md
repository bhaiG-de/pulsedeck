# DDJ-FLX4: official interoperability evidence

Researched 13 October 2026. Sources are AlphaTheta/Pioneer DJ publications. This is documentation research, not a connected-device measurement.

## Driver and audio boundary

**No custom macOS audio driver is required.** AlphaTheta explicitly says DDJ-FLX4 uses the standard Windows/macOS audio driver over USB. The product page calls it class compliant. For a Rust application, reusing macOS audio/MIDI services is the appropriate starting point; writing a replacement kernel driver is not indicated by this evidence. [Driver FAQ](https://support.alphatheta.com/en-us/articles/12410664372121), [product page](https://www.pioneerdj.com/en/product/dj-controllers/ddj-flx4/).

The official manual specifies **44.1/48 kHz**, **16/24-bit A/D and D/A**, stereo RCA master output, 3.5 mm stereo headphone output, microphone input, and two USB-C ports. Printed pages 159–160. [Operating instructions](https://downloads.support.alphatheta.com/manuals/dj-controllers/DDJ-FLX4/DDJ-FLX4_DRI1804A_manual.pdf).

The product page confirms USB audio includes microphone sound and supports recording through the USB connection. The macOS setup FAQ selects DDJ-FLX4 as the audio device and supports PC MASTER OUT duplication. [Product page](https://www.pioneerdj.com/en/product/dj-controllers/ddj-flx4/), [macOS OBS setup](https://support.alphatheta.com/en-us/articles/26128206440601).

**Unknown:** exact CoreAudio input/output stream channel count, USB endpoint numbers, channel order, supported format combinations, current hardware buffer limits, hardware latency and mic/loopback stream identity. Stereo master plus stereo headphones imply two stereo destinations, but are not sufficient evidence to assert a particular four-channel USB descriptor. Enumerate the connected device before implementing routing.

## Public MIDI contract

Official mapping: controls, pads, LEDs, meters, settings. Input = controller → application; output = reverse. Hexadecimal below; `v` = data byte 2. [MIDI list v1.0](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf).

| Function | Published messages |
|---|---|
| Play / cue, deck 1 | `90 0B v` / `90 0C v`; deck 2 `91` |
| Tempo | `B0/B1 00 MSB`, `B0/B1 20 LSB` |
| Channel fader | `B0/B1 13 MSB`, `B0/B1 33 LSB` |
| Crossfader | `B6 1F MSB`, `B6 3F LSB` |
| Smart CFX / Fader | `96 00 v` / `96 01 v` |
| Loaded illumination | `9F 00 7F` / `9F 01 7F` |
| Vinyl setting | `90/91 17 v`; default on |

Buttons usually use `00/7F`. Jog counts center on `40`; browser uses `01/7F`. Preserve MSB/LSB precision. Beat Sync sends on release. Vinyl requires application output. FX LED: NOTE ON blinks, NOTE OFF lights. No SysEx initialization handshake is documented. Validate against hardware.

Apparent inconsistency: LEVEL/DEPTH lists channel 6 but `B4`. Resolve using hardware observations. PDF retained locally.

The official third-party MIDI article recommends one DJ application at a time and power-cycling the FLX4 when switching applications. Its English text contains a DDJ-REV7 copy/paste error; the Japanese version describes this unit and FLX4 switching explicitly. [MIDI-compatible software, English](https://support.alphatheta.com/en-us/articles/12294087060377), [Japanese](https://support.alphatheta.com/ja/articles/12294087060377).

## Smart features and parity scope

The manual describes Smart Fader as enabling mandatory Beat Sync and adapting BPM, EQ LOW and echo to fader position. Equal deck levels target midpoint BPM; outgoing tracks receive echo at the endpoint. Smart CFX chooses software presets and can alter loops and keys as well as effects. Printed pages 70–74. [Operating instructions](https://downloads.support.alphatheta.com/manuals/dj-controllers/DDJ-FLX4/DDJ-FLX4_DRI1804A_manual.pdf).

**Inference:** these are host-integrated performance behaviors, not a standalone mixer feature unlocked simply by sending their button MIDI. Exact DSP algorithms, interpolation curves, preset contents and host/firmware division are undocumented here. Reproduce observed behavior with an independent host engine and verify audio equivalence separately.

The official rekordbox hardware diagram is the controller-specific acceptance checklist: two decks; play/cue; jog scratch/bend/search; tempo/sync/master/range; loop in/out, active loops, halve/double and stored cue/loop recall; instant doubles; browser/tree navigation and waveform zoom; three-band EQ/trim/faders/cue/headphone mix; Beat FX, tap/auto timing and Release FX; Smart modes; sampler; and all eight pad modes—Hot Cue, Keyboard, Pad FX1/2, Beat Jump, Beat Loop, Sampler, Key Shift. [Hardware diagram for rekordbox Mac/Windows](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_HardwareDiagram_rekordbox_E1.pdf).

This list is FLX4 control parity, not every rekordbox product feature. Library analysis/export, stems, streaming, cloud, subscriptions and other controller ecosystems need a separate scope and evidence.

## Performance validation needed

Documentation cannot demonstrate matching performance. Compare both applications on the same Mac, controller, sample rate, buffer size and audio files. Measure MIDI-to-audio response, jog response, cue timing, drift, CPU and memory, waveform rendering, and dropout count under analysis/library load. AlphaTheta's own troubleshooting notes smaller buffers can cause cuts/noise and suitable latency depends on the system. [Audio troubleshooting](https://support.alphatheta.com/en-us/articles/12185302314649).

## Retained references

- `research/flx4-reference/DDJ-FLX4_DRI1804A_manual.pdf`
- `research/flx4-reference/DDJ-FLX4_MIDI_message_List_E1.pdf` and layout text
- `research/flx4-reference/DDJ-FLX4_HardwareDiagram_rekordbox_E1.pdf` and layout text

No firmware flashing, controller initialization or connection-changing actions were performed.

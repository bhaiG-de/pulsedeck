# Waveform playback and cursor seeking

Updated 2026-10-13. This is an independent PulseDeck implementation informed by the observed centered rekordbox performance display in [the UI evidence](rekordbox-performance-waveform.md).

## Causes and changes

The former transport polling path cloned and serialized all loaded analysis, including peak/low/mid/high vectors, on every tick. React therefore received fresh waveform identities on every tick, invalidating display work. Moving waveforms repainted after native polls instead of on display frames.

Native `transport_snapshot` now reads transport, meters, touch state and discontinuity revision from published atoms without taking the track metadata mutex or cloning waveform vectors. `metadata_snapshot` delivers the analysis only when its separately published metadata revision changes. The full snapshot remains available for infrequent initial and mutation refreshes. Controller/import status has a separate lightweight polling command; recurring updates no longer enumerate devices or read the complete library.

The frontend retains immutable analysis references across transport updates. Its bounded animation clock follows native position and rate; it extrapolates for at most 120 ms and treats pause, source changes and explicit position revisions as discontinuities. It caches source-aligned waveform raster tiles and the full-track overview, composites at fractional pixel positions, and keeps beat/cue/loop overlays aligned with the same displayed source time.

Pointer-down holds the current displayed source position immediately and sends native jog touch. Drag updates local display immediately, coalesces movement, and queues absolute seeks in order. Release waits for pending movement before releasing native touch. Playing intent is preserved: a playing deck resumes, a paused deck stays paused. Native seek cancels any previous scratch grain. Beat-sync maintenance respects the touched platter until release.

## Measurements and checks

A release headless benchmark of 100 two-deck snapshot reads plus JSON serialization, with eight 4096-bin waveform arrays, measured:

| Payload | Bytes | Mean native read + serialization |
| --- | ---: | ---: |
| Former full snapshot | 385,640 | 552.80 µs |
| Transport only | 1,471 | 1.37 µs |
| Infrequent track metadata | 384,204 | — |

The transport payload is about 262 times smaller. These figures measure native snapshot work, not end-to-end UI frame rate. Zero allocation/deallocation was checked across 1000 transport reads. Audio callback allocation checks continue to pass. Behavioral fixtures cover held playback, repeated exact seeks, paused dragging, release, loop discontinuities, metadata revisions and touch with beat sync.

A reproducible headless Skia canvas benchmark is in `frontend/src/waveform-render.bench.ts`. Across 180 frames of two 1400×86 detailed graphs and two 500×28 overviews, at DPR 2 with 4096 source bins, the former envelope/rebin/sort painting measured median 15.165 ms and p95 15.508 ms. Cached raster compositing measured median 2.541 ms and p95 2.627 ms, about six times less canvas-layer CPU. Cold first build of the four graphs was 19.471 ms. Neighbor tiles were prewarmed, overview tiles were reused, and each graph retains at most six tiles. This excludes React, native IPC, marker overlays and browser compositing; it does not establish browser FPS.

The updated 16.85 MiB macOS release bundle was launched. Saved Spotify accounts restored after the user approved Keychain. With the real full-length “FALLING FOR YOU” track loaded, a three-second timed pointer hold kept position at 00:36 while playing intent remained true; after release the clock advanced to 00:45. A rightward drag during playback sought 01:06 → 01:04 and preserved playing state. A leftward drag while paused sought 01:04 → 01:06 and remained paused. Detailed playheads stayed centered, and the full overview reflected the new position. The app displayed zero backend stream errors. This live check used the existing master level; it did not measure audible scratch quality, headphone routing, hardware latency or sustained frame rate.

Validation: 87 Rust tests, 29 frontend tests, strict workspace Clippy, TypeScript/Vite production build and Tauri release packaging passed. Hardware latency, sustained frame rate and the full stress acceptance gate remain unmeasured.


## Audible scrub and waveform appearance update

The first optimized drag implementation held the platter and sent silent absolute seeks. It has now been replaced with native `ScrubTo`: each timed pointer update retargets an actual forward/reverse source-PCM traversal from the current audio cursor. Retargeting avoids the lost displacement of overwritten fixed-delta jog grains. Original PCM is read during a scrub even with key lock enabled; normal prepared playback resumes after release. Short attack/tail ramps soften start/stop edges. Stationary holds stop after the movement grain, paused decks emit movement audio and stay paused afterwards, and the other deck continues through the normal mixer. On release native ScrubEnd completes any pending movement at the exact final target before touch-off; even movement and release in one callback batch remain audible. Focus loss also releases the held platter.

DSP fixtures verify nonzero PCM in both directions, sample order, exact trajectory endpoints, continuous retargeting, independent second-deck advance, paused/playing release intent, key-lock source choice, bounds and invalid inputs. The callback allocation audit includes the new scrub command and remains allocation/free-free. No browser audio or network work enters the render callback. This is basic varispeed scrubbing; commercial scratch fidelity and physical controller latency are not established by these tests.

Detailed graph height increased from 86 to 108 CSS pixels. The separate gray peak reference has been removed whenever frequency-band data is available. A fixed per-track display gain, shared by all three bands, expands the genuine colored envelopes while preserving their relative energy; it does not pump during playback. Raster tests check that only blue/orange/white opaque band pixels are emitted, that the colored envelope fills approximately 88% of the fixture height, and that silent bands cannot inherit a ghost peak shape. Raster caching and bounded neighbor prewarming remain in use. A follow-up headless benchmark with 108-pixel details measured about 15.6 ms before versus 3.1 ms with cached compositing; browser FPS remains unmeasured.

Validation after this update: 91 Rust tests, 30 frontend tests, strict workspace Clippy, the frontend production build and the 16.87 MiB Tauri release package passed. The enlarged RGB display without a gray peak outline was inspected in the native app. Forward/reverse drag positions and playing/paused release intent were verified with the generated demo, with zero backend stream errors.

End-to-end capture through the real UI initially exposed a release-order failure: the final absolute seek cancelled all remaining scratch output during short drag gestures. Native `ScrubEnd` now retains the final traversal until its endpoint before clearing touch, with a regression for movement and release in the same callback batch. A repeat UI-driven recording with the generated deck paused throughout captured 3072 nonzero float samples, peak 0.2621 and no clipped samples. Only original generated test audio was recorded, not Spotify. [Audio measurements](evidence/native-audible-scrub.json) and the short generated-tone WAV are retained alongside the zero-output failure evidence. This establishes audible output through the UI-to-CoreAudio/mixer/recording path, not parity with rekordbox scratch fidelity.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

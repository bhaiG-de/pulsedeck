# PulseDeck UI renders

Seven feature views of the real PulseDeck production frontend, rendered with Three.js / WebGL and native browser HTMLTexture on an Apple M5 Pro through Metal. The interface is painted by the browser; it is not manually rebuilt for these images.

| File | Feature |
| --- | --- |
| `01-hero.webp` | Two decks, stacked waveforms and mixer |
| `02-waveforms.webp` | Beatgrid, playhead and cue countdowns |
| `03-deck.webp` | Jog, tempo, transport and hot cues |
| `04-mixer.webp` | EQ, filters, faders and effects |
| `05-library.webp` | Local collection, metadata and tags |
| `06-suggest.webp` | Ranked next moves and transition timing |
| `07-controller.webp` | White DDJ-FLX4-W and Space Black MacBook from a centered, straight-on DJ point of view |

## README delivery

The repository contains seven **1920 × 1080 WebP images**, totaling **991 KB**. The first six were downsampled with Lanczos; the controller frame was rendered natively at 1080p. All are encoded at quality 90, method 6. Fine text and waveforms were visually compared with the downscaled source renders; the images use lossy compression and are intended for normal README display.

Inline images and click-through links use the same 1080p files. The 4K source renders are kept outside the tracked repository.

The sessions use real local track metadata in isolated null-audio profiles. The first six views are paused with staged hot cues; the controller view uses a captured real playback state on the laptop display. The native profile is paused after capture. The equipment connection is an illustration, not evidence of an attached USB/MIDI controller or a completed mix. No audio, account credentials, local library database or private source file paths are included.

The depicted MacBook Pro uses Apple's product-page AR asset for its enclosure, keyboard and display geometry. Apple retains rights to that asset and its marks. The model itself, source scene, private runtime profile and render tooling are not distributed here. The Blender reference informed lighting and camera direction; these images were rendered in WebGL.

The white DDJ-FLX4-W is newly authored geometry and vector legends based on [official multi-angle product references](https://www.pioneerdj.com/en/product/dj-controllers/ddj-flx4-w/). Blender was used for modeling/export; the final image was rendered in WebGL on the Mac GPU. The Space Black MacBook finish is unchanged. Controller source models and high-resolution masters remain local and untracked; no downloaded controller mesh or product photo texture is included.

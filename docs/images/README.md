# PulseDeck UI renders

Six approved views of the real PulseDeck production frontend, rendered with Three.js / WebGL and native browser HTMLTexture on an Apple M5 Pro through Metal. The interface is painted by the browser; it is not manually rebuilt for these images.

| File | Feature |
| --- | --- |
| `01-hero.webp` | Two decks, stacked waveforms and mixer |
| `02-waveforms.webp` | Beatgrid, playhead and cue countdowns |
| `03-deck.webp` | Jog, tempo, transport and hot cues |
| `04-mixer.webp` | EQ, filters, faders and effects |
| `05-library.webp` | Local collection, metadata and tags |
| `06-suggest.webp` | Ranked next moves and transition timing |

## README delivery

The repository contains six **1920 × 1080 WebP images**, totaling **832 KB**. They were downsampled with Lanczos and encoded at quality 90, method 6. Fine text and waveforms were visually compared with the downscaled source renders; the images use lossy compression and are intended for normal README display.

Inline images and click-through links use the same 1080p files. The 4K source renders are kept outside the tracked repository.

The session uses real local track metadata in an isolated profile. Playback is paused and hot cues are staged; the empty successor queue and preparation requirements are preserved. No audio, account credentials, local library database or private source file paths are included.

The depicted MacBook Pro uses Apple's product-page AR asset for its enclosure, keyboard and display geometry. Apple retains rights to that asset and its marks. The model itself, source scene, private runtime profile and render tooling are not distributed here. The Blender reference informed lighting and camera direction; these images were rendered in WebGL.

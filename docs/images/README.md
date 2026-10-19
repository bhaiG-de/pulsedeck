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

The README displays **1920 × 1080 WebP previews** from `previews/`, encoded at quality 90 with method 6 after Lanczos downsampling. All six previews total **832 KB** rather than 9.52 MB of inline images (**91.3% less image data**). Fine text and waveforms were visually compared with the downscaled originals; the previews use lossy compression and are intended for normal README display.

Clicking any preview opens its **3840 × 2160 lossless WebP original** listed above. These originals preserve the approved PNG pixels exactly and are not loaded as inline images. Their combined size remains 9.52 MB. The native render, original dimensions and full-resolution downloads are preserved.

The session uses real local track metadata in an isolated profile. Playback is paused and hot cues are staged; the empty successor queue and preparation requirements are preserved. No audio, account credentials, local library database or private source file paths are included.

The depicted MacBook Pro uses Apple's product-page AR asset for its enclosure, keyboard and display geometry. Apple retains rights to that asset and its marks. The model itself, source scene, private runtime profile and render tooling are not distributed here. The Blender reference informed lighting and camera direction; these images were rendered in WebGL.

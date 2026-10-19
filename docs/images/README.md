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

All images are **3840 × 2160**, encoded as **lossless WebP**. Decoded RGBA pixels were checked against the approved PNG exports and are identical. Click an image in the main README to view its full resolution.

The session uses real local track metadata in an isolated profile. Playback is paused and hot cues are staged; the empty successor queue and preparation requirements are preserved. No audio, account credentials, local library database or private source file paths are included.

The depicted MacBook Pro uses Apple's product-page AR asset for its enclosure, keyboard and display geometry. Apple retains rights to that asset and its marks. The model itself, source scene, private runtime profile and render tooling are not distributed here. The Blender reference informed lighting and camera direction; these images were rendered in WebGL.

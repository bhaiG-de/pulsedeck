# Third-party code

PulseDeck's original code is licensed under the MIT license in `LICENSE`.

`vendor/cpal` contains CPAL 0.15.3, licensed under Apache-2.0. Its original
license and source notices are preserved in `vendor/cpal/LICENSE`. Local
CoreAudio changes are documented in `vendor/cpal/PULSEDECK-PATCH.md`.

Other Rust and JavaScript dependencies retain their respective licenses and
are resolved by Cargo and npm using the committed lockfiles. Third-party
application binaries, downloaded manuals and local inspection captures are
not distributed in this repository.

The beat model in `crates/audio-analysis/models/beat-this-small0.onnx` is
converted from the Beat This! `small0` checkpoint by the Institute of
Computational Perception, JKU Linz, released with its code under the MIT
license. Its permission notice is preserved in
`crates/audio-analysis/models/LICENSE-beat-this`; the conversion is described in
`crates/audio-analysis/models/README.md`.

Anatomy theme palette values are adapted from [Anatomy](https://github.com/wheresryan22/anatomy),
Copyright (c) 2026 Ryan, under the MIT license. Its permission notice is
preserved in `frontend/ANATOMY-LICENSE`. No illustration runtime is included.

# Third-party code

PulseDeck's original code is licensed under the MIT license in `LICENSE`.

`vendor/cpal` contains CPAL 0.15.3, licensed under Apache-2.0. Its original
license and source notices are preserved in `vendor/cpal/LICENSE`. Local
CoreAudio changes are documented in `vendor/cpal/PULSEDECK-PATCH.md`.

Other Rust and JavaScript dependencies retain their respective licenses and
are resolved by Cargo and npm using the committed lockfiles. Third-party
application binaries, downloaded manuals and local inspection captures are
not distributed in this repository.

The active Beat This! final0 and mel models are MIT licensed and described in
`src-tauri/resources/beat-models/README.md`. The bundled notices there cover
Beat This!, beat-this-rs and the BSD temporal decoder. The incoming small-model
tract spike is historical research; its ONNX weights are not shipped.

Anatomy theme palette values are adapted from [Anatomy](https://github.com/wheresryan22/anatomy),
Copyright (c) 2026 Ryan, under the MIT license. Its permission notice is
preserved in `frontend/ANATOMY-LICENSE`. No illustration runtime is included.

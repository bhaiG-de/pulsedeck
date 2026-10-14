# Beat This! small model

`beat-this-small0.onnx` is the Beat This! beat and downbeat tracker
([CPJKU/beat_this](https://github.com/CPJKU/beat_this), Foscarin, Schlüter and
Widmer, ISMIR 2024), converted for CPU inference with tract. The upstream code
and published weights are MIT licensed; the notice is in `LICENSE-beat-this`.

| Item | Value |
| --- | --- |
| Source checkpoint | `small0.ckpt`, 2,099,960 parameters, SHA-256 `6074be2c4d490c5f6101fcc374a1ec72ae93456e23bb6019783b849f5dc7d47b` |
| This file | 8,363,204 bytes, SHA-256 `96c06f05678d2e979cd8a053c1f065315f1d181425e52420a853973d7f2b4765` |
| Input | `spect`, float32 `[1, 1500, 128]`: one 30 s chunk of 128-band log-mel frames at 50 fps |
| Outputs | `beat` and `downbeat`, float32 `[1, 1500]` frame logits |

Two changes from a plain export, neither of which changes the arithmetic:

- The frontend's time-direction attention runs one frequency row at a time
  instead of all 32 at once. Peak inference memory drops from about 813 MB to
  325 MB; outputs match PyTorch within 1e-5.
- Weights are stored as float16 and cast back to float32 at load, cutting the
  file from 12 MB to 8 MB; logits stay within 0.003 of the float32 export.

Reproduce it with `tools/beat-model-spike` (`reference.py`, `variants.py export 1`,
`weights_f16.py`). Measurements are in `research/beat-model-spike.md`. When the
model or its preprocessing changes, bump `BEAT_MODEL_VERSION` in `src/beats.rs`
so stored analyses are re-run.

# PulseDeck macOS configuration-query patch

Vendored CPAL0.15.3 from the installed crates.io package (MIT/Apache license retained). Local installed source already carries lint annotations needed by this toolchain. Unused Rc import removed.

In `supported_configs` and `default_config`, query kAudioDevicePropertyBufferFrameSizeRange directly on the AudioDevice with AudioObjectGetPropertyData, rather than instantiating an input AudioUnit merely to read the hardware buffer range. This avoids temporary-device activation and input permission interactions during enumeration. Real input/output stream construction remains unchanged.

Primary source: https://github.com/RustAudio/cpal/blob/v0.15.3/src/host/coreaudio/macos/mod.rs

Evidence: main-thread and background discovery samples stalled inside AudioUnitSetProperty/HAL proxy while initializing a temporary unit. Discovery/start/stop additionally run away from the UI thread. This patch changes no render DSP or microphone permissions. Verify CoreAudio enumeration/output startup on hardware after applying.

`add_disconnect_listener` now captures a Weak stream reference. The previous strong reference formed a cycle (stream inner → listener callback → stream), retaining nondefault output AudioUnits after restart. Engine lifecycle additionally pauses old streams before drop and uses a stream generation to reject stale callbacks/errors.

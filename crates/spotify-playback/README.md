# Experimental Spotify PCM preparation

This crate connects the unmodified registry `librespot` 0.8 `Player` to a custom decoded-output sink. It supplies a native DJ source only if normal Spotify authentication, Premium eligibility and complete-track decoding succeed. Account playback has not yet been verified by this crate's unit tests.

The adapter uses a separate user-authorized native playback token with the platform identity returned by unmodified `SessionConfig::default()`. The user's developer Client ID and Web API token remain separate for library/API access, as in working ncspot and spotify-player implementations. Native identity is obtained from the library, not copied from another installed application. No identity rotation, entitlement changes, custom encoded audio/key/DRM path, automatic login or scope changes occur in this crate.

## Native API

- `run_worker_from_stdio() -> Result<(), String>`: invoke from the executable's `--spotify-worker` branch before Tauri or logging starts.
- `prepare_in_worker(executable: &Path, request: PrepareRequest, cancel: Arc<AtomicBool>) -> Result<PreparedSpotifyTrack, String>`: call from a native background job.
- `native_client_id() -> String`: obtain unmodified librespot's native platform identity for the separate native authorization flow.
- `probe_client_identity(developer_id: &str) -> Result<ClientIdentityProbe, String>`: dedicated pre-Tauri CLI diagnostic, with no account credentials or sign-in. It compares normal SDK client-token requests with the same device/OS/version configuration, changing only developer versus native identity. Returns acceptance booleans and sanitized errors only; SDK token values are discarded. Each request has a 20-second limit. This diagnostic does not establish Premium eligibility or audio playback.
- Request: native `client_id`, fresh separately authorized native `access_token`, `track_uri`, optional `expected_duration_ms`. A different Client ID is rejected before network; no token from the Web API login may be rebound to this identity. No `Debug` implementation; token is cleared on request drop.
- Result: owned `samples: Vec<[f32; 2]>`, `sample_rate: 44100`, and track metadata (`uri`, title, artists, album, duration). No credential or encoded-file fields.

A child process is required because upstream Session can call `process::exit(1)` for non-Premium accounts and playback has other process-exit paths. Those exits never run in Tauri. The parent sends a bounded JSON request through private stdin; only metadata, decoded f32 stereo packets, completion, and sanitized errors return through stdout. Child stderr is suppressed, and no credential-bearing arguments or environment variables are used.

The worker has no persistent librespot `Cache`, audio/source cache, or PCM export. Upstream's ordinary temporary encoded download handling is unchanged; the custom sink never receives or saves encoded packets. Normal decoded PCM exists only in bounded memory and the private process pipe.

Preparation is limited to 256 MiB of PCM per track (about 12 minutes 40 seconds), with a 120-second worker deadline and a five-second parent shutdown grace. Cancellation kills/reaps the child and discards partial PCM. A normally decoded full track, matching URI/duration, `EndOfTrack`, byte/frame integrity and successful worker exit are required before handing audio to the engine. Up to one second of codec/container duration tolerance is allowed; previews and truncated tracks are rejected. No network work occurs in the native audio callback.

The version-specific upstream build dependency is pinned to `vergen = 9.0.6`, matching the reviewed upstream lockfile; newer vergen 9.1 has incompatible traits with the `vergen-gitcl` 1.x dependency used by librespot 0.8. No librespot source is patched.

## Validation

Ten tests cover full-track PCM handoff, request/duration bounds, separate native identity and rejection of arbitrary Web API identities, credential-free probe validation/sanitization, partial/preview rejection, malformed/non-finite/wrong-rate output, credential-free metadata, worker cancellation/reaping, rejection of credential-bearing upstream error messages, and safe stage/status classification. `cargo clippy -p spotify-playback --all-targets -- -D warnings` passes.

Real authentication/decoding remains an experimental runtime check. If Spotify rejects the normal native session or the account/track is ineligible, return that failure; do not rotate identities or bypass it.

PulseDeck's original own-client trial requested `streaming` through explicit user consent. That corrected initial PermissionDenied: native sign-in/Premium confirmation succeeded, but native client-token acquisition returned HTTP 400 before decoding. The revised flow uses a separate native authorization rather than assuming a developer Web API identity is valid for native playback. The crate itself does not initiate OAuth consent. See [authentication diagnosis](../../research/spotify-authentication.md).

The separate native-login correction is ready for a live retry. Spotify PCM delivery and live cue/tempo/mixing remain unverified by this crate's tests.

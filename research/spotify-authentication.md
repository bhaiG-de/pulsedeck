# Native Spotify authentication failure

Updated 13 October 2026. This follows source analysis of pinned registry librespot 0.8, current upstream applications and PulseDeck's OAuth code, plus the coordinating agent's reported runtime results. Public source client identifiers were compared; no live token, original Spotify/rekordbox account credentials or user account storage was inspected by this research agent.

## Missing playback grant

PulseDeck's original OAuth request includes library/playlist and Spotify Connect control permissions, but **does not include `streaming`**. That is a concrete implementation gap. `user-modify-playback-state` permits controlling another Spotify playback device; it is not the same playback grant as `streaming`. Spotify documents `streaming` for Premium Web Playback SDK playback. [Spotify scopes](https://developer.spotify.com/documentation/web-api/concepts/scopes#streaming).

Upstream librespot's `get_creds` and `play_connect` examples explicitly request `streaming`, then pass the resulting token to `Credentials::with_access_token` and `Session.connect`/Connect initialization. That supports requesting this missing grant as the first legitimate correction. It does **not** prove that Spotify will accept every custom developer application's token for native access-point playback. [Upstream playback example](https://github.com/librespot-org/librespot/blob/v0.8.0/examples/play_connect.rs), [Upstream credential example](https://github.com/librespot-org/librespot/blob/v0.8.0/examples/get_creds.rs).

## Exact failure stage

The inspected registry source is under `.toolchain/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`, packages `librespot-core-0.8.0` and `librespot-oauth-0.8.0`.

- `Credentials::with_access_token` deliberately uses `username: None`, Spotify token authentication type and the token bytes. No user name, password or copied stored-credential blob is required by that upstream entry point.
- `Session::connect_inner` first connects to an access point and calls `connection::authenticate` with that token credential. If accepted, it reconnects using the reusable credential returned by the access point. Both are ordinary upstream behavior; PulseDeck does not implement or change this exchange.
- In `connection::authenticate`, a provider `AuthFailure` packet becomes an `AuthenticationError::LoginFailed(ErrorCode)`, which maps to `ErrorKind::PermissionDenied`. An access-point failure can therefore appear as the screenshot's coarse error.
- The initial login packet contains credential type/data and device/system information. It does **not** include `SessionConfig.client_id` as a separate login field. An initial token rejection should not be described as proof that the configured Client ID was rejected by a client-token endpoint.
- `Session.connect` also resolves an access point before this exchange. The broad `PermissionDenied` kind alone does not uniquely distinguish resolver errors from initial-token or reusable-credential rejection. A subsequent sanitized recognized provider reason can narrow the observation.

The observed error occurs before successful `Session.connect`, metadata lookup, decoder setup or the custom PCM sink. Changing sample rates, MIDI drivers, the deck mixer or the source-memory cap cannot repair that authentication failure.

## Initial own-client correction (historical)

The legitimate next step is an **explicit user-triggered authorization** requesting the existing required library scopes plus `streaming`, with the same user's own registered application, PKCE, checked state and loopback callback. Preserve the existing library session until the new consent/token exchange succeeds. No automatic scope expansion or login should occur from a deck load.

Read the new token response's granted scope; refresh does not add permissions to an older library-only grant. If the server omits the scope field because it matches the requested scope, preserve that request's actual scope set rather than incorrectly falling back to the older library-only constant. Expose a credential-free `playback_permission` readiness flag and reject a missing grant before spawning a playback worker.

This first attempt kept the existing developer application identity for native playback. That was our implementation constraint, not a fundamental librespot requirement. The later comparison below corrects that architecture using separate native authorization and the unmodified library configuration, while retaining ordinary Premium checks.

## Diagnostic improvement

The adapter now identifies sign-in as occurring before track metadata/audio and suggests **Authorize playback**, while explicitly acknowledging that granted streaming permission may still be insufficient for a native session.

Its source-level provider error class is retained. Because librespot's connection module is private, the adapter cannot downcast its private AP error enum; it classifies only exact fixed error phrases for known provider reasons. Every emitted reason is the adapter's own static text. Arbitrary upstream Display/debug output, URLs and token-like strings are never returned or logged. This preserves useful distinctions such as Premium required, token credentials rejected, additional verification or application access disabled without exposing raw provider messages.

The new test supplies an unknown synthetic message containing an access-token URL and verifies that neither it nor its secrets appear in the emitted diagnostic. The runtime AP reason is not known from the original coarse screenshot; the improved diagnostic must be exercised again after rebuilding.


## Verified retry after consent

The user approved Keychain and explicit playback consent. The app reports streaming granted; normal session sign-in and Premium confirmation now succeed. Two separate track loads initially reported metadata InvalidArgument. A further live retry with staged ordinary upstream calls identifies **client-token acquisition returning HTTP 400**, before login5 renewal/metadata/decoding. This is now the observed blocker; repeating library OAuth is not a demonstrated remedy. The server reason behind HTTP 400 remains unknown. No Spotify PCM reached the decks; native cue/tempo/mix acceptance is still pending.

## Comparison with working librespot applications

The normal architecture separates a developer Web API grant from native playback credentials. Public native Client IDs identify a client/protocol; they are not account passwords or client secrets. Their inclusion in upstream software does not itself establish Spotify partner approval.

| Application | Native playback | Web API access |
| --- | --- | --- |
| ncspot | Native OAuth uses its public `SPOTIFY_CLIENT_ID`; `Spotify::session_config` uses the same native identity. | Separate OAuth uses `NCSPOT_CLIENT_ID`; its developer explicitly requests permission before other apps reuse that API identity. |
| spotify-player | Native OAuth and SessionConfig use the same upstream native identity. Native credentials are independently cached. | User-configured developer Client ID has its own OAuth token; upstream also documents an ncspot fallback which PulseDeck does not adopt. |
| spotifyd | SessionConfig inherits the unmodified library default. User selects its Connect receiver for zeroconf pairing, or initiates manual native OAuth. | No configured developer Web API identity is substituted in that SessionConfig. |

Sources: [ncspot authentication](https://github.com/hrkfdn/ncspot/blob/main/src/authentication.rs), [ncspot SessionConfig](https://github.com/hrkfdn/ncspot/blob/main/src/spotify.rs), [spotify-player authentication](https://github.com/aome510/spotify-player#authentication), [spotify-player SessionConfig](https://github.com/aome510/spotify-player/blob/master/spotify_player/src/config/mod.rs), [spotifyd SessionConfig](https://github.com/Spotifyd/spotifyd/blob/master/src/config.rs), [spotifyd login documentation](https://github.com/Spotifyd/spotifyd/blob/master/docs/src/configuration/auth.md).

PulseDeck now exposes `spotify_playback::native_client_id()` from `SessionConfig::default().client_id`, rather than copying identifier literals or inspecting any installed application's credentials. Native preparation rejects a declared identity different from that platform default before network. The coordinating app is adding a separate user-triggered PKCE native login with its own Keychain lifecycle. Existing developer Web API grants remain separate and are not rebound to a different Client ID. The native worker continues to require normal accepted authentication, Premium eligibility and complete decoded audio.

This is a concrete correction to an observed difference from working applications. It remains a **live-test hypothesis** until a newly authorized native token succeeds through client-token/login5, metadata and decoding. HTTP 400 alone did not reveal the provider's reason.

## Upstream payload and platform assessment

The pinned registry 0.8 and current GitHub dev source have identical client-token request construction through the initial network call: SHA256 `2ee83ee3eb86b376e1629d76caad1c1a85eea9272b281e12f26bd74d957bbe27` for that compared source section. macOS uses the Session Client ID, detected OS version, upstream's fixed hardware-model field and Rust architecture string. No relevant current macOS 26 or `compiled_cpu_type` fix was found in the inspected issue searches or recent `spclient.rs` history. An upgrade alone has no identified correction for this failure. [Upstream request construction](https://github.com/librespot-org/librespot/blob/dev/core/src/spclient.rs), [Native defaults](https://github.com/librespot-org/librespot/blob/dev/core/src/config.rs).

Old issue #1261 concerned Linux client-token HTTP 400 and was closed through #1267; it is not evidence of a current macOS fix. Issue #1623's November 2025 playback regression had successful client-token acquisition and was fixed by extended metadata; PulseDeck's failure precedes metadata, and pinned 0.8 already uses that endpoint. [Issue #1261](https://github.com/librespot-org/librespot/issues/1261), [Issue #1623](https://github.com/librespot-org/librespot/issues/1623).

Normal zeroconf pairing is an alternative credential-acquisition flow: selecting the receiver in Spotify sends newly paired credentials to upstream Discovery and then Session/Spirc. It avoids copying existing app caches. It still advertises the receiver's configured native Client ID and uses the same identity for native service requests; pairing with the previously rejected developer ID is not a demonstrated remedy. No new pairing flow is implemented here.


## Controlled client-token comparison and correction

A live pre-Tauri diagnostic used two copies of the same librespot 0.8 SessionConfig/device ID, changing only client_id. No account credentials, session sign-in, metadata or audio were used. The configured developer Client ID returned HTTP 400; the unmodified upstream native Client ID successfully acquired a client token. Tokens were discarded/zeroized, and only acceptance/error flags were returned. [Retained credential-free result](evidence/client-token-identity-probe.json). This identifies the incompatible developer identity as the cause of the observed client-token rejection; the macOS request shape works with the native registration.

PulseDeck now keeps native and library sign-ins separate, following working ncspot/spotify-player/spotifyd implementations. Native OAuth uses the upstream default identity with streaming-only consent, PKCE, validated state and a separate loopback callback at http://127.0.0.1:8898/login. Its refresh token is stored under a distinct native Keychain account; the worker refuses developer IDs. Library OAuth remains under the user configured developer ID. The old developer grant is never rebound to the native identity. Keychain restoration happens on a background thread to avoid blocking the UI.

Native user consent and actual track preparation are the next runtime gates; client-token success alone does not establish PCM delivery or DJ acceptance.


## Successful native account/deck test

The user completed the separate native-player consent. The current release app then loaded full Spotify catalog tracks of 2:06 and 6:17 into decks A/B, with waveforms, native clocks and channel meters. The user confirmed playback. Controlled UI checks returned both decks to 00:00 with CUE, changed A from +8% /100.69BPM back to0% /93.23BPM and then +7.65% /100.36BPM, ran both decks independently and moved the crossfader through -1, approximately0 and +1. Both decks were stopped at their cue points after testing. CoreAudio remained48kHz with zero reported stream errors. Controlled transport checks used master gain0; acoustic mixing/cue-routing quality was not measured. No recording/export of Spotify audio was made.

The native client-token400 blocker is resolved by grant/identity separation. The earlier reports that native playback was fundamentally unavailable from normal librespot output were overly broad; they do not describe the current implementation. The remaining analysis/waveform quality work is separate from authentication and playback.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

# Spotify investigation and implemented integration

Updated 13 October 2026. Target: `/Applications/rekordbox 7/rekordbox.app`, version `7.2.19.0342`.

Native Spotify deck playback now works through the experimental librespot adapter after separating native and developer Web API authentication. A controlled comparison identified the developer identity as the native client-tokenHTTP400 cause; the upstream native identity succeeds. After the user's separate consent, full2:06 and6:17 tracks loaded into Rust decks. Native cue return, tempo changes, independent clocks and crossfader controls were checked, and the user confirmed playback. Acoustic mix/cue quality and FLX4 routing remain unmeasured.

The earlier partner/REA findings below describe rekordbox's implementation and official integration access. They do not establish that normal librespot output is technically unusable. The current experiment uses unmodified librespot 0.8 Player output and the user's own application/token, rather than recreating protected transport. See the final section for its current limits.

## What the installed application shows

The REA skill workflow reused existing matching shipped-artifact evidence instead of opening another native analysis session:

- REA `inspect_artifact`, evidence `ev_0f9e0cb5b5da4c2f5f25581bd40c12da6b40448650fec8d9d09ca0e908d16e86`: bundle inventory and file identities. The bundle includes native executable, streaming UI and Spotify login artwork. Its root graph SHA-256 is `c44ca9675301a84631c222fb6df631efeb6342ac39b0e86c2f486b2104ade90b`.
- REA `inspect_macho`, evidence `ev_0cb56fe857156135bf806b67c3ff43ac5eb91abe354136a7ecf74dc5d45d27ad`: native executable identity and load commands. Main executable SHA-256: `07dcecbaf304f777e435539c83c7dcc02e41de1eaf2e0a75e8a9852e4411a244`. Universal `x86_64`/`arm64` build; selected inspection is arm64.
- REA `inspect_dylib_resolution`, evidence `ev_1fe924908681aa4b5dfc9b5769871e2deea0dc1c9c09cbb71bcb6efe7b4e680d`: linked library inventory. It does not identify a separately named Spotify SDK library/version. This bounded negative result cannot rule out a statically linked SDK.

Evidence files: `research/evidence/rekordbox-7-installed/`. Collection used REA 6.4.0 and Node 24.21.0. No connected REA MCP tools or native decompiler are available in this task; no pseudocode was generated. Additional bounded LLVM static disassembly corroborates the SDK calls below; this is explicitly ordinary shell evidence, not REA pseudocode.

A focused projection of the existing arm64 symbol table is saved as `research/evidence/spotify-static-symbols.json`, including its source SHA-256 and demangled names. This is ordinary `nm`/`c++filt` evidence, not a new REA Evidence record. Relevant symbol observations:

| Observed function | Arm64 address | Architectural implication, inferred from names |
|---|---|---|
| `db::SpotifyBridge::login()` | `0x101f5aaa4` | A dedicated native Spotify login bridge |
| `db::SpotifyBridge::restoreLoginStatus()` | `0x101f5d190` | Restored authentication state |
| `db::SpotifyBridge::refreshTokenIfNeed()` | `0x101f5d554` | Token renewal responsibility |
| `db::SpotifyBridge::checkEligibilityFlow(...)` | `0x101f5ba84` | Eligibility checked independently of metadata browsing |
| `RekordboxCloud::postExtAccessTokenSpotifyCode2Token(...) const` | `0x100ebe0c4` | Candidate cloud-mediated authorization-code exchange |
| `RekordboxCloud::setSpotifySsoFunction()` | `0x100ec1524` | Candidate SSO adapter between cloud and streaming modules |
| `db::SpotifyLibraryLoader::run()` | `0x101f58a40` | Background collection/library loading |
| `db::SpotifyBridge::requestTrackMetadataAsync(...)` | `0x101f5c12c` | Asynchronous metadata retrieval |
| `db::SpotifyBridge::requestTrackArtworkAsync(...)` | `0x101f62358` | Separate artwork requests |
| `db::SpotifyBridge::onPlaybackCall(...)` | `0x101f58128` | Native streaming playback callback boundary |
| `streaming::StreamingManager::isSpotifyProtocol(...)` | `0x102d8f5ec` | Spotify tracks distinguished from ordinary file paths |

These names establish that the compiled classes/functions exist. They do not establish request URLs, exact OAuth grants, control flow, PCM delivery, codecs, caching policy, or execution reachability. No user tokens, cookies, embedded credentials, internal endpoint requests, or existing Spotify account state were inspected. Original rekordbox was not logged into Spotify by this task.

## Public integration boundaries

Spotify announced its DJ integration with named partners, including rekordbox, on 24 September 2025. This confirms actual DJ support in those products, but it does not publish a general-purpose DJ audio API. [Spotify announcement](https://newsroom.spotify.com/2025-09-24/dj-software-integration-premium/).

AlphaTheta documents Premium sign-in and online streaming, with no Spotify offline storage, recording/capture, sampler, or stems support. Since rekordbox 7.2.16, Spotify tracks can appear in its collection and playlists. These product capabilities cannot be inferred from the ordinary public Web API. [AlphaTheta streaming FAQ](https://rekordbox.com/en/support/faq/streaming-7/).

The public Web API's playback endpoint controls a Spotify playback device. It does not return a downloadable audio file or decoded PCM. It requires Premium for playback. PulseDeck uses this supported remote playback operation; playback success depends on an available Spotify Connect device. [Start/resume playback](https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback).

The Web Playback SDK creates an ordinary browser Spotify Connect player. Its supported browser list includes Safari; initialization can fail without encrypted-media support. The reviewed documentation does not establish support for Tauri's WKWebView/custom-origin environment or provide a Rust DSP/PCM interface. This build therefore does not advertise it as a native DJ stream provider. [SDK overview](https://developer.spotify.com/documentation/web-playback-sdk), [SDK initialization errors](https://developer.spotify.com/documentation/web-playback-sdk/reference#initialization_error).

The public Developer Policy prohibits mixing/overlapping Spotify content with other audio, and the playback documentation requires keeping content unaltered. The specific missing capability is authorized partner access to audio suitable for native DJ transport/mixing. Replacing HTTP endpoints, using a Premium account, or replacing the language with Rust does not supply that capability. No stream-ripping, internal-token reuse, or DRM workaround was implemented. [Spotify Developer Policy, section III.7](https://developer.spotify.com/policy).

Official Embeds can display a track player using `https://open.spotify.com/embed/track/{id}`. The widget must retain encrypted-media permissions. Spotify warns that unsupported browser/encrypted-media environments may receive only a preview under 30 seconds. OAuth login in PulseDeck does not authenticate the widget's separate browser cookie session. The widget can be offered with that condition, while Connect remains the implemented full-playback control path. [Creating an Embed](https://developer.spotify.com/documentation/embeds/tutorials/creating-an-embed), [Embed troubleshooting](https://developer.spotify.com/documentation/embeds/tutorials/troubleshooting).

## Own-app sign-in and API implementation

Implementation: `src-tauri/src/spotify.rs`. It is independent of the realtime deck engine.

1. The user supplies their own Spotify developer application's public Client ID. No client secret is needed or accepted.
2. Register `http://127.0.0.1:8765/callback` in that application's redirect URIs. The app binds port 8765 on IPv4 loopback and uses the exact registered URI in both authorization and token exchange. The dashboard rejected a portless loopback URI in the user's observed setup, despite documented dynamic-port support, so this implementation uses an explicit port; `localhost` is disallowed. [Redirect URI rules](https://developer.spotify.com/documentation/web-api/concepts/redirect_uri).
3. Clicking sign-in opens the system browser and initiates authorization code PKCE with a cryptographically random verifier and state. A five-minute native callback worker validates request method, exact callback path, loopback Host, state, and duplicate parameters. Unexpected requests do not consume the valid sign-in attempt. [PKCE flow](https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow).
4. Only the Client ID is saved in `spotify.json`. Refresh tokens and granted scope are held in PulseDeck's macOS Keychain entry, service `com.pulsedeck.spotify`; access tokens remain in native memory. Tokens are excluded from every frontend result. Account disconnect cancels pending sign-in, clears memory, and deletes PulseDeck's credential.
5. Playlist and liked-track operations return one page at a time, with `next_offset`. Pagination advances over raw items even when unavailable tracks/episodes are filtered. New playlist endpoint `/playlists/{id}/items` and response field `item` are supported; a 404-only fallback handles the older `/tracks` endpoint. Missing playlist counts and missing subscription fields are handled.
6. Token expiry triggers renewal; an API 401 gets one renewal attempt. Revoked/invalid sessions clear native authentication state. HTTP 403, 404, 429, network failures, and server errors produce actionable messages. A 429 imposes local request backoff respecting `Retry-After`; transport redirects are disabled to avoid forwarding bearer credentials to other origins.

Development Mode now requires the developer/app owner's Premium subscription and allows five new users; allow the intended account in the developer dashboard. Playlist contents are restricted to owned/collaborative playlists. User `product` is no longer reliably available, so the app does not infer Premium from an absent field. [February migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide), [Current playlist items endpoint](https://developer.spotify.com/documentation/web-api/reference/get-playlists-items).

The July 2026 changelog supersedes February's one-app limit: developers may create up to 25 apps. Development quotas are shared per developer account. This implementation reacts to quota failures without trying to spread traffic across Client IDs. [July 2026 changelog](https://developer.spotify.com/documentation/web-api/references/changes/july-2026).

## Verification and remaining work

`./tools/rust-env.sh cargo test -p pulsedeck spotify::tests -- --nocapture`: **9 tests passed**. They cover the RFC PKCE vector, callback state/path and ambiguous parameters, real loopback spoofed-state rejection followed by denial, fragmented HTTP headers, cancellation/listener cleanup, frontend token exclusion, current/legacy playlist metadata, pagination after filtering, and useful API error messages.

No real Spotify account or developer Client ID was available during implementation, so OAuth consent, token refresh against Spotify, macOS Keychain permission prompts, actual library responses, and audible Spotify Connect playback still need an account-level test. No guarantee of full-track widget playback in WKWebView is made.

| Investigation question | Result |
|---|---|
| Where are Spotify auth/library/native SDK boundaries in the installed app? | Native DJDownload exports and three wrapper call sites established; exact SDK ABI and runtime authorization remain unknown |
| Can own-app sign-in and library browsing be implemented with public APIs? | Implemented; account-level verification remains |
| Can the public API play selected songs? | Spotify Connect remote play/pause implemented; requires a ready Spotify device |
| Can public APIs feed two Rust DJ decks with equivalent mixing/scratching? | No such public capability established; authorized partner integration is needed |
| Can the official widget play inside the interface? | Ordinary widget is feasible; full playback is conditional and unverified in WKWebView |


## Native SDK follow-up: a concrete DJ audio boundary exists

A deeper focused search found **14 `DJDownload_*` exports directly in the retained REA Mach-O result**, evidence `ev_0cb56fe857156135bf806b67c3ff43ac5eb91abe354136a7ecf74dc5d45d27ad`. This is materially stronger evidence than simply finding Spotify UI or cloud-related names. The SDK is linked into the main native executable; no independently named Spotify dynamic library was found. The precise SDK vendor distribution name/version and its headers are not recovered.

`research/evidence/spotify-sdk-boundary.json` contains the projection of those actual REA exports, original Evidence ID and limitations, plus 109 focused ordinary native wrapper symbols. The main executable's current SHA-256 was checked again and matches the retained REA target identity.

| Export observed by REA | Arm64 address |
|---|---|
| `DJDownload_new` | `0x102e7c9d8` |
| `DJDownload_init_connectivity` | `0x102e7e46c` |
| `DJDownload_add_track` | `0x102e7eca4` |
| `DJDownload_open_track` | `0x102e7f904` |
| `DJDownload_read_track` | `0x102e7fd30` |
| `DJDownload_close_track` | `0x102e80638` |
| `DJDownload_open_track_rate_limit_remaining_time` | `0x102e80b14` |
| `DJDownload_get_client_token` | `0x102e80cc0` |
| `DJDownload_notify_playback_started` | `0x102e81154` |
| `DJDownload_start_playback_reporting` | `0x102e81564` |
| `DJDownload_report_playback_heartbeat` | `0x102e81bb4` |

Additional exports cover track removal and client shutdown/destruction. REA marks its merged nm/dyld export inventory non-exhaustive, so the 14 discovered names should not be interpreted as the full original SDK ABI.

### Corroborating behavior from bounded static disassembly

The installed Command Line Tools' LLVM disassembler was used only on five named functions, without running the target. Commands, output digests and target identity are in `research/evidence/spotify-disassembly-manifest.json`.

- `streaming::spotify::Client::createClient()` calls `DJDownload_new` at `0x102d15154`. It provides several function/context pointer pairs and stores an opaque client. Symbols identify associated auth, HTTPS, WebSocket, timer, random, clock, logging and initialization/shutdown helpers. This supports a host-abstraction/callback integration design; exact structure layouts remain unknown.
- `Client::openDownloadTrack(String const&)` calls `DJDownload_open_track` at `0x102d16430`, passing a string representation, completion callback and callback context. The corresponding named completion helper receives `TrackHandle const*`, `DJDownloadError`, two floats and an unsigned integer. The meanings of the float/integer metadata need the real SDK header or validated runtime evidence.
- `Client::readDownloadTrack(String const&, unsigned, unsigned, MemoryBlock&)` allocates the output `MemoryBlock` using the second unsigned argument, calls `DJDownload_read_track` at `0x102d167e4`, and resizes the output from an unsigned count written by the SDK. At that call, registers carry client, track handle, first unsigned argument, requested size, output buffer and output-count pointer. This supports **positional buffer-read** behavior, with the first integer plausibly an offset and the second a byte count; the offset unit, return/error enumeration, ownership and cancellation guarantees are not proven. No guessed `unsafe extern` binding was shipped.
- Spotify's `DownloadConnectionJob::run()` calls its service's `startDownloadTrack`, checks thread cancellation/download readiness and remaining time, then closes/ends the download track. This is a background producer boundary rather than work that should happen in an audio callback.
- Shared `CompressDataManager::InputStream::setPosition(int64)` clamps a requested position between zero and a virtual total length when initialized, saves it, and returns readiness. This is observed in a **shared** compressed-data reader, not proof that every Spotify SDK handle supports arbitrary seeks immediately.

These observations support the reconstruction architecture `authorized native SDK -> compressed/range reader -> native decoder -> prepared stereo PCM/cache -> Rust transport/mixer`. They do not by themselves establish a reusable standalone SDK binary, authorize another application with rekordbox's credentials, or demonstrate access to a single Spotify audio frame.

Names `Auth::getPartnerApiV2BaseUri`, `getPartnerReportingBaseUri`, `requestUserCapabilities`, `Service::getAuthorizedAccessToken`, and `Client::requestClientToken` are present. Spotify-specific playback observers include speed change, scratch, short loop and reverse state. The native path therefore appears to include **partner entitlement/capability and playback-reporting responsibilities**, in addition to ordinary user OAuth. This is an architectural inference from compiled names and call sites, not an authenticated network trace.

### What can be rebuilt, and what access is still missing

The host adapter, decoder/cache, memory ownership, cancellation, UI and Rust deck integration can be independently built. The Rust engine now exposes `EngineHandle::load_source(deck, Box<dyn EncodedAudioSource>, AudioSourceInfo)` for an integration-owned Read/Seek source. It decodes on the calling worker into `TrackData.samples: Vec<[f32;2]>` and passes the asset through the same generation-checked `load_data` path as local files. A short-read memory-source fixture validates decoding, waveform generation and native seek without a file. This is functional source-adapter groundwork, not a Spotify provider: SDK access, capabilities/reporting, expiry and eventually a seekable/page-backed streaming cache remain separate engineering and authorization gaps.

A user-supplied, legitimately licensed SDK could be integrated through a small native Rust adapter/plugin. The minimum supplied package and product access would need to include:

1. A macOS arm64 (and optionally x86_64) native DJ SDK build intended for this application's use, with its official C headers, ABI/version contract and dependency list. Export names in another executable are insufficient to reconstruct safe struct sizes, enum values or ownership.
2. PulseDeck's own enabled DJ integration identity and authorized token/capability acquisition flow. Public Web API Client ID + user Premium has not been shown to grant DJDownload stream access.
3. Documentation/authorization for two concurrently prepared decks, buffering, compressed formats, seek/range reads, track/handle lifetime and teardown. A Connect receiver build alone does not establish permission or capability for DJ mixing.
4. Required playback-reporting events, rate limits, token renewal, offline/recording/stems restrictions, and failure behavior.

A minimal independent provider contract would return an opaque track handle, source format/sample rate/channels/duration, seek/preparation capabilities, bounded `read_at`/`seek` results and typed readiness/errors. A native worker would decode prepared audio into owned stereo PCM pages. The audio callback would only read already prepared PCM, with no HTTP, OAuth, mutex waits, allocation, or SDK calls. Source capabilities would disable recording, sampler/stems or other unsupported operations before audio becomes available; the provider would receive actual playback state/reporting events outside the callback.

There is a publicly documented Spotify **hardware eSDK** access route: organizations apply, obtain approval, sign agreements and receive a build through Certomato. Its implementation documentation states that the binary handles backend negotiation/digital rights management and delivers compressed audio for device decoding. This establishes that licensed native audio SDKs exist; it does **not** establish that the hardware eSDK is the DJDownload SDK or that hardware approval enables desktop DJ operations. [Commercial hardware onboarding](https://developer.spotify.com/documentation/commercial-hardware), [eSDK implementation](https://developer.spotify.com/documentation/commercial-hardware/implementation), [technical requirements](https://developer.spotify.com/documentation/commercial-hardware/implementation/requirements/technical).

No public download, header/API reference, or general developer onboarding for the **DJDownload** desktop API was found in the focused first-party Spotify sources searched. Search absence is bounded evidence, not proof that such a program cannot exist. The SDK ABI can be learned further from the installed program, but rebuilding a client transport still leaves server-issued product eligibility, capabilities and authorized audio access unresolved.

**Native-deck completion remains blocked on an authorized Spotify audio provider.** The implemented OAuth/library/Connect functions are useful partial work. They are not a native-deck substitute and should not be reported as meeting the user's acceptance criterion.


## Open-source playback review, 13 October 2026

The user's GitHub search request found native playback projects, so the limitation must not be described as a technical impossibility of ordinary Spotify playback in Rust. These are different integration approaches, not established native two-deck DJ solutions. READMEs were reviewed; no third-party client was installed, authenticated or audio-tested.

| Repository | Documented functionality | Relevance to PulseDeck |
| --- | --- | --- |
| [librespot](https://github.com/librespot-org/librespot) | MIT Rust Spotify client/Connect receiver, Premium required, multiple native audio backends | Most relevant native playback architecture. Unofficial service implementation; its README warns about service permission. No verified two-deck DJ authorization or transport parity. |
| [spotify-player](https://github.com/aome510/spotify-player) | MIT terminal client, embedded librespot playback, remote control and native audio visualization | Application-level reference for playback lifecycle and library UI. Its current README distinguishes Web API auth from streaming-session auth; enabling the Web Playback SDK on our own Client ID is not a substitute for that session. |
| [ncspot](https://github.com/hrkfdn/ncspot) | BSD-2-Clause Rust terminal player built on librespot, macOS support, Premium required | Additional application-level native playback reference, not independent DJ SDK access. |
| [Spotify Web Playback example](https://github.com/spotify/spotify-web-playback-sdk-example) | Apache-2.0 official React/browser Connect player example | Supported ordinary browser-player starting point. Older README callback guidance uses localhost; follow current redirect requirements instead. No Rust PCM or DJ DSP interface established. |
| [spotifycontroller](https://github.com/gbroeckling/spotifycontroller) | Web API MIDI remote with optional real local-file decks | README explicitly says Spotify decks are virtual with a single active playback stream. Does not solve simultaneous Spotify deck mixing. |

The official Web Playback SDK is useful for an ordinary browser player. Dashboard selection enables that intended API product; it does not implement a player in PulseDeck. A real implementation needs the streaming scope and SDK initialization, token refresh, device readiness, autoplay/user activation and encrypted-media compatibility. Tauri WKWebView playback remains unverified, and the SDK exposes player controls/state rather than PCM for the native mixer. [SDK overview](https://developer.spotify.com/documentation/web-playback-sdk), [SDK reference](https://developer.spotify.com/documentation/web-playback-sdk/reference).

### Redirect correction

The user observed the Spotify dashboard rejecting the portless URI on 13 October 2026. Despite the documented dynamic-port option, PulseDeck now advertises and binds the explicit `http://127.0.0.1:8765/callback`, and sends the same URI to authorization and token exchange. Port conflicts return an actionable error without leaving sign-in pending. A real loopback denial test checks the production callback/authorization URI; a port-conflict/retry regression test also passes. Close and reopen the rebuilt app to pick up this change. The dashboard's acceptance of the new URI and live OAuth consent still require user verification.


## Librespot source feasibility review

Reviewed the upstream default branch at commit `4a005903e078d512456fb008aaa9ba6246881bd9` on 13 October 2026. Source checkout is under the ignored research scratch directory `artifacts/librespot-review`; no third-party binary was run and no Spotify account credentials were read or transmitted during this review.

Findings from original upstream source:

- `playback/src/audio_backend/mod.rs` supplies a custom audio-output interface, so ordinary native playback is technically possible without a webview. This does not establish an authorized two-deck DJ source.
- `core/src/authentication.rs` accepts OAuth credentials, while `core/src/config.rs` defaults to Spotify first-party application identities. PulseDeck's current own-app OAuth session and that native session must not be treated as interchangeable. No first-party identity substitution was implemented.
- `core/src/session.rs` has a process-exit path for non-Premium accounts. Embedding the library directly without isolation would create an application-termination risk. No attempt was made to weaken Premium checks.
- The native player owns transport and decoding; its custom decoded sink is technically suitable for a PCM source adapter. A Connect receiver beside CPAL alone does not supply independent deck transport; prepared audio must enter PulseDeck's source/clock/mixer path.

This initial source review did not access credentials or run an account trial. It was followed by the experiment below. Its earlier refusal to consider the normal decoded-output boundary was too broad; that boundary is implementable without rebuilding transport.


## Current experimental implementation and live result

`crates/spotify-playback` uses registry librespot 0.8's normal Player and a custom decoded sink. A child process isolates upstream process-exit paths. Private stdin carries the native refreshed OAuth token and configured Client ID; stdout carries framed stereo f32 PCM, metadata and sanitized errors. No credentials enter frontend payloads, CLI arguments, environment or logs. Premium checks remain upstream. No manually implemented protected transport or alternate client identity is used.

Preparation requires matching URI/duration, full-track completion and successful worker exit; partial/preview output is rejected. PCM is RAM-only, capped at 256 MiB per track (about 12m40s) and 120 seconds preparation, with cancellation. One preparation runs at a time. Loaded decks have independent native clocks/cue/tempo/mixing. Generation tickets prevent stale loads; disconnect/identity changes cancel and unload these assets. Recording is blocked both before WAV creation and in the render command path. Whole-track preparation means loading is delayed until completion; this is not a paged streaming cache.

The live release-app attempt on 13 October returned `PermissionDenied` from Session.connect before any audio. Upstream AP authentication receives the OAuth token at that stage; this error alone does not prove client-identity rejection. Inspection found PulseDeck's original OAuth scopes lacked `streaming`, which upstream `examples/play_connect.rs`, `examples/get_creds.rs` and OAuth examples request. [Upstream examples](https://github.com/librespot-org/librespot/tree/dev/examples), [Spotify scope reference](https://developer.spotify.com/documentation/web-api/concepts/scopes).

The app now reports whether the stored token includes that exact scope, provides an explicit user-triggered **Authorize playback** consent flow, preserves the old library session on cancellation, and refuses to start native preparation with the known incomplete grant. User consent is now verified. The next acceptance step is resolving native client-token acquisition, then a complete track load, native playback/cue/tempo verification and two-deck mixing. Successful native decoding is not yet verified. Unit tests exercise the PCM interface and lifecycle, not account playback.


### Runtime retry after playback consent

The user approved macOS Keychain access and streaming consent. The app reports the playback grant, and normal librespot session sign-in plus Premium eligibility succeeds. Load A and Load B for different catalog tracks both initially surfaced metadata InvalidArgument. The rebuilt worker separates the normal cached client-token and login5 requests before metadata; a live retry now reports **client-token acquisition: InvalidArgument / HTTP 400**. This localizes the failure before login5 renewal, metadata retrieval, decoder creation or PCM delivery. HTTP 400 alone does not establish the exact server reason. No protected content was loaded, and Spotify cueing, tempo control and mixing have not been exercised. The native source fixtures cover those engine operations separately.


### Successful account test

See [the authentication report](spotify-authentication.md#successful-native-accountdeck-test) for the current verified result. The earlier PermissionDenied/HTTP400 results above are historical investigation steps; normal native authentication and full-track PCM delivery now succeed. No vendor app credential reuse or custom protected transport was needed.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

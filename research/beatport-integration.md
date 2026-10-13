# Beatport native-streaming investigation

Updated 13 October 2026. Target: `/Applications/rekordbox 7/rekordbox.app`, version `7.2.19.0342`.

**Beatport audio in PulseDeck's native Rust decks is not implemented.** The installed rekordbox build contains a concrete native Beatport client/reader SDK, with full-track, deck-count and offline capability checks. PulseDeck has no independently issued Beatport integration access or SDK package. A consumer streaming subscription alone has not been shown to provide those application-level prerequisites.

The user's subsequent direction is to investigate record pools and use their officially downloaded local files. That path fits PulseDeck's existing local-file deck engine. This report preserves the Beatport findings for a later authorized streaming integration; no further probes or app changes were made after that pivot.

## Evidence and scope

The REA skill's shipped-artifact workflow reused matching retained evidence. The target is already selected, and the native executable identity was revalidated in the Spotify follow-up rather than opening another broad native session.

- REA `inspect_macho`: `ev_0cb56fe857156135bf806b67c3ff43ac5eb91abe354136a7ecf74dc5d45d27ad`, saved in `research/evidence/rekordbox-7-installed/main-macho.json`. Main executable SHA-256: `07dcecbaf304f777e435539c83c7dcc02e41de1eaf2e0a75e8a9852e4411a244`. Universal x86_64/arm64 application; this symbol inspection concerns arm64.
- REA `inspect_artifact`: `ev_0f9e0cb5b5da4c2f5f25581bd40c12da6b40448650fec8d9d09ca0e908d16e86`, saved beside that file. Bundle root graph SHA-256: `c44ca9675301a84631c222fb6df631efeb6342ac39b0e86c2f486b2104ade90b`.
- REA linked-library resolution: `ev_1fe924908681aa4b5dfc9b5769871e2deea0dc1c9c09cbb71bcb6efe7b4e680d`. No separate Beatport-named native library was identified in that linked dependency inventory; the native SDK symbols are present in the main executable.
- Existing ordinary shell symbol table: `research/evidence/rekordbox-native-symbols.txt`, SHA-256 `52681a447c23016338369c1d48d460068e90e3e51c85533ed8aee00dfa3885be`. Focused name searches corroborate host wrappers but are not new REA Evidence records.

REA's retained Mach-O export inventory contains **367 names matching Beatport**, including client, asset, networking, state and reader functions. This inventory is explicitly non-exhaustive: merged nm/dyld results may be incomplete. The count is a filtered observation, not an SDK completeness claim or version identification.

No original application login, captured traffic, requests to internal endpoints, private vendor credentials, user tokens, password storage, keys, DRM internals or account mutations were inspected. No Beatport SDK implementation was executed, and no native decompilation was available. This focused Beatport pass uses export/signature and wrapper-name evidence only; it does not claim dynamic behavior or a verified SDK ABI.

## Native reader and capability boundary

These SDK functions are actual exports in the retained REA result, not merely UI references:

| Export, demangled for readability | Arm64 address | What the signature establishes |
|---|---|---|
| `beatport::client::getTrackReader(callback, void*, int64)` | `0x103466f90` | A track reader acquisition operation with callback context and numeric track identity |
| `beatport::client::releaseTrackReader(void*)` | `0x103467154` | Explicit opaque-reader lifetime release |
| `beatport::client::getTrackBytes(void*, void*, int, int, bool*)` | `0x10346722c` | A native buffer-reading operation with reader/buffer pointers, two integer arguments and boolean output |
| `beatport::client::isOfflineSupported() const` | `0x103469aa8` | Account/client capability query for offline support |
| `beatport::client::isFullTrackPlaybackSupported() const` | `0x103469b08` | Full-track eligibility is exposed as a separate capability |
| `beatport::client::getMaximumDecks() const` | `0x103469b68` | The permitted deck count is a queryable client capability |
| `beatport::client::getMaximumOfflineTracks() const` | `0x103469bbc` | Offline track allowance is a queryable capability |
| `beatport::client::reportPosition(void*, int)` | `0x103469cf8` | Position reporting is associated with a native reader |
| `beatport::client::getInfo()` | `0x10346a818` | An SDK metadata/introspection function exists |

The reader callback's mangled type includes `client*`, callback context, an `int64` value, `beatport::load::status`, two integers and another opaque pointer. Those names support asynchronous acquisition/status and an opaque handle. They do **not** identify exact parameter meanings, integer units, return types, ownership after errors, thread guarantees, compression, or decoded PCM format. C++ mangling omits ordinary non-template return types, so unsafe Rust bindings must not be fabricated from these signatures alone.

The installed SDK also exports a client constructor using `beatport::Environment`, string parameters, callback context, an integer and a `partnerEvent` callback, and a `Services` constructor taking `PartnerConfig`. State-transition names include fetched profile, user-loaded state with `License`, login-required state and logout. Those observations support a **partner-configured client plus subscriber/license state** boundary. They do not establish that arbitrary public credentials or a subscription authorize a new product to use it.

This differs from a remote playback API: the native `getTrackReader/getTrackBytes` interface is a plausible source for a real native deck decoder, once the SDK build and the application's own authorized access are available.

## How rekordbox connects that SDK to the deck engine

Focused observations from the existing ordinary arm64 symbol table:

| Native host wrapper | Arm64 address | Inference from compiled names/types |
|---|---|---|
| `beatport::BeatportClient::initialize(bool)` | `0x102c74014` | Host SDK/client initialization boundary |
| `beatport::BeatportClient::login(bool)` | `0x102c744a0` | Separate authentication action |
| `beatport::DownloadReader::downloadReaderCallback(...)` | `0x102c827c8` | SDK reader results/status are bridged into host downloading |
| `streaming::BeatportService::requestDownload(...)` | `0x102c9bb40` | Native service delegates asynchronous track preparation |
| `streaming::BeatportService::getData(String const&, int64, int64)` | `0x102c9c01c` | Track-associated data/range access boundary |
| `BeatportCompressDataItem::createInputStream(...)` | `0x102c7b474` | Compressed data can be exposed through a stream abstraction |
| `BeatportInputStream::read(void*, int)` | `0x102c7c384` | Ordinary native byte-oriented input-stream interface |
| `BeatportInputStream::setPosition(int64)` | `0x102c7c568` | Positional stream operation exists |
| `streaming::BeatportService::isFullTrackPlaybackSupported()` | `0x102c9c66c` | Full-track readiness is propagated to application services |
| `streaming::BeatportService::getSdkVersion() const` | `0x102c9c67c` | A version getter exists; its returned version was not recovered |

Additional names include compressed total/download bytes, total/download samples, input-stream exhaustion/position, cancellation, download workers, current streaming quality, playback position reporting, offline-file management and user/library/playlist queries.

Together these support this **inferred reconstruction boundary**:

`own authorized Beatport SDK client → asynchronous track reader → compressed byte/range stream → native decode/preparation → stereo PCM/cache → Rust deck transport and mixer`

Static names do not prove immediate arbitrary seeking, uninterrupted playback while downloading, lock-free access, observed buffer sizes, or the exact codec. The existence of unrelated Castlabs/DRM classes elsewhere in the same application does not attribute that DRM implementation to Beatport; this report makes no such attribution.

## Independent implementation contract

The host side can be rebuilt independently without copying rekordbox code:

1. A native provider worker owns a Beatport client, subscriber/login state and opaque reader handles. It publishes connection/readiness and explicit full-track, maximum-deck and offline capabilities.
2. Reader acquisition and buffered reads occur outside the realtime callback. An officially specified byte reader/seek interface feeds a decoder. The worker tracks cancellation, source generation, partial-read readiness, renewal failures and reader release.
3. Prepared stereo PCM is passed to the native deck engine. The current engine's `decode_file` creates `TrackData { samples: Vec<[f32;2]>, sample_rate, analysis }`, and its private `EngineHandle::load_data` installs prepared audio. A provider-facing preparation entry point and streaming/page-backed source abstraction are needed for this future provider. They are separate from account authorization.
4. The audio callback reads prepared PCM only. It performs no HTTP, OAuth, native SDK acquisition, blocking reads, allocation, Keychain access or network waits.
5. Actual deck playback state/position is reported through a worker, and provider restrictions govern recording, sampler use, offline storage and deck count. Limits should come from the provider/license response rather than a hardcoded marketing plan name.

Because the observed SDK interface is C++, the sensible Rust boundary is a small `extern "C"` shim compiled against **official supplied SDK headers**, exposing versioned opaque client/track handles, typed status and bounded read operations. This avoids passing `std::string`, callback wrappers or compiler-specific C++ object layouts directly through Rust FFI. The shim's ownership, threading and return types require official headers or validated documentation; no guessed adapter was shipped.

The external prerequisites are PulseDeck's own approved integration identity/API access, its supported native SDK package and headers, a documented sign-in/renewal flow, and a subscriber license with full-track playback for that product. A binary already linked into another application does not establish a standalone SDK package or PulseDeck entitlement.

## Current first-party product and developer access findings

The coordinating agent checked these first-party sources during this investigation:

- Beatport's streaming product page lists its currently supported DJ software and consumer plans. PulseDeck is not listed as a supported product. Buying a plan gives subscriber access within supported integrations; it does not itself supply a native SDK for a newly built application. [Beatport Streaming](https://stream.beatport.com/).
- Beatport's partner SDK portal is gated. Its published API terms describe access for preapproved licensees, application for access, and the licensee's own API key. The coordinating agent observed the terms page updated on 28 September 2026. [Partner portal](https://partnerportal.beatport.com/hc/en-us), [Beatport terms](https://support.beatport.com/hc/en-us/articles/4414997837716-Terms-and-Conditions).
- The public API documentation entry point is login-gated, so this pass does not treat it as an available anonymous streaming specification. [API documentation](https://api.beatport.com/v4/docs/).
- The coordinating agent found a streaming allowance of up to four concurrent streams per device in the terms. The actual native SDK advertises a queryable maximum-deck capability, so implementation should enforce the authorized SDK result rather than assume the theoretical allowance applies to every product/account.

No marketing price or plan label is used as proof of a returned audio URL, codec, DRM capability or PulseDeck authorization. No partner registration or service agreement was submitted.

## Completion status

| Question | Result |
|---|---|
| Does installed rekordbox contain a native Beatport audio reader integration? | Yes: retained REA exports establish the client/reader/buffer and capability API names/types |
| Does that prove a standalone SDK can be safely reused by PulseDeck? | No: official ABI, package, own product access and authentication are still missing |
| Can Rust decode/preparation/mixing be independently implemented? | Yes; current local engine supplies the destination seam, with provider/streaming work still required |
| Is subscriber sign-in and native Beatport audio working in PulseDeck? | No; no account-level integration or audio test occurred |
| What can the user use now without that partner integration? | Officially downloaded local music files, including suitable record-pool downloads, imported into the existing native decks |

The Beatport streaming investigation is complete to this bounded static scope. The native streaming feature remains unresolved. Current implementation work should follow the user's record-pool/local-download direction rather than silently label a playlist browser or preview player as native streaming.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

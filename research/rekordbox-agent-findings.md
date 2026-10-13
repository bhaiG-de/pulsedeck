# rekordboxAgent static findings

Inspected 2026-10-13. Target: `/Applications/rekordbox 7/rekordbox.app/Contents/MacOS/rekordboxAgent.app`. This report covers the embedded helper, not the main rekordbox native audio/UI process. No helper launch, runtime inspection, network request, user database access, or application/configuration modification was performed.

## Main result

The shipped helper is **rb-cloud-agent / rekordboxAgent 3.3.0**, an Electron background application with a small React notification renderer and architecture-specific compiled JavaScript logic. The available evidence points to cloud/library synchronization, sharing, notifications and tray management as its responsibilities. It does **not establish** the helper as the live DJ audio engine or DDJ-FLX4 controller implementation.

For a Rust/Tauri rewrite, implement library/cloud jobs as an asynchronous service separate from the realtime deck/audio engine. Notification UI and event bridges translate naturally to a Tauri webview, while sync, database and file-transfer tasks belong in Rust background workers. This separation is a design recommendation, not proof of every internal boundary in rekordbox.

## Evidence identities and scope

- `ev_f9ab6147ce085aaae01ec0d3b5bcc364a306cde8141f64e765015afc4bc7da77`: REA `inspect_plist` on the helper, saved in `research/evidence/rekordbox-agent/plist-rea.json`.
- `ev_ed442536a33b23e146a2a1feff0fb25d24099d3937e4fddd90ed1151b6037367`: REA `analyze_javascript_application` on a **selected extracted artifact tree**, saved as complete JSON in `javascript-analysis.json` before any projection. It is not analysis of the entire original ASAR.
- `ev_a5a4fcac01e47954fc6afc658cc1083332de0c33c7e32eb235883f177fd4600b`: REA summary view of that completed analysis, saved as `javascript-summary.json`.
- Original ASAR full-file SHA-256: `3c26a89c4fcec100c872c7a3b96ba10b76baf7461543c54fe8ca1c7acc341176`. Complete format-aware inventory: `asar-inventory.json`, 8,382 member entries.
- Selected tree SHA-256 recorded by REA: `c396668401c93c11ee6c6555e8190870b653a5365e15ee32dab4786766644710`. Selection manifest records original member byte offsets and SHA-256 values. Policy: all first-party members plus dependency `package.json` and `.node` files; dependency JavaScript excluded. 807 selected members, 12,422,177 bytes.
- The plist's `ElectronAsarIntegrity` hash is a header JSON digest, not a full-file digest. `asar-header-integrity.json` records the direct comparison.

REA 6.4.0's actual CLI schema has no summary/source-path option for application analysis. The producer was bounded by a 45-second per-source timeout and a 1 GiB worker heap, with full JSON redirected to disk; its output was then projected through `inspect-analysis-view`. The minified notification bundle's semantic analysis exhausted its heap. REA explicitly reports partial application/semantic coverage and 54 semantic unknowns. Main/preload static observations remain useful; zero recovered BrowserWindow/native-addon bindings must not be read as absence. `.jsc` compiled logic is not decompiled by this pass.

## Observations

| Finding | Static source | Confidence and meaning |
|---|---|---|
| Electron background helper | helper `Contents/Info.plist`: `com.electron.rekordboxagent`, `LSBackgroundOnly=true`, `LSUIElement=true`, `AtomApplication` | Exact shipped declarations; REA plist evidence above. Permissions strings do not establish actual camera/microphone/Bluetooth use. |
| Actual embedded Electron framework 19.0.10 | `Contents/Frameworks/Electron Framework.framework/Resources/Info.plist`: `CFBundleVersion=19.0.10` | Exact framework metadata; manifest dev dependency is only `^19.0.8`. |
| Helper package 3.3.0 | ASAR `package.json`: `name=rb-cloud-agent`, `productName=rekordboxAgent`, `version=3.3.0` | Exact package bytes; REA package observation. |
| Architecture-dependent bytecode bootstrap | ASAR `main.js` requires `bytenode`, sets V8 `--no-lazy`, selects `jsc_arm64`/`jsc_x64` using `os.platform()` and `os.arch()`, dynamically requires `main_core.jsc` | Readable source, statically parsed. Actual selected runtime branch not observed. |
| 210 ARM64 `.jsc` members plus corresponding x64 tree | ASAR inventory `jsc_arm64/`, `jsc_x64/` | Exact member inventory; filenames alone do not prove executed responsibilities. |
| React notification view | `public/renderer/notification/static/js/main.60e305e9.js.map`, embedded sources `contexts/AppContext/index.js`, `components/pages/NotificationsPage/index.js`, `App.js`, `index.js` | Readable shipped source-map content: graphic/text announcements, carousel, dates/new indicators, click IDs, sizing and always-show checkbox. Source-map content is historical/source representation, not observed runtime. |
| Renderer framework strings | same source map: `react.production.min.js` exports version `18.2.0`; react-dom declares renderer version `18.2.0` and export/reconciler `18.2.0-next-9e3b772b8-20220608` | Exact embedded text; do not simplify react-dom's mixed version markers to one exact npm package identity. |
| SQLCipher native bindings | `node_modules/@journeyapps/sqlcipher/lib/binding/napi-v6-darwin-{arm64,x64}/node_sqlite3.node` | Exact inventory. `@journeyapps/sqlcipher` package 5.3.1; loading/encryption settings remain unobserved. |
| Library persistence/ORM candidate | Sequelize 4.44.4 manifest; `.jsc` strings mention `dialectModulePath`, SQLCipher, models and SQLite WAL/NORMAL/autocheckpoint pragmas | Exact static bytes supporting an inferred ORM/database role. No user database/schema was opened. |
| Express HTTP API candidate | Express 4.16.4, express-bearer-token manifests; shipped config `server.port=30001`; bytecode literals `/api/v1/`, `/data`, `/agent`, `/localSync`, `/sharedPlaylists` | Static boundary inference. No live listening address, successful registration, request or authentication enforcement observed. Binding interface remains unknown. |
| Sync/cloud dependencies | AWS SDK 2.1185.0; config AWS region/bucket and clock-server URL; managers for sync, playlist sync, file/image transfer | Inventory/config observations plus bytecode literals; network use and full protocol semantics remain unknown. |

Installed dependency versions, distinguished from manifest version ranges: bytenode 1.3.6, Express 4.16.4, Sequelize 4.44.4, SQLCipher 5.3.1, AWS SDK 2.1185.0, electron-log 3.0.9 and electron-json-storage 4.5.0. These are bundled helper dependencies and do not imply main-app audio libraries.

## Readable Electron IPC surface

REA observed one `contextBridge.exposeInMainWorld('electronAPI', ...)` with seven members in ASAR `public/shared/preload.js` (lines 3–11):

| Direction implied by source | Channel | Value or role |
|---|---|---|
| Renderer → main | `renderer-ready` | Renderer startup message |
| Renderer → main | `client-ready` | Client initialization response |
| Main → renderer | `client-init` | Listener callback; source-map client handler accepts `{ env }` |
| Main → renderer | `data-updated` | Listener callback; source-map handler reads graphics, texts, always-show setting and localized strings |
| Renderer → main | `content-size` | `{ width, height }` |
| Renderer → main | `show-always-notification-window` | `{ show }` |
| Renderer → main | `item-click` | `{ id, isGraphic }` |

The compiled `server/index.jsc` contains matching literals `ipcMain`, `renderer-ready`, `client-init`, `client-ready`, and `DataUpdated`; this corroborates a candidate main-side notification bridge but is not a verified semantic pairing. REA's AST graph reports **no recovered main handlers** because core logic is compiled `.jsc`, not because there are none. Runtime event rates and execution reachability are unmeasured.

## Compiled-bytecode string evidence and conservative inferences

Complete offset-bearing `strings -t x` results for selected `.jsc` files are retained in `bytecode-strings/`; offsets refer to extracted member bytes, linked to original ASAR offsets/digests by `selection-manifest.json`.

- `app.jsc`: `0x9d7 /api/v1/`; `0x8ab 100mb`; `sync_token`, `api_token`, `Bearer`, `express-bearer-token`, `compression`, body-parser. These support an HTTP application candidate, not a recovered middleware configuration.
- `routes/api/index.jsc`: `0x5bc /data`, `0x5ec /agent`, `0x620 /localSync`, `0x65c /sharedPlaylists`, plus sibling module names. Names suggest a versioned CRUD/sync boundary; exact endpoint verbs/schema/error behavior remain unknown.
- `routes/api/handler_helpers/handler.jsc`: `0x2061 APITokenAuthHandler`, `0x2546 SyncTokenAuthHandler`, `0x29e8 UserAgentCheckAndAPITokenAuthHandler`, `0x25d8 ^rekordbox\\/`, `unauthorized`, `POST`, `DELETE`. These are candidates for distinct auth policies, not evidence of secure enforcement.
- `main_core.jsc`: `0x24f5 30001`, CLI option strings `analysis-data-root-path`, `settings-root-path`, `port`, `passphrase`, `0x2d66 /api/client/v1/config`, `0x12a7 ^rekordboxagent:\\/\\/notification\\/([0-9]+?)\\/`, `BrowserWindow`, `loadURL`, tray/storage/protocol strings. This supports configurable helper paths/HTTP endpoint and notification activation candidates. No private path values or passphrases were collected.
- `express_loader.jsc`: `0x2de PRAGMA journal_mode = WAL`, `0x340 PRAGMA synchronous = NORMAL`, `0x376 PRAGMA wal_autocheckpoint = 1000`, `createServer`, `listen`; configuration remains inferred rather than executed.
- `server/index.jsc`: auth/sync/tray/strings/notification managers; progress labels `full_sync`, `incremental_sync`, `send_changes`; file/image worker-cluster and shared-playlist progress strings.
- `controllers/sync_manager.jsc`: content/cue/hot-cue/active-censor expansion and cloud registry strings. Compiled model member inventory includes playlists, content, cue, artists, albums, genres, tags, histories, related tracks, sampler and shared playlist objects. This establishes the artifact's breadth, not recovered table definitions or complete sync algorithms.

## Rust/Tauri reconstruction consequences

1. Keep **audio transport, mixing, scratching, MIDI and controller LED feedback** independent of cloud/library helper scheduling. This helper pass provides no audio callback, latency or MIDI evidence and cannot establish performance parity.
2. Implement a typed Rust library service for persistence/import, metadata mutations and queued file work; communicate to Tauri with bounded event payloads. SQLCipher and Sequelize observations are useful interoperability clues, but an independent schema is a design choice until a permitted format-level library inspection establishes exchange requirements.
3. Model sync as its own worker/state machine: run/pause, full/incremental progress, file transfers, sharing and notifications. Shipped config declares `standalone`, `sync`, `trial_sync`, `trial_export_sync` modes and run/pause settings. Feature parity with vendor cloud services cannot be established from static packages; compatibility, account APIs and service permissions remain separate unresolved requirements.
4. Use a small typed notification event contract for graphic/text announcements, readiness handshake, window sizing and user clicks. The exact existing channel names can guide conceptual mapping; reproducing names does not reproduce semantic behavior.
5. For first DDJ-FLX4 performance milestone, defer cloud, shared playlists and announcement rendering. They belong in the full roadmap after local library and two-deck reliability.

## Open questions

- Full compiled `.jsc` algorithms, exact HTTP verbs/bodies, main-to-helper client boundary, bind address, runtime security policy and callback ordering are unresolved.
- No helper CPU, memory, background wakeup, throughput or crash/isolation measurement was taken.
- No conclusion here identifies the main rekordbox audio implementation, precise database compatibility, controller driver requirements, time-stretch quality or performance parity.
- A bounded runtime observation would be needed to confirm which inferred HTTP/IPC surfaces are exercised, without opening user account content or making changes.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.

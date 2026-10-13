# PulseDeck CLI control map

Planning inventory, 2026-10-14. **All CLI commands below are proposed; no product implementation is included in this change.**

## Product contract

Every product action available through the UI or a supported MIDI control must have a callable command. The CLI, UI, and MIDI adapters should execute the same domain operations against the **same running audio engine**, library, jobs, and controller state. The future MCP layer can then describe these commands, inspect state, and call them without reimplementing DJ behavior.

The goal includes presentation controls: selecting music, searching, sorting, opening the browser, choosing pad modes, changing theme, editing a form, zooming a waveform, and dismissing a notice. A native deck operation should work independently of which UI panel is open. A presentation operation should report whether its desired state has been rendered by a mounted view.

Two separate completion targets:

1. **Complete current-product parity:** every implemented UI/MIDI action, plus state and completion information needed to drive it reliably.
2. **Agent DJ capabilities:** explicit-time cue editing without disturbing playback, complete playlist/setlist editing, musical-section analysis, server-side automation and transitions. Some of these are new product capabilities, not existing functions waiting to be exposed.

## Repository evidence

The inventory was traced through the actual handlers, not just labels or MIDI enums.

| Source | What it establishes |
| --- | --- |
| [App.tsx](../frontend/src/App.tsx) | Deck transport, waveform/platter gestures, cues, grid editor, mixer, recording, sampler, library, keyboard actions, settings, MIDI browser interpretation. |
| [SpotifyPanel.tsx](../frontend/src/SpotifyPanel.tsx) | Library/native authentication, collections, albums, pagination, native loading, Connect playback, widgets, analysis, browser selection and focus. |
| [TrackDrawer.tsx](../frontend/src/TrackDrawer.tsx) | Open/close, resize, snap heights, keyboard resizing, drag-to-load presentation. |
| [AnalysisInfo.tsx](../frontend/src/AnalysisInfo.tsx), [theme.ts](../frontend/src/theme.ts), [controller-browser.ts](../frontend/src/controller-browser.ts) | Analysis popover/candidate choice, stored theme, browser navigation, zoom, instant-double gesture. |
| [main.rs](../src-tauri/src/main.rs) | AppState, 54 registered Tauri handlers, dialogs, loading/import/audio generations, engine-command smart-mode interception, persistence loop. Relevant anchors: `engine_command`, `set_controller_*`, `choose_sample`, `invoke_handler`. |
| [controller.rs](../src-tauri/src/controller.rs) | Separate controller state and dispatch, effects, pad modes, smart modes, tap tempo, native fallback browser and LED feedback. Relevant anchors: `ControlState`, `ControllerSurface`, `handle`, `feedback`. |
| [model.rs](../crates/deck-engine/src/model.rs) | 49 EngineCommand variants, snapshots, deck indices, ranges and validation. |
| [lib.rs](../crates/deck-engine/src/lib.rs), [dsp.rs](../crates/deck-engine/src/dsp.rs), [snapshot.rs](../crates/deck-engine/src/snapshot.rs) | Bounded render queue, asynchronous preparation, native engine lifecycle, sampler, load revision, instant double, actual cue/loop behavior, atomic transport snapshots. |
| [FLX4 lib.rs](../crates/flx4-midi/src/lib.rs) | 13 ControllerAction kinds, buttons/knobs/global buttons, 8 pad modes, parsed messages, connection/history/feedback. |
| [library.rs](../src-tauri/src/library.rs) | SQLite local tracks/playlists/cues, separate Spotify analysis cache, XML import/export. |
| [spotify.rs](../src-tauri/src/spotify.rs), [spotify_decks.rs](../src-tauri/src/spotify_decks.rs), [spotify_analysis.rs](../src-tauri/src/spotify_analysis.rs) | Separate Spotify services, native worker preparation/cancellation, cache/batch analysis, read-only playlists and remote play/pause APIs. |
| [audio-analysis lib.rs](../crates/audio-analysis/src/lib.rs) | Current tempo, beat phase, key and three-band waveform analysis; no structural-section detector. |

The application is currently a Tauri Rust binary plus React frontend. There is no general product CLI or external control server. Special process arguments exist for a Spotify identity probe and a stdio Spotify playback worker; these are internal diagnostics/provider plumbing, not a product control interface.

## Action matrix: engine operations

In these tables, **E** means an existing engine operation, **A** an existing app/service operation, **F** frontend-owned behavior, **N** a needed new capability. Existing means the functionality exists in code, not that a CLI command exists. Public decks should be `A`/`B`; adapt to native indices `0`/`1`. Hot cues should use `A` through `H`; sampler slots should use `1` through `16` publicly and `0` through `15` internally.

| Existing EngineCommand | Proposed CLI | Current semantics / required parity |
| --- | --- | --- |
| `Play` | `deck A play` | E. Starts loaded track; when at end, returns to main cue first. Empty-deck play currently silently does nothing; API should return `deck_empty`. |
| `Pause` | `deck A pause` | E. Pauses at current position; idempotent. |
| `TogglePlay` | `deck A toggle-play` | E. Used by UI, keyboard and MIDI; retain for human gestures, prefer explicit play/pause in automation. |
| `Cue` | `deck A cue return` | E. Pauses and seeks to main cue. |
| `SetCue` | `deck A cue set --current` | E. Sets main cue to render position. |
| `SetCuePoint` | `deck A cue set --at 32.5s` | E. Explicit main cue position; currently clamps to duration. New contract must state validation/clamping. |
| `CuePress` | `deck A cue hold --duration 500ms` / `cue press`, `cue release` | E. Press while playing stops/returns; press while paused previews; release stops/returns only preview. Paired/leased gestures prevent stuck preview. |
| `Seek` | `deck A seek --at 48s [--snap beat]` | E; snapping currently F. Transport-independent seek; perform snapping in shared command logic. |
| `ScrubTo` | `deck A scrub to --at 20s --duration 16ms --gesture ID` | E. Audible traversal while touched, duration 8..100ms. |
| `ScrubEnd` | `deck A scrub end --at 20s --gesture ID` | E. Completes audible traversal and releases held cursor. |
| `JogTouch` | `deck A jog touch on\|off` | E. Hold/release; use gesture expiry on disconnect. |
| `Jog` | `deck A jog --delta 0.05s [--scratch]` | E. Bend, scratch or loop-boundary edit depending on state; expose explicit modes so automation does not infer a hidden mode. |
| `BeatJump` | `deck A jump --beats -4` | E. Signed beat jump, ±256 maximum. |
| `SetHotCue` | `deck A hotcue A set --current` | E. Captures render position. Need N explicit-time single-slot setter. |
| `TriggerHotCue` | `deck A hotcue A trigger` | E. Seeks and **starts playback**, unlike ordinary seek. Empty-slot trigger currently no-ops. |
| `ClearHotCue` | `deck A hotcue A clear` | E. Clears slot. |
| `RestoreHotCues` | `deck A hotcue replace --file cues.json` | E. Whole 8-slot replacement; require revision guard, duration validation and persistence. Avoid client read-modify-write to change one slot. |
| `MemoryCueSave` | `deck A memory save --current` | E. At most 10, sorted by time, merges positions within ~20ms; full list currently silently does nothing. |
| `MemoryCueDelete` | `deck A memory delete --nearest-current` | E. Deletes nearest cue, with no distance threshold; explicit cue identity/time deletion needed for agents. |
| `MemoryCueNext` | `deck A memory next` | E. Seeks to next point > current +30ms; preserves play state. |
| `MemoryCuePrevious` | `deck A memory previous` | E. Seeks to previous point < current −30ms; preserves play state. |
| `RestoreMemoryCues` | `deck A memory replace --file cues.json` | E. 10-slot replacement. Need explicit-time add/remove without changing cursor. |
| `SetLoop` | `deck A loop set --beats 8` / `loop off` | E. Active loop starts at preceding grid beat; `null` disables. Length >0..256 beats. |
| `ToggleActiveLoop` | `deck A loop toggle` / `loop reloop` | E. Stores/reinstates last loop; same operation for active-loop/reloop hardware. Add explicit enabled state API rather than retrying toggle. |
| `LoopIn` | `deck A loop in --current` | E. Captures position, disables active region. |
| `LoopOut` | `deck A loop out --current` | E. Captures valid out point, enables loop and moves to loop start. Need explicit region setter if authoring loops offline. |
| `SetLoopEdit` | `deck A loop edit in\|out\|off` | E. Requires active region; jog adjusts boundary. |
| `SetTempo` | `deck A tempo set --percent -2.5` / `tempo reset` | E. −90..100%; preparation worker may still be pending after acceptance. |
| `SetPitch` | `deck A pitch set --semitones 2` | E. −12..12; preparation completion matters. |
| `SetKeylock` | `deck A keylock on\|off` | E. Preparation completion matters. |
| `Sync` | `deck A sync align` | E. One-shot sync operation, distinct from enabled state. |
| `SetSync` | `deck A sync on\|off` | E. Persistent sync-enabled state. |
| `SetMaster` | `deck A master` | E. Assign timing master. |
| `SetBpm` | `deck A bpm set 124` | E+A. 30..300; app wrapper persists manual provenance. Half/double/restoring analyzed BPM/tap tempo are additional shared operations. |
| `SetBeatgrid` | `deck A grid set --offset 0.125s` | E+A. App wrapper persists manual provenance. Atomic BPM+offset update desirable. |
| `SetDeckGain` | `deck A gain set 1.2` | E. 0..2, neutral 1. |
| `SetChannelFader` | `deck A fader set 0.7` | E. 0..1. |
| `SetEq` | `deck A eq set --low 0.5 --mid 1 --high 1.1` | E. Each 0..2, neutral 1; N partial-band domain setter prevents overwriting other agents/MIDI values. |
| `SetFilter` | `deck A filter set -0.4` | E+A. −1..1, neutral 0; shared command honors Smart CFX as UI does. |
| `SetHeadphoneCue` | `deck A headphones on\|off` | E. Deck cue bus; distinguish from transport main cue. |
| `SetFx` | `deck A fx set --echo 0.3 --reverb 0` | E. Wet amounts 0..1; N partial effect setter. Controller FX selection/enabled state needs separate semantics below. |
| `SetFxBeat` | `deck A fx beats set 0.5` | E. 1/32..16 beats. |
| `SetCrossfader` | `mixer crossfader set -0.6` | E+A. −1 = A, +1 = B; app wrapper honors Smart Fader. |
| `SetMasterGain` | `mixer master-gain set 0.8` | E. 0..2. |
| `SetCueGain` | `mixer cue-gain set 1` | E. 0..2. |
| `SetCueMix` | `mixer cue-mix set 0.5` | E. 0..1. |
| `TriggerSample` | `sampler slot 9 trigger` | E. Restarts sample from beginning. |
| `StopSample` | `sampler slot 9 stop` | E. Stops playback. |
| `SetSamplerGain` | `sampler gain set 0.8` | E. 0..2. |

Every one of the 49 variants is accounted for above. Restore operations can be advanced schema-driven commands; users should have named operations for ordinary actions. A generic `command --file request.json` may expose the same typed registry for tooling, but must not bypass validation, guards or shared semantics.

## Action matrix: application and library services

| User function | Proposed commands | Current backing and gaps |
| --- | --- | --- |
| Get current state | `status`, `state get [--scope deck.A,mixer]`, `state watch` | A: get_app_state/get_engine_snapshot/get_engine_transport/get_engine_metadata/get_activity_state. Current transport polling ~33ms, activity ~1s; recording polled separately. Controller, Spotify account and UI state need joining into the query contract. |
| Select a local song | `browser select --track local:42` | A+F: native controller index is derived from track ID/context; actual highlighted row/collection/focus owned by React. |
| Load local song | `deck A load --track local:42` / `--path /music/song.flac` | A: load_track/load_into_deck, canonical path, decode, library upsert, persisted cue/grid restore. Add ID lookup and a shared playing-deck replacement policy. |
| Load generated test tone | `deck A load --demo` | A: load_demo. |
| Instant double | `deck B instant-double --from A` | A: instant_double uses other deck. Current frontend double LOAD <500ms gesture; API should use explicit intent. |
| Unload a deck | `deck A unload` | E-only EngineHandle::unload exists for account invalidation; no ordinary UI/Tauri operation. Useful N public capability, included as extension. |
| Cancel source preparation | `deck A load cancel --job ID` | A for Spotify only. Local decode/import/reanalysis lacks equivalent public cancellation. Add job-scoped cancellation, not an unguarded cancel of whichever job is now current. |
| Import music | `library import --path ... [--path ...]` | A: choose_tracks dialog + import_tracks (≤10,000). Partial failures accumulate in progress/errors, method returns successes. Explicit paths replace dialog. A folder/recursive importer would be N. |
| Find/list songs | `library tracks list --search ... --sort title\|bpm\|duration --playlist local:7` | F: UI searches combined title/artist/BPM/key text and sorts local library. Move query into shared service, add bounded pagination/metadata detail. |
| Inspect a song | `library track get local:42 [--include analysis,cues,waveform]` | A data in library/app snapshots; add direct efficient query instead of full collection/waveforms. |
| Remove library song | `library track remove local:42` | A: delete_track removes DB record and playlist membership, **does not delete audio file** or unload deck. Document it and guard persistence against recreating removed metadata unexpectedly. |
| Create local playlist | `playlist create --name Work --source local` | A: create_playlist trims name, unique name returns existing playlist. Make create/upsert behavior explicit. |
| Add song to playlist | `playlist add local:7 --track local:42` | A: add_to_playlist append, duplicate ignored. Playlist items are unique track IDs, so repeated song entries are not supported. |
| Read playlists/order | `playlist list`, `playlist get local:7` | A: library.playlists returns ordered track IDs; UI may display a separately sorted list. Distinguish playlist order from browser sort. |
| Rename/delete/remove/reorder playlist | `playlist rename\|delete\|remove\|move\|replace` | N: missing in UI and service today. Required for useful complete playlist management, separate from current parity. |
| Import rekordbox XML | `library import-rekordbox --path set.xml` | A: dialog-based import, tracks + tags/BPM/grid/hot/memory cues + playlist hierarchy flattened into names. Partial failure progress. Path-based service needed. |
| Export rekordbox XML | `library export-rekordbox --path set.xml [--overwrite]` | A: export_rekordbox_xml currently save dialog. Make exact destination and overwrite result explicit. |
| Reanalyze loaded audio | `deck A analyze [--wait]` | A: reanalyze_loaded_track; preserves manual metadata according to wrapper rules. Offline library-only reanalysis is N if not imported/loaded. |
| Analysis status/cancel | `jobs list\|get\|watch\|cancel` | A: local import progress and Spotify batch/source status are different shapes with no common IDs. N uniform job model; local cancellation requires checkpoints. |
| Sample assignment | `sampler slot 9 load --path kick.wav` | E load_sample allows 16; A choose_sample dialog **rejects slots ≥8 although UI/MIDI expose two banks/16**. Fix parity and add path-based loader. |
| Sample inspection | `sampler list`, `sampler slot 9 get` | N authoritative slot metadata/play state. Current names live in React; EngineSnapshot only sampler gain, no sample contents/positions. |
| Recording start/stop/status | `recording start --path mix.wav`, `recording stop`, `recording status` | A: start_recording save dialog, stop_recording/get_recording_status. E rejects recording while any service source is loaded; preserve actual recording_allowed capability. Need job/applied/file-finalized acknowledgments. |
| Audio device discovery | `audio devices list [--refresh]` | A: list_audio_devices (3s discovery timeout/cache). Return discovery-in-progress separately from missing devices. |
| Start/configure audio | `audio start --device NAME --buffer 256 --master 1,2 --cue 3,4` | A: start_audio + AudioRouting; native channel indices 0-based. Public convention can be 1-based, unambiguous in help/schema. Saves audio preferences. |
| Stop audio | `audio stop`, `audio status` | A: stop_audio serializes with generations. Preserve rendering lifecycle and state when no hardware is running; do not claim queued control already rendered. |
| MIDI ports/connect/disconnect | `midi ports`, `midi connect --input NAME [--output NAME]`, `midi disconnect` | A: list_midi_ports/connect_midi/disconnect_midi. Explicit disconnect disables auto-reconnect; reconnect enables it. Output optional; LED feedback depends on it. |
| MIDI history | `midi history [--limit N]`, `midi watch` | A: get_midi_history bounded raw events; N event stream. Redact credentials from all unrelated diagnostics. |
| Controller tempo range | `controller deck A tempo-range set 6\|10\|16\|100` | A: get_controller_surface/set_controller_tempo_range; changes physical tempo interpretation, not actual percent by itself. |
| Smart modes | `mixer smart-fader on\|off`, `mixer smart-cfx on\|off`, `mixer smart-cfx preset filter-echo\|filter-reverb` | A settings and controller globals. Explicit preset selection N wrapper around existing cycle logic. |
| Controller browse context/ownership | `browser context get\|set`, `controller browser-owner set runtime\|view` | A: set_controller_selection/context/browser_active. Public semantic browser should make these internal adapters, retain diagnostics. |

## Action matrix: MIDI and controller behavior

All 13 parsed ControllerAction kinds need audit coverage. The raw profile can remain diagnostic (`controller action --file action.json`), but normal CLI names should expose the meaningful action without requiring raw note/CC knowledge. MIDI is an input adapter, not the business-logic owner.

| Action kind | Current interpreted behavior | Required CLI parity |
| --- | --- | --- |
| `Button` | Play toggle; cue press/release; shifted cue = seek zero; headphone toggle; sync toggle/off; master; range cycle; auto-loop 4 beats/off; active/reloop; manual in/out versus loop-boundary editing; tap BPM; memory previous/next versus loop half/double; loop half/double; fader start/cue; memory save/delete. | Named deck operations above plus `bpm tap`, `bpm restore-analysis`, `loop half\|double`, `tempo-range next`, gesture press/release. **Shift alone has no audible action**; parser stores modifier. `SyncOff`/`Reloop` variants have handler branches but no normal parser mapping in current note table. Shifted LoopIn/Out variants currently have no handler action. |
| `Knob` | Tempo via range, trim, fader, EQ, filter/Smart CFX, crossfader/Smart Fader, master gain, headphone mix/gain, FX depth. | Explicit absolute setters, partial EQ/effect updates, optionally `control adjust --delta`; render values and physical interpretation in schema. **MicGain is parsed and ignored; no software mic path.** |
| `JogTouch` | Hold/release platter with loaded-source guard; skipped while loop editing. | Gesture commands with source revision and expiry; native loop-edit operation explicit. |
| `Jog` | Bend; scratch; shifted accelerated scratch/search; editing loop boundary. | `jog --mode bend\|scratch\|loop-in\|loop-out`; structured trajectory/leased stream for continuous input, not thousands of shell processes. |
| `PadMode` | HotCue, PadFx1, BeatJump, Sampler, Keyboard, PadFx2, BeatLoop, KeyShift; sampler opens UI and chooses bank. | `controller deck A pad-mode set MODE`; reflect LEDs and UI. UI currently has its own hotcue/loop/memory view, so distinguish physical pad mapping from UI display. |
| `Pad` | Hot cue set/trigger/delete; beat jump steps with shifted scale halves/doubles; beat-loop lengths; press/release Pad FX; keyboard/key-shift pitch banks, keyboard triggers cue A; sampler play/shift-stop. | `controller deck A pad 1 press\|release [--shift]` for exact gesture parity; also named `jump-scale`, `pitch-bank`, `padfx hold`, `sampler trigger/stop`, etc. Expose mode, banks and scales in state. |
| `Browse` | Normal rotation chooses source/collection/song via frontend; native local fallback wraps index while React navigation clamps. Shifted rotation zooms waveform. | Shared `browser move --delta N`, `ui waveform zoom ...`; preserve intentional navigation boundaries and eliminate native/view disagreement. |
| `Load` | Normal loads selected song, rejects playing deck in frontend, double press performs instant double. Shift A = local related artist/title search or Spotify album. Shift B = drawer toggle. | `deck load --selected`, explicit instant-double, `browser related`, `ui browser toggle`. Central playing-deck protection; direct UI buttons currently bypass MIDI-only warning. |
| `GlobalButton` | MasterCue toggles both headphone cues; browse press/back walks home/footer/playlists/tracks/close; PanelForward toggles drawer; SmartFader/SmartCfx toggle; SmartCfxNext cycles two presets. | `mixer master-cue on\|off`, `browser enter\|back`, presentation open/close, smart mode/preset commands. **SmartFaderNext and AndroidMono are parsed but currently ignored.** |
| `FxSelect` | Cycles Echo/Reverb/Filter on global selected FX target; clears previous filter where appropriate. | `controller fx type set echo\|reverb\|filter`, `fx type next\|previous`. Preserve reset effects. |
| `FxBeat` | Cycles 10 beat fractions; shifted positive = tap BPM, shifted negative restores analyzed BPM. | `controller fx beats set`, `fx beats next\|previous`, `deck bpm tap\|restore-analysis`. |
| `FxEnable` | Toggles selected-target FX; shifted press pauses deck, selects/enables echo (“release FX” behavior). | `controller fx on\|off`, `controller fx release`; report compound changes. |
| `FxTarget` | Selects global target A/B only when selected=true. | `controller fx target set A\|B`. |

LEDs, meters and loaded acknowledgments are derived output feedback, not separate musical commands. Existing `set_vinyl_mode` MIDI output is setup plumbing; if made public, classify it as controller configuration and keep it synchronized with actual gesture interpretation. Unknown/unsupported raw controls must return unsupported capability, not a successful no-op. Hardware-side behavior outside software must not be promised as software control.

## Action matrix: Spotify

Three playback surfaces must be named separately: **native deck** (PulseDeck mixer/cues/DSP), **Spotify Connect** (remote device), and **official iframe widget** (provider-controlled embedded player).

| User function | Proposed commands | Existing behavior / gap |
| --- | --- | --- |
| Library config/status | `spotify configure --client-id ID`, `spotify status --account library` | A spotify_configure/status; client ID public, tokens stay in native vault. Changing ID clears active native sources through current wrapper. |
| Library sign-in | `spotify login --account library [--playback-scope]` | A spotify_begin_login; browser consent + callback, status checked separately. N `--no-open` returns URL for CLI use. Human authentication remains required. |
| Native sign-in/status | `spotify login --account native`, `spotify status --account native` | A spotify_native_begin_login/status, separate credentials/session. |
| Disconnect | `spotify disconnect --account library\|native` | A disconnect wrappers cancel analysis/preparation and unload Spotify decks. Expose affected decks as result. |
| List collections | `spotify playlists list --offset 0`, `spotify liked list --offset 0` | A spotify_playlists/liked_tracks, 50-item pages. |
| Playlist/album songs | `spotify playlist tracks ID --offset 0`, `spotify album tracks ID --offset 0` | A playlist_tracks/album_tracks. Stable URI IDs, next_offset. Album API currently also takes display album_name; remove display text as required identity. |
| Native deck loading | `deck A load --uri spotify:track:...` | A load_spotify_track prepares PCM in isolated worker, one preparation at a time while loaded decks mix independently; native sign-in required. |
| Cancel preparation | `jobs cancel ID` / `deck A load cancel --job ID` | A cancel_spotify_load, N job ID/revision-safe adapter. |
| Cached analysis / batch | `spotify analysis get --uris ...`, `spotify analyze --uris ...`, `jobs watch\|cancel ID` | A cached_analysis/analyze_spotify_page/status/cancel. Current page jobs max 50; versioned cache. Must not confuse page batch progress with deck load progress. |
| Connect devices | `spotify connect devices [--refresh]`, `ui spotify device select ID` | A spotify_devices; F selected device default. Domain play can pass device ID directly. |
| Connect play/pause | `spotify connect play --uri ... --device ID`, `spotify connect pause --device ID` | A spotify_play/pause; response means remote request sent, not verified playback. UI `playingUri` is optimistic commanded state. Need verified remote playback query as N if used in agent decisions. |
| Widget open/close/link | `ui spotify widget open --uri ...`, `ui spotify widget close`, `ui spotify widget-link set ...` | F iframe/link input. Open/close presentation is callable; iframe's internal play/pause/volume/fullscreen cannot currently be driven through PulseDeck. Report this boundary in capabilities; provider integration would be separate work. |
| Edit Spotify playlists | `spotify playlist create\|add\|remove\|reorder ...` | N. Current scopes include playlist reads, library reads and playback, **no playlist modification scopes or write APIs**. Requires actual new integration and consent; local playlist creation is immediately feasible first. |

Do not emit vault material or bearer tokens in JSON/status/events. Source recording policy must remain enforced: current native Spotify tracks set recording_allowed=false.

## Literal UI presentation coverage

These functions are currently frontend state. They should become fields in an authoritative UI session model and named `ui` operations. Domain commands also update any corresponding visible state. Multiple views require view IDs; default is the primary desktop view. `ui state get` returns desired and rendered revisions, mounted/connected state and applicable draft values.

| UI function | Proposed commands | Notes |
| --- | --- | --- |
| Source choice / browser open-close | `ui browser source set local\|spotify`, `ui browser open\|close\|toggle` | Source chooser, footer buttons, panel shortcut, shift-load-B, Escape and close buttons share state. |
| Drawer resizing | `ui browser height set 28\|44\|70` / `height adjust --delta 8` | Pointer currently allows 10..72dvh and snaps to 28/44/70; drag-to-load contracts to 28, <18 closes; keyboard Home/End and ±8. Include raw size + snap/close semantics for gesture parity. |
| Browser selection/focus/navigation | `browser select --track ...`, `browser collection set ...`, `ui browser focus footer\|playlists\|tracks\|close`, `browser enter\|back\|move` | Selected ID/source/collection survives filtering by identity; distinguish selection from focused DOM element. No requirement to open drawer for loading explicit track ID. |
| Search/filter/sort | `browser search set TEXT`, `browser sort set title\|bpm\|duration`, `browser related --track ...` | Local related action uses artist or title; Spotify related action opens album. Expose clear search/collection setters and the visible result IDs/revision. |
| Local playlist creation form | `ui playlist-form open\|close`, `ui playlist-form name set TEXT`, `ui playlist-form submit` | Equivalent direct `playlist create` exists; form draft is still callable for literal parity. |
| Spotify collection/album/paging UI | `ui spotify collection set liked\|playlist:ID\|album:ID`, `ui spotify playlists more\|refresh`, `ui spotify tracks more\|refresh`, `ui spotify focus playlists\|tracks` | Backend list query pagination and rendered accumulation are separate operations; pending fetch generation prevents stale results. |
| Spotify analysis preference/page | `ui spotify auto-analyze on\|off`, `ui spotify analyze-current-page` | Auto-analysis currently only initial selection, max current page 50, not entire collection; command help must reflect it. |
| Spotify setup/error/drafts | `ui spotify setup open\|close`, `ui spotify client-id set ID`, `ui spotify error dismiss`, `ui spotify connection refresh` | Two mounted SpotifyPanel instances may hold independent drafts today; define primary browser/settings view target. |
| Waveform zoom/snap | `ui waveform zoom set --beats 8\|16\|32\|64\|128`, `zoom in\|out\|reset`, `ui waveform snap on\|off` | Existing visible-beat range 8..128, default 32. Overview click and scrolling scrub map to deck seek/gesture commands; snap must have authoritative semantics. |
| Deck quick-load menu | `ui deck A menu open\|close` | Demo and first 12 local track menu items call ordinary load. Drag-and-drop track/song load uses same domain operation; no pixel dragging required. |
| Grid editor | `ui deck A grid-editor open\|close`, `draft set --bpm 124 --offset 0.1s`, `half\|double\|apply` | Opening seeds loaded BPM/grid; choosing tempo candidate fills draft then opens. Domain `deck grid set --bpm ... --offset ...` commits both atomically. |
| Analysis details/candidate | `ui deck A analysis-details open\|close`, `ui deck A analysis-candidate choose --bpm 124` | Current details popover + candidate opens grid review, not immediate BPM application. |
| UI pad view | `ui deck A pads set hotcue\|loop\|memory` | Independent from hardware's 8 physical modes; eliminate misleading state where physical mode/visible view diverge without explanation. |
| Mixer FX display | `ui mixer fx-view set echo\|reverb` | Selects which sliders render, does not itself alter audio. Hardware FX kind/target/on remain separate shared controller state. |
| Sampler display/bank | `ui sampler open\|close`, `ui sampler bank set 1\|2` | Sampler physical action opens display/chooses bank; assignment/name comes from native slot metadata. |
| Settings open/tab/drafts | `ui settings open [--tab appearance\|audio\|midi\|spotify\|library\|roadmap]`, `ui settings close`, `ui settings audio-draft set ...`, `ui settings midi-draft set ...` | Draft device/buffer/routing and chosen MIDI names are distinct from applied config. `ui settings apply` dispatches audio/MIDI domain actions with real results. Roadmap is the existing informational final tab. |
| Theme | `ui theme list`, `ui theme set hardware-dark\|anatomy-light\|anatomy-dark` | Today persists only localStorage. Migrate/mirror preference with UI refresh acknowledgment; keyboard navigation same setter. |
| Notices and errors | `ui notice dismiss`, scoped `ui error dismiss --target ...` | Notice close, Spotify error close; analysis inline errors may clear on retry. Errors remain in job history even after visual dismissal. |
| Dialog entry points | `ui dialog open --kind tracks\|sample\|recording\|xml-import\|xml-export` | Exact human UI affordance can be callable, returning waiting-for-user job. Normal CLI uses paths and avoids needing dialogs. |
| External links | `ui link open --target spotify-dashboard\|spotify-auth\|spotify-track` | Existing anchor/browser actions. Expose only meaningful declared product targets, not arbitrary script/DOM injection. |

Keyboard shortcuts, double-click resets, pointer controls, dropdowns and pad shift gestures must map to catalog operations too: Space = A toggle, Enter = B toggle unless browser owns navigation, Cmd/Ctrl+I = import dialog, Escape closes active presentation, arrow keys jog/browser/resize/theme as appropriate. Knob double click uses its declared reset (gain/EQ=1, filter=0, cue mix=0); crossfader double click=0; tempo zero button=0%. Read exact reset values from component props in implementation. A named `control reset --target ...` exposes each reset without synthetic input.

The final coverage gate must inventory all actionable controls/handlers, including details toggles, form submissions and gesture-end/cancel handlers. Decorative animations, inferred peaks and drag hover styling are observations; user initiation and release/cancel have semantic commands. Browser text drafts are callable, while tool consumers normally call the direct domain operation.

## Architectural proposal

### One typed command registry and dispatcher

Introduce shared request/response/schema types (suggested `crates/control-protocol`) and a product dispatcher/service layer, initially hosted in the existing desktop process. Extract reusable business operations from Tauri command wrappers and controller handlers into explicit services. The initial socket server should call that dispatcher inside the existing AppState owner; do not launch a second EngineHandle, audio device owner, Spotify session or independent SQLite writer for each CLI invocation.

```text
React controls ── Tauri adapter ─┐
FLX4 MIDI ───── semantic adapter ├── typed product dispatcher
pulsedeck CLI ─ local IPC client ┘       ├── deck engine / render queue
                                        ├── library + cue persistence
                                        ├── audio / MIDI / Spotify services
                                        ├── jobs / scheduler / events
                                        └── UI session state + view adapters
future MCP ─── CLI or same protocol client
```

Registry descriptors include name, arguments/types/units/ranges, enums, output schema, side effects, relevant resources, idempotency, wait conditions, source restrictions, UI requirement and supported/unsupported status with reason. Generate CLI help, JSON schema, capabilities, TypeScript bindings and future MCP descriptors from it. Use named typed operations rather than exposing raw Rust internals as the stable public API.

Use a user-private Unix socket on this macOS runtime with owner-only permissions, instance handshake and protocol version. Keep transport payloads portable for future named-pipe support. Client disconnect is not engine shutdown. `pulsedeck app status\|start\|stop` distinguishes control-client process from product runtime; initial implementation can report `app_not_running` and optionally launch the existing desktop executable. Headless ownership can follow after runtime extraction. UI-only actions in headless mode return `ui_unavailable` or an explicitly requested staged result; never pretend a view rendered.

During extraction preserve Spotify worker launch identity: `spotify_decks` uses `current_exe()` for `--spotify-worker`. A standalone CLI executable is not automatically that worker. Runtime must own/provider-configure its worker executable explicitly before headless extraction.

### State and acknowledged completion

Existing EngineHandle::command returns success when it queues a command. Render processing drains at most 64 per callback; time-stretch can finish later. Track metadata/load revisions can publish before the render callback applies the queued asset. Therefore success must have a stage:

* **accepted**: validated/admitted, has request/job ID.
* **applied**: native render/control state applied, with command sequence, source revision and render frame; UI actions applied to authoritative model.
* **ready**: asynchronous decode/preparation complete and usable for dependent playback operations.
* **persisted**: required library/preference transaction committed.
* **rendered**: mounted view acknowledged corresponding UI revision.

Commands default to the stage useful for that operation (e.g. persisted for metadata, ready for load, applied for play), have finite `--timeout`, and offer `--no-wait` plus `jobs wait`. Timeout means status is uncertain/pending, not automatically failed/cancelled. Retry uses the same idempotency key. Remote Connect acknowledgment is `sent` unless verified by a playback query.

Add sequence IDs and acknowledgment channel/atoms without allocation/blocking in the audio callback. Reuse the existing atomic transport snapshot approach; it already has a seqlock-style consistent read, so there is no need to replace it with callback locks. Publish cheap lean state (positions/play/mixer/meters) separately from library/waveform metadata. Composite queries should declare revisions and capture coherent command-visible source identity; separate native metadata and transport are currently different lifecycle stages.

`state get` includes instance/version, state revision, deck source IDs/load revisions, playing/cue/loop/sync/tempo/pitch/preparation state, mixer, sampler contents/play state, audio routing/status, controller modes/FX/banks, MIDI status, library/playlists revisions, jobs, source limitations and desired/rendered UI state. Rich metadata/waveforms are opt-in. `events watch --since SEQ` produces NDJSON typed changes, job progress, command application, device/account changes, persistence errors and bounded transport samples. Gap/overflow returns resync required; do not stream 4096-point waveforms every frame.

### Persistence, validation and concurrency

Fix ownership asymmetries before calling parity complete:

* Most MIDI commands currently call engine.command directly while UI calls app engine_command, which adds smart mode interception and BPM/grid persistence. Both must use shared operations.
* Controller EQ arrays and FX/loop/bank state are separate from engine values. A MIDI single-band EQ change can resurrect stale values set in UI/CLI. Partial-band updates must read/update authoritative control state atomically, not snapshots on clients. Controller FX enabled/type and direct wet sliders need a defined synchronization model.
* Cue persistence currently polls every 100ms, ignores DB errors, and uses local path updates only after same-track observation. This is not a durable edit acknowledgment. Spotify analysis rows store TrackAnalysis, which has **no main/hot/memory cue fields**, and Spotify load restores analysis without cue restoration. Add source-keyed cue metadata for local and Spotify tracks and explicit save/error acknowledgment.
* Loading the same track on both decks can otherwise produce competing persisted cue sets. Source-keyed metadata revisions and a defined broadcast/merge rule are required. Suggested: track metadata is authoritative; commits update loaded instances or reject conflicting stale edits.
* Explicit deck metadata edits use `expected_load_revision` and track ID, so queued commands cannot modify the next song. UI selection operations use browser revision when relying on an ordinal. Offline track edits use metadata revision.
* Existing restore lists accept finite nonnegative times without enforcing track duration. Engine seek/cue may clamp while no-op operations silently succeed. Typed service validation should reject invalid/empty/full/unsupported operations explicitly and document intentional idempotent clear/off outcomes.
* Centralize playing-deck load policy: default reject replacement while playing; explicit `--replace-playing` documents intentional effect; instant double is its own action. Today the MIDI frontend warning is bypassed by direct UI buttons/native load calls.

Primary setters are deterministic; human toggle/cycle gestures remain available. Relative adjustments compute against server state. Library edits use transactions, result IDs and revisions. Filesystem outputs require explicit destinations with no-overwrite default. Multi-client controls expose manual/MIDI/agent origin and resource ownership; a manual move of an automated knob should cancel/suspend its ramp by default, with documented configurable takeover behavior.

### Stable CLI and machine contract

Proposed executable: `pulsedeck`; optional `pd` alias. `--json` is single-result JSON; watch commands use NDJSON. Diagnostics go to stderr; stdout must stay parseable. Public units are explicit (`s`, `ms`, `beats`, gain, percent, semitones), deck identities A/B and source-scoped IDs `local:42`, `spotify:track:...`. A local playlist is `local:7` inside the playlist namespace; protocol types additionally carry entity kind, so a track and playlist with the same numeric ID cannot be confused. Resolve paths before dispatch and return canonical source identity. Avoid ambiguous title matches; search returns candidates/IDs.

Illustrative envelope:

```json
{"protocol_version":1,"request_id":"r-123","ok":true,"stage":"applied","state_revision":81,"result":{"deck":"A","playing":true,"source_id":"local:42","load_revision":7,"render_frame":124928}}
```

Illustrative error:

```json
{"protocol_version":1,"request_id":"r-124","ok":false,"error":{"code":"source_changed","message":"Deck A now contains another track","retryable":false,"details":{"expected_load_revision":7,"actual_load_revision":8}}}
```

Define stable errors and documented exit codes: syntax/validation (2), instance unavailable (3), state conflict/unsupported operation (4), asynchronous operation failure (5), timeout (6), cancellation (7). Useful structured codes include deck_empty, cue_empty, cue_capacity, track_not_found, playlist_not_found, busy, queue_full, source_changed, revision_conflict, auth_required, source_unavailable, recording_prohibited, ui_unavailable, command_unsupported, protocol_mismatch and persistence_failed. Error codes remain stable when text changes. Retryability and job ID distinguish queue backpressure from a failed operation. Credentials never appear in traces.

`capabilities --json`, `commands list --json`, `commands describe deck.hotcue.set --json` report versioned schemas and current availability. Help/examples must be generated from exactly the operations clients can invoke. `command --file request.json` and a long-lived `session --stdio` can carry the same protocol for efficient scripts/future tools. MCP should later call this protocol/CLI and preserve its errors rather than scrape human help or drive the UI with clicks.

## Cue authoring and music analysis for agents

Adding points at major sections needs two capabilities that are not supplied by the current DSP analyzer:

1. **Non-disruptive metadata editing:** `library track cue set --track local:42 --kind hot --slot A --at 32s --label Intro`, `library track memory add ...`, and batch replace/apply. Works unloaded, persists by stable source ID, validates duration/revision, optionally updates all loaded instances. For hot/memory cues the existing engine has current-position capture and whole cue-array restore; the main cue already has an explicit-time SetCuePoint operation. Add native atomic hot/memory slot/time setters instead of seek→set→seek, which disrupts playback and races moving playheads. Labels/colors/section/provenance require schema changes; current cues are times only. Preserve 8 hot/10 memory limits or deliberately extend the product with an additional unlimited section-marker collection.
2. **Structural analysis:** `analysis sections --track ...` should eventually return candidate times, estimated musical role (intro/build/drop/break/outro), confidence/provenance and alternatives. Current audio-analysis provides tempo/phase/key/waveforms only; the four-beat grouping is not a verified musical downbeat. Add downbeat/phrase/section inference separately and benchmark it on varied music. Do not advertise “major part” detection based on waveform peaks alone.

Suggested workflow: analyze/propose → inspect candidate sections → apply cues with explicit replace/preserve policy and capacity allocation. Unknown/low-confidence labels remain candidates. Analysis jobs operate away from audio thread and use source cache/policy; no analysis/network work in render callback. User intent to “add cues at major parts” can authorize application, but automated overwrite behavior must be a concrete tool parameter, not an accidental consequence of filling every slot.

Playlists need native list/query/create/add now; remove/reorder/rename/delete and full-collection pagination next. A DJ setlist/queue may need repeated tracks, preparation status and transition plan, unlike current unique-membership playlists. Native mixed-source playlists need source-aware items if Spotify songs are to coexist with local records; current local membership foreign keys only reference local tracks.

## Chaining and native timing

Shell chains are useful for setup, but sub-process startup and model/tool latency must not time a live mix. Provide `batch validate`, `batch run --file plan.json`, `jobs wait`, and a runtime scheduler with explicit dependencies and guards.

* Ordered batches stop on error by default, return every executed step/result, and make partial application visible. Validation/dry-run is side-effect-free. Do not promise rollback of audible actions; pure library transactions can be atomic separately.
* Dependent steps wait for required stage (`load.ready`, metadata.persisted, pitch.preparation_ready) instead of sleeping. A load failure must prevent playing the wrong old track.
* Server-side timing supports relative monotonic time, absolute render-frame deadlines, and deck beat/phrase targets based on BPM/grid plus source revision. Bar timing requires declared meter/downbeat confidence; initial fixed 4/4 grouping is a user-defined grid convention, not verified analysis.
* Add native scheduled controls and ramps (`mixer crossfader ramp --to 1 --over 16beats --clock A`, deck filter/fader/gain/EQ ramps); sample accurate/tight bounded timing must be implemented in render scheduling, not by sending a socket command per desired sample. Existing DSP smoothing is not a programmable timed ramp.
* Policies explicitly define what pause/seek/loop/tempo changes, audio restart, device loss, source replacement and late deadlines do (cancel/freeze/rebase/execute-late/reject). Render frames do not progress normally without running audio; do not conflate them with wall clock. Persisted sessions cannot blindly resume old source revisions.
* Scheduler cancellation stops future steps and ramps, releases held gestures, and returns actual final values; it cannot undo already-audible audio. Manual intervention can revoke an agent lease on a resource. Provide one `automation stop --all` action, current owner/plan visibility and event receipts.

Proposed human-facing examples (not runnable yet):

```sh
pulsedeck library tracks list --search "ambient" --sort bpm --json
pulsedeck playlist create --name "Work session" --source local --json
pulsedeck playlist add local:7 --track local:42
pulsedeck deck A load --track local:42 --wait ready
pulsedeck deck A play
pulsedeck deck A hotcue A set --at 32s --expected-load-revision 7
pulsedeck deck A filter set -0.25
pulsedeck deck B load --track local:61 --wait ready
pulsedeck deck B sync on
pulsedeck mixer crossfader ramp --to 1 --over 16beats --clock A
pulsedeck events watch --topic decks,jobs,automation --json
pulsedeck ui theme set anatomy-dark
pulsedeck ui browser source set spotify
pulsedeck ui waveform zoom set --beats 64
pulsedeck batch validate --file work-session.json
pulsedeck batch run --file work-session.json --json
```

A batch transition should preload B, await ready, verify source identities and grid, prepare/await tempo, set incoming fader/filter, schedule incoming play and crossfade on a declared beat, then pause outgoing only after ramp completion. “DJ for me” also needs agent reasoning about compatible tracks, energy, phrase structure and monitoring; CLI exposure alone does not infer a good transition.

## Implementation phases and acceptance gates

Phases are sequencing, not permanent omissions. The completion target remains literal UI/MIDI parity.

### 1. Contract, shared dispatcher and desktop IPC

Create protocol/registry, local socket client/server and CLI help/JSON/errors/capabilities. Host server inside current desktop runtime. Start with state, play/pause, load/demo, seek/cue, gains/EQ/filter/faders and audio status to verify ownership and round trip. Refactor those UI/MIDI adapters onto the same dispatch. Add source guards and accepted/applied/ready receipts from the beginning; legacy handler wrappers can forward during migration.

Gate: CLI affects the same deck the UI/MIDI observes; no second audio owner; stable empty/not-running/queue/source-change errors; malformed requests cannot affect audio; UI/MIDI smart-mode semantics equal CLI; no callback allocations/locks introduced by control acknowledgments. Commands/schema version are independently tested.

### 2. Full native/service parity and durable metadata

Cover all 49 engine variants, all implemented controller mode/shift behaviors, smart/FX controller state, explicit-time/offline cues, reliable source-keyed persistence and MIDI feedback. Expose path-based recording/sample/XML/import equivalents. Fix 16-slot assignment, partial EQ/FX updates, load policy, cues on Spotify and same-track two-deck conflicts. Introduce uniform jobs/cancel/status/events; expose all existing Spotify APIs and their separate playback/account surfaces. Implement local metadata and playlist queries/operations required for agents.

Gate: coverage manifest accounts for every enum/registered handler, with unsupported parsed controls explicitly marked; restart preserves committed cues/grid; CLI can import/list/create playlist/load/mix/record/export without dialogs; source replacement never receives stale cues or actions; Spotify/non-recordable policy unchanged; all 16 sample assignments visible in state; failed persistence is surfaced, not reported durable.

### 3. Literal presentation parity

Move frontend presentation/preferences/browser/drafts into a typed UI session model; views subscribe and acknowledge rendered revisions. Convert UI controls and MIDI browser events to this model. Implement every row of presentation matrix, including theme, selection/search/sort/focus/page state, drawer resize, waveform zoom/snap, menus/grid/analysis editor, bank/pad/FX views, dialogs/notices/settings drafts.

Gate: UI→CLI state matches and CLI→UI rendering acknowledges; hidden/unmounted views return accurate status; mounted settings/browser instances have clear targeting; every actionable UI control has a catalog command/coverage record; keyboard and gesture resets use identical setters. UI actions unavailable in headless mode are discoverable rather than silent.

### 4. Batches, automation and live DJ control

Add guarded dependency batches, runtime scheduler, native ramps/beat execution, persistent plan/job introspection, cancellation/manual takeover, and reliable preparation waits. Keep heavy decode/analysis/job work off render thread. Expose setlist/queue editing and staged transition plans.

Gate: deterministic audio fixtures prove timing/ramp duration/ordering; late/cancel/seek/pause/restart/source-change cases behave by documented policy; unattended agent can execute a two-deck transition without shell timing; manual controls safely take over; status/events reveal actual progress and final values.

### 5. Section analysis and MCP adapter

Add section/downbeat/phrase analysis with evidence/confidence, candidate cue planning/apply, richer cue annotations and optional provider playlist writes with correct scopes. Thin MCP tools wrap typed capability/query/command/batch/job interfaces. Natural-language intent resolves into inspectable guarded plans; the runtime remains timing owner.

Gate: cue planning does not disturb playback, preserves protected existing cues, handles more sections than slot capacity explicitly, and persists results; section accuracy measured on representative tracks; MCP produces the same results/errors as CLI, and unsupported capabilities are discoverable.

## Coverage and verification plan

Maintain an implementation manifest with action ID → UI control/keyboard/gesture handler → MIDI mode/shift mapping → command schema → service → observed result → persistence policy. Include all 54 Tauri handlers (query, dialog or domain adapter) and every actual engine/controller dispatch path. New actionable UI or MIDI behavior without a mapped operation should fail a coverage review/check; enum exhaustion alone misses frontend controls and ignored variants.

Required tests focus on contracts and behavior, not mirroring setter implementations: IPC protocol round trip/version/errors; CLI parser units/JSON stdout; UI/MIDI/CLI equivalent compound smart-mode behavior; async application/readiness receipts; source revision and stale-job races; DB restart/Spotify cue persistence; same-song two-deck editing; sample bank two assignment; batch partial failure; held-gesture cleanup; scheduler timing/cancellation/takeover using generated audio fixtures; snapshot/event resync and bounded backpressure. Use a manual hardware/audio smoke check for actual device/MIDI feedback, routing and audible transitions because software tests cannot establish physical output.

The map is ready for implementation review. It deliberately records unsupported parsed controls and new AI/music features as gaps, so “everything” can be measured against evidence instead of a handpicked list of convenient commands.

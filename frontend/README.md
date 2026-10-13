# PulseDeck interface

React / TypeScript UI for the native Rust/Tauri application. Audio processing, decoding, controller handling, sample playback, recording, and library storage are native commands. The browser preview does not contain an audio engine or fabricated playback state.

Run `npm install` and `npm run dev` in this directory for an interface preview, or use the desktop commands in the repository README for full functionality. `npm run build` performs TypeScript validation and produces the Tauri web assets in `dist/`. Fonts are bundled locally; the production UI makes no external font requests.

## Working controls

- Load local audio through Import tracks, the library A/B buttons, or an internal track drag. Each deck's Load menu also has an explicitly generated test tone.
- Space toggles deck A, Enter toggles deck B, and Command/Ctrl+I imports tracks. Shortcuts pause while entering text or configuring settings.
- Drag a jog wheel horizontally to scratch; keyboard arrows provide a small jog adjustment. Hold CUE to preview the cue point; Set Cue stores the current position.
- Click empty hot-cue pads to set, filled pads to trigger, and Shift-click to clear. Beat Loop selects a beat length. Memory mode saves, deletes, and navigates ten memory cues. IN/OUT controls define manual loop boundaries.
- GRID edits the estimated BPM and first-beat offset. KEY LOCK requests pitch preservation, with the native preparation state displayed while processing. Pitch shift is selectable independently.
- The mixer has independent trim and channel level, three EQ bands, filter, headphone cue, crossfader, master gain, headphone gain / mix, and selectable echo or reverb depth. Its meters read actual native deck peaks.
- The sampler drawer has eight native synthesized defaults and per-slot audio loading. Click plays a one-shot; Shift-click stops it. REC uses a native save dialog and displays recording errors / dropped frames reported by the engine.
- Audio settings select the output device, buffer request, and zero-based native channel routing shown to the user as channels 1–N. Separate cue routing requires four output channels. Controller settings show real CoreMIDI endpoints, message counts, and LED feedback status.
- Library records and playlists use SQLite through Tauri. Native controller browsing follows the visible filtered/sorted library and updates selection. XML exchange uses native dialogs.

The interface polls native deck state at approximately 30 Hz with one request in flight and updates the library / devices at 0.5 Hz. Waveforms redraw canvas from native snapshots and size changes without depending on foreground animation frames, and library rows are memoized across transport updates. Recording status is read at 1 Hz while recording. The Roadmap panel identifies unsupported feature families; it does not offer fabricated switches for them.

## Spotify

The Spotify Connect tab is a separate player and library browser. Configure your own public Spotify developer Client ID, register `http://127.0.0.1/callback`, and authorize through the system browser. Private playlists and liked songs use the native OAuth integration; access tokens are never passed to React. Playlist and track pages follow the backend next-offset values. Remote full-track playback selects a real Spotify Connect device and requires Premium.

The official Spotify widget can be opened for a listed track or a pasted Spotify track URL. Spotify controls its account / playback behavior; embedded WebKit playback can be preview-only. The widget source is constrained to the Spotify embed origin and validated track IDs, and receives no native OAuth tokens. Spotify music is never represented as local audio or loaded into the Rust decks. `npm test` verifies accepted Spotify share links and rejection of untrusted widget origins.

## Analysis and waveforms

Three-band canvases use the native `waveform_bands` arrays on a shared amplitude scale: low blue, mid orange, high white. The frequency legend exposes nominal low-pass 300 Hz, mid 250 Hz–1.2 kHz, and high 1.2–9 kHz ranges. The full-mix transient envelope remains a neutral reference behind the common-center band overlays. Pixel rebinning preserves narrow peaks in a compressed overview; enlarged envelopes interpolate existing points without inventing additional audio detail. Old peak-only records display a reanalysis notice instead of fabricated frequency colors.

Loaded tracks show rhythm support, tonal strength, ambiguous / tagged / manually corrected tempos, and alternative BPM / key candidates. These detector scores are heuristics, not measured probabilities of accuracy. Selecting a BPM candidate opens the grid editor for review; half / double tempo choices also require Apply. Reanalyze calls the native in-memory worker, refreshes app state, and leaves deck transport under the native engine's control. The frontend does not decode audio or perform spectral analysis.

The performance layout has no sidebar. Header source selection chooses local or Spotify browsing; the bottom dock opens a nonmodal track drawer. Both colored waveforms are stacked above the side-by-side decks and center mixer. The drawer can stay open while playing, and closing it retains the selected Spotify playlist. Local playlist controls remain in the local browser toolbar. Spotify track rows carry metadata-only drag payloads; drop uses the same authorized native loading command as Load A/B. Unavailable/local Spotify entries cannot initiate a drag. Escape closes the browser; audio transport remains active.

The browser now uses TrackDrawer: a nonmodal bottom sheet with swipe handle, 28/44/64% height snaps, keyboard resizing, Escape/Close, focus restoration and independent scroll. Native deck targets remain interactive behind the sheet; starting a track drag shrinks it. Spotify's compact mode removes setup/player banners from normal browsing. Album links fetch album tracks through the existing library login. Analysis metadata arrives through a native cache lookup and incremental events; table BPM/key update without receiving PCM. Analyze page targets the latest 50-item page, and opt-in auto analysis runs on album/playlist selection.

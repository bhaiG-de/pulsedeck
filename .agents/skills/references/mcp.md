# PulseDeck MCP connection and capabilities

Use the connected PulseDeck MCP server for live source, deck and session work. Call its registered tools directly; the client may show a namespace prefix before the leaf names below. If the server or a required tool is absent, report the missing connection/capability and continue only independent planning or analysis of captured evidence. Do not replace missing tools with a shell client, a private socket, an improvised stdio adapter or another application's controls.

Start with `status {}` to identify the connected instance and inspect the current sources/output. Use `performance_observe` for one compact coherent score generation, exact event handles and beat/bar telemetry. Protect contributing music and user-prepared decks. A paused deck is not necessarily available for replacement.

The plugin connection instructions include resolved installed paths for `art`, `waveform_reasoning`, `library` and `control`. Read those files directly; resolve their remaining relative links from the linking file. A guessed personal skill path is not an alternative installation. If a required reference cannot be read, report the concrete missing path instead of hiding the read error.

## Discover only what the decision needs

Use the actual tool's input schema. `commands_list` and `commands_describe` describe the shared service catalog; that catalog also contains methods the MCP server does not expose. A catalog entry does not create a callable MCP tool. For example, `commands_describe` arguments `{"name":"engine.command","action":"set_eq"}` describe the native object accepted by `deck_control`. `commands_describe` with `{"name":"performance.commit"}` describes the reviewed-ticket or full-score input accepted by `performance_commit`.

`commands_describe` also accepts advertised MCP leaf names, such as `deck_control` or `performance_commit`.

The current MCP surface exposes these tools:

| Work | Tool leaf names |
|---|---|
| Artist-authored native scores | `performance_observe`, `performance_validate`, `performance_commit`, `performance_receipts`, `performance_cancel` |
| All typed renderer gestures | `deck_control` |
| Sources and preparation | `deck_load`, `deck_unload`, `deck_instant_double`, `deck_load_cancel`, `engine_wait` |
| Current state and source evidence | `status`, `engine_transport`, `engine_metadata`, `track_inspect`, `cues_get` |
| Library reads and proposals | `library_tracks`, `library_track`, `library_suggest_next`, `mix_index_scope`, `mix_openers`, `mix_options` |
| Spotify collections/readiness | `spotify_liked`, `spotify_playlists`, `spotify_playlist_tracks`, `spotify_album_tracks`, `spotify_native_status` |
| Fixed endpoint and explicitly chosen emergency successor | `mix_session_start`, `mix_session_status`, `mix_session_stop`, `mix_session_successor` |
| Lifecycle and owned automation | `jobs_get`, `jobs_list`, `events_get`, `automation_stop` |
| Sampler and smart mixer controls | `sampler_load`, `sampler_state`, `mixer_smart_modes` |
| Schema discovery | `commands_list`, `commands_describe` |

`deck_control` takes the native command object directly, such as `{"type":"set_eq","deck":"B","low":0,"mid":0.7,"high":0.8}`. It covers transport, source-relative tempo, Sync, pitch/key lock, all three EQ bands, filter, faders/gain, loops, FX, cueing, jog/scratch and sampler controls. Do not wrap it in an `engine.command` or generic `args` object. Native objects inside a score's `commands` array use that same vocabulary.

## Source guards, completion and retries

A direct deck tool accepts optional `expected_load_revision` and `request_id` as tool arguments. Bind a mutation to the observed source. Retain the returned `_meta["pulsedeck/request_id"]` or your supplied ID after an uncertain acknowledgement, and reuse the identical ID/body to resolve it. A new ID for an uncertain toggle, jump or launch can repeat the gesture.

Direct preparation controls can additionally carry `expected_state` (`playing`, `launch_pending`, `channel_fader_max`); it is rechecked in the callback. Read the actual direct receipt before composing recovery. A timeout with `receipt_ticket` can be resolved through `engine_transport {"receipt_ticket":TICKET}`; an expired bounded receipt is an unknown outcome, not a failed gesture.

A score's `generation` protects source, position, grid and control state; exact event handles retain full source precision. `operation_id` is the immutable commit identity. Preparation, acceptance, application and verified completion are different results. `performance_receipts` shows actual member states, controls and whether authored recovery ran. For source jobs use `jobs_get`; for bounded waits use `events_get` with the appropriate job/session ID and saved `since` cursor. An uncertain timeout is not cancellation.

Cancel only the owned score with `performance_cancel`; `stabilize:false` stops its pending controls without restoring values. A broader requested stop can use `automation_stop`, which revokes/yields the active endpoint and stops broader pending automation while ordinary transport continues. It does not finalize a recording. Inspect each owner and use `mix_session_stop` for its endpoint. User takeover ends authority to restore remembered controls.

## Requests beyond this connection

The current tool set does not expose recording start/stop, audio-device/routing reconfiguration, MIDI connection, account login, local imports, playlist/tag writes, arbitrary named-cue editing, bulk reanalysis or XML exchange. State the specific missing MCP capability when such work is requested. Do not infer an available callable tool from a similarly named service-catalog entry.

For an authorized recording, preserve source eligibility, output levels/routing, explicit destination and finalization; a loaded restricted source can block capture even while paused. A software meter does not prove physical sound or discrete headphones. Login/library access does not establish native deck permission. Keep these operational facts when explaining what further connection is needed; do not change the musical plan to work around absent tools.

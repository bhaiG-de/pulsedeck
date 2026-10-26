# Targeted diagnosis through MCP

A diagnostic request permits observations and an explanation; an authorized repair permits its necessary exposed controls. Use `status`, `engine_transport`, `performance_observe`, `jobs_get`/`jobs_list` and `spotify_native_status`. Compare advancing renderer frames and source positions over a useful interval. Software signal is not proof of physical sound.

| Evidence | Targeted investigation |
|---|---|
| Missing/stalled/failed source | Match source job, current source/load revision and provider error; use the [loaded-source check](deck-preparation.md#loaded-source-readiness). |
| Paused, held or ended deck | Inspect transport, cue/jog holds, current position and renderer before an intended resume. |
| Zero deck peak | Inspect the actual passage with `track_inspect`; source validity, gain/EQ/filter or a musical withdrawal can explain it. |
| Deck signal but no master signal | Inspect channel/crossfader route, master gain and attenuation; change only the intended control. |
| Master signal but silence in the room | Device/OS/amp/cabling/channel mapping needs real verification. |
| Cue selection but no headphone sound | Inspect cue mix/gain and discrete routing; selection alone does not configure output. |

Preserve deliberately isolated incoming channels and the user's master/cue level. Do not compensate for a breakdown by raising every gain. A software snapshot can retain the last successful audio configuration after a failed start; actual `engine.running` and frame advance establish rendering.

`deck_control` exposes `set_headphone_cue`, `set_cue_mix` and `set_cue_gain`. Cue feed is pre-channel-fader/pre-crossfader: mix 0 selects cued decks and mix 1 selects master. Audition needs requested preview, master isolation and an actually separate headphone output. Do not map cue onto master or raise an intentionally muted cue level as a workaround.

Audio-device/routing configuration, physical MIDI connection and account login tools are not exposed by the current MCP server. Report the missing capability for requested changes; do not open another transport or restart a live device. MIDI LED/input behavior and advertised channel counts need observed hardware evidence, not a synthetic software gesture.

Spotify collection access and native deck permission are separate. Inspect restoration, `connected`, `playback_permission` and real errors. A null account profile or false placeholder availability flag does not prove sign-out. A concrete provider cooldown means preserve the carrier and stop repeated loads; it does not establish a need for new OAuth consent. Missing manual cues are normal source knowledge, not failed analysis.

For a requested stop/handover, cancel the owned score with `performance_cancel` (`stabilize:false` when retaining user values), and use `mix_session_stop` for its endpoint. Broader `automation_stop` does not by itself pause ordinary transport or finalize capture. Re-read actual pending work and controls. Return the observed cause, targeted available change and any unresolved physical/account question.

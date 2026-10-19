# Audio, monitoring, MIDI and targeted recovery

Observe before changing controls. A diagnostic request permits reads and an explanation; an authorized configuration/repair permits its necessary targeted changes. Do not replace a live source, launch login, reset every knob or restart the audio device merely because output is quiet.

```sh
pd status --view compact
pd audio devices
pd midi ports
pd midi history
pd jobs list
```

## Establish software rendering

Record source identities/load revisions, play/position, source jobs, controls/levels, active ramps, audio device/routing and recording state. Compare snapshots across a short observation interval: advancing rendered frames and position establish software transport, not speaker sound. `engine.underruns` counts stream/device errors, not measured physical hardware underruns.

`status.result.audio` retains the last successful configuration. A failed `audio.start` removes the old stream and can leave cached `running:true`. Use `engine.running` and rendered-frame advance to verify actual rendering. Before a requested reconfiguration, retain the known-good device/buffer/routing; on failure restore it once if within the repair scope and verify the engine.

| Evidence | Targeted investigation |
|---|---|
| Missing source or failed/stalled load | Inspect source job and identity; repair the specific file/login prerequisite |
| Paused/held/ended deck or stationary position | Inspect transport, cue/jog holds and renderer before requested resume |
| Preparation pending | Inspect source job and compatibility; current continuous processing has no whole-track pitch-worker job |
| Zero deck peak | Inspect current bounded waveform, source validity and gain/EQ/filter; a quiet passage is plausible |
| Deck signal but no master signal | Check channel/crossfader route, master gain and strong attenuation |
| Master signal with valid route | Software produces output; device/OS/amp/cables/channel mapping still need actual verification |
| Cue selected but no cue signal/output | Check cue mix/gain and discrete headphone mapping |

An incoming closed fader is often deliberate isolation. Preserve that and the user's master level. Do not compensate for a musical pause by raising all gains. Use source-bounded inspection at the actual current position, and let `$djing-art` interpret the passage. Positive meters do not establish correct perceived level or absence of artifacts.

## Headphones and routing

Cue feed is pre-channel-fader/pre-crossfader. `cue_mix=0` selects cued decks; `cue_mix=1` selects master mix; `cue_gain=0` mutes phones. Selecting a cue does not configure physical output or authorize loading/starting its track. An audition needs incoming master isolation and a requested preview.

For requested B-only monitoring, set B headphones on and A off under observed source guards, then set the intended cue mix if required. Do not silently raise a deliberately muted cue level. Verify `cue_enabled`, cue mix/gain, cue meter and actual routing.

Cue left/right must be assigned together; channels are zero-based and master/cue outputs must be distinct and supported by the actual device. Two channels with null cue outputs cannot provide discrete phones on that device. Never map cue onto master to simulate headphone isolation.

`audio.start` with `device_name`, `buffer_frames` and `routing` reconfigures rendering and can interrupt performance. Use exact discovered device names and a verified physical map. Four advertised outputs do not prove master 0/1 and phones 2/3. Configure within the requested setup scope, retain existing supported buffer where appropriate, verify engine/frame advance and then physical output when available.

## MIDI and controller checks

Use exact observed port names. Inspect connected state, selected input/output, feedback state, received/dropped messages and last error. Input-only connection can work without LED feedback; report that distinction. Empty ports means no observed endpoint, not a successful connection.

After an actual physical control movement, compare message history/count and engine change. A synthetic `controller.action` does not test USB hardware. Diagnose disappearance/errors before a bounded reconnect; do not disconnect a working controller just to test it. Verify discovered FLX4 names and physical mapping rather than assuming them.

## Jobs, source failures and accounts

Inspect actual `jobs get` results and terminal state. `jobs wait` can return an `ok:true` query but exit 5 with failed state, 7 cancelled or 6 timeout. It may emit multiple JSON lines; parse each envelope. Completed earlier batch steps remain applied and later ones can be skipped. Timeout is uncertain completion, not cancellation.

Correct malformed arguments from schema/error hints; validate missing paths/source identity before another load. On revision conflict, reassess the changed target. Use identical request ID/body for an uncertain retry. Keep live playback healthy and use an authorized alternate rather than waiting through EOF for failed preparation.

Spotify library/Connect login and native deck-streaming login are separate. Inspect configured/connected/authorizing/restoring state, permissions and errors for each. A configured client ID or working library does not prove native deck access; Connect controls a provider device, not local deck transport. Account setup/login needs that request and the user's actual registered client/redirect data. Do not disconnect a working account, open browser consent speculatively or bypass availability/recording restrictions.

This experimental build can return `account:null`, `dj_streaming_available:false` and track `playable_in_deck:false` even with usable native credentials. They are not a sign-out/Premium diagnosis. With native `connected:true`, `playback_permission:true` and restoration settled, use the authorized ordinary guarded load on an available isolated deck and inspect its actual result/source job. Stop for a concrete authentication/eligibility refusal; do not bypass it or reopen consent merely because the placeholder flags remain false. Use the [loaded-source readiness check](deck-preparation.md#loaded-source-readiness): verify the intended source, current load revision, explicit readiness and guarded `deck wait`. Do not use `resolved_uri` as a readiness flag or Connect devices as a native output check.

Missing manual cues are normal and do not call for reanalysis. For unavailable timing or suggestions, inspect source identity, pulse warnings, scope/cache completeness, errors and exclusions. Let `$pulsedeck-library` handle an authorized refresh or cancellation, preserving saved knowledge.

## Takeover

Stop the owned session promptly for session handover; `automation stop` handles requested broader ramps/jobs/launches and releases holds. Verify actual pending work afterward. It does not pause ordinary transport, stop capture or roll back applied controls. Generic job cancellation may leave an active import/load/provider call; use its specific cancellation and verify the operation actually stopped.

Return observed cause, targeted change, verified resulting software state and the specific unresolved device/account/listening question. Do not infer hardware success or claim hearing from JSON.

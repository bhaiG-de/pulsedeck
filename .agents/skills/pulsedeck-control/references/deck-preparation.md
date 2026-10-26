# Prepare the chosen source and playing relationship

Obtain the entry, playing pulse/key strategy and intended later arrival from `$djing-art`. Use the [connected MCP tools](../../references/mcp.md). Preparation alone leaves the target paused; an audition additionally needs verified master isolation and monitoring.

## Establish one ready isolated source

1. Read `status` or `performance_observe`. Protect the audible carrier and any deliberately prepared destination. Preserve master level, crossfader and output routing.
2. Isolate only the available incoming channel using `deck_control` with `{"type":"set_channel_fader","deck":"B","value":0,"expected_load_revision":OBSERVED_REVISION}`.
3. Call `deck_load` with the exact observed source: a `uri` for Spotify, `path` for local audio or returned `track` identity. Bind `expected_load_revision` to the destination's old source and retain `request_id`. Ordinary replacement of a contributing deck is not preparation.
4. Obtain actual readiness and the new load revision before further controls. A normal synchronous load reaches `ready`; an uncertain response requires checking the same request/job rather than another load. `engine_wait` with `{"deck":"B","expected_load_revision":NEW_REVISION}` verifies the loaded deck's preparation. Compare the returned source and identity with the chosen version.
5. Prepare the exact entry, then its tempo, key lock and intended loop/FX/filter/EQ baseline through `deck_control`. Bind each direct action to the new revision. Sequence position-dependent setup rather than putting seek and its local-tempo calculation in one atomic gesture.
6. If using continuing Sync, establish the outgoing master and enable the distinct incoming follower last. Set explicit native Sync rate limits from the acceptable source-relative playing range. Verify both roles, limits, playing targets and actual rates. Then obtain the fresh `performance_observe` generation for the score.

Useful native objects for `deck_control` are:

```json
{"type":"seek","deck":"B","seconds":32.125,"expected_load_revision":7,"expected_state":{"playing":false,"launch_pending":false,"channel_fader_max":0.002},"request_id":"entry-B-7"}
```

```json
{"type":"set_playback_bpm","deck":"B","bpm":124,"expected_load_revision":7}
```

```json
{"type":"set_keylock","deck":"B","enabled":true,"expected_load_revision":7}
```

Replace illustrative seconds, BPM and revision with the musical choice and current evidence. Source-event handles preserve full precision; a rounded display timestamp can select the next beat. Use `set_loop` with `beats:null` only when releasing an unintended old loop; keep a loop or effect that belongs to the arrangement.

For preparation, `expected_state` expresses the state in which the gesture makes sense. The native callback rechecks playing, pending launch and both target/settled fader values before changing the deck. A failed expectation returns its cause and actual state; recompose from that state. Intentional live seeking/juggling remains available without the isolated-preparation expectations. A load revision identifies the source, not whether that source has started contributing.

## Loaded-source readiness

A source job is the latest provider request, which can differ from a previous track still on the deck. `preparing:false` and `resolved_uri` alone do not establish readiness. Match actual source, title/artist, duration and current load revision; inspect `preparation_pending`, explicit readiness and `engine_wait` result. A provider-relinked release may have a different arrangement, so refresh its passage evidence.

Spotify's `account:null`, `dj_streaming_available:false` or collection `playable_in_deck:false` can be placeholders in this experimental runtime. `spotify_native_status` with settled restoration, `connected:true` and `playback_permission:true` supports an ordinary authorized isolated load. Its actual result establishes availability. Respect a concrete refusal or cooldown; repeated account consent is not a demonstrated remedy.

## Playing tempo, Sync and bars

`set_playback_bpm` computes source-relative tempo from the local grid at application. `set_tempo` sets source-relative percent. Both take manual tempo ownership and disable follower Sync. They leave saved source BPM/grid intact. `set_bpm` and `set_beatgrid` are persistent metadata corrections, not matching controls.

`set_sync` can arm a paused distinct follower on credible loaded grids. A playing follower requires a playing master with jog released. Native Sync adapts local tempo and fractional phase while preserving position; it does not establish whole-bar or phrase identity. Check both fractional and bar telemetry when origins/meter are known. Use score `alignment` for the entry relationship and `maintain:true` across the action carrying a sustained rhythmic overlap and its clearance. The latter makes ongoing bounded Sync part of the executable score.

Bounded follower-event/deadline coverage can require explicit native `set_sync_rate_limits`. Derive suitable bounds from the selected playing relationship and source evidence; discover its exact schema with `commands_describe` arguments `{"name":"engine.command","action":"set_sync_rate_limits"}`. Do not chase every local BPM fluctuation or alter the library grid to force the pair.

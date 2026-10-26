---
name: pulsedeck-control
description: Perform artist-chosen PulseDeck mixes through connected MCP tools, including full deck controls, native musical timelines, preparation and recovery. Use with djing-art for musical judgment and pulsedeck-library for source knowledge.
---

# Perform the musical plan

Use the connected PulseDeck MCP tools directly. Read [connection and capabilities](../references/mcp.md) when establishing access or discovering an unfamiliar control. If a required tool is missing, report the missing connection/capability; do not switch to a shell client or private transport.

When asked to DJ, retain musical ownership throughout the requested duration: choose songs, paired passages, playing tempo, energy course and control gestures using [DJing — the art](../djing-art/SKILL.md). Translate that arrangement into native execution. Proposals can inform it; they do not choose the listener's experience for you.

## Derive execution from the music

Retain each source's playing BPM/rate and Sync strategy, incoming entry and later arrival, first audibility, foundation/body/foreground ownership, outgoing clearance and the combined course through the next developed phrase. Bind those milestones to observed source events and revisions. Runway determines what is reachable; it does not explain why the passage serves the listener.

| Need | Reference |
|---|---|
| Exact source events, atomic controls, curves, loops and authored recovery | [Typed performance](references/typed-performance.md) |
| Load and establish the chosen entry, tempo and Sync | [Deck preparation](references/deck-preparation.md) |
| Keep the requested finish while choosing successive moves | [Sessions and continuity](references/session.md) |
| Recompose after a failed move without losing valid preparation | [Musical recovery](references/recovery.md) |
| Inspect ownership, readiness, routing or provider trouble | [State and ownership](references/state-and-ownership.md), [targeted diagnosis](references/setup-and-recovery.md) |

## Prepare ahead; let the native clock perform

A performance request authorizes necessary preparation and playback. Planning stays read-only; preparation alone leaves the incoming deck paused. Protect contributing and user-prepared sources. Keep one native owner per affected control.

After preparing, obtain a coherent `performance_observe` packet. Build a bounded score from its unchanged generation and integer source-event anchors, typed commands and curves. Call `performance_validate` with `{"plan":SCORE}` and compare its timed transition `forecast` with the art skill's intended course. Resolve unintended support/arrival relationships by changing the musical placement or controls and validating again; interpret partial coverage from the source evidence. Then call `performance_commit` with its returned `ticket`, preserving the reviewed score and cue without re-entering them. For a sustained beatmix, prepare bounded native Sync and use the action’s `alignment.maintain` contract through the complete audible overlap and clearance; the launch gate alone does not hold phase. Register the known entrance, role transfers, clearance and following development together; native execution handles control timing while you compose the next move. Use the full available palette when it serves the passage rather than reducing every arrangement to a bass/fader swap.

Keep the original endpoint of a bounded set using `mix_session_start`; it owns time without choosing songs. Read `mix_session_status` and follow actual feedback through `events_get` and `performance_receipts`. A passive log alone does not wake an idle performer. Prepare the next source while worthwhile musical exits remain.

## Recover from what actually played

`performance_receipts` distinguishes pending, applied, verified, failed and cancelled members, current controls and actual recovery. A rejected score leaves successful preparation intact. Retry an uncertain commit with the same ticket or identical operation ID/body; an altered arrangement is a new operation.

A running failure uses the authored recovery only while its declared carrier remains available. Reobserve after recovery, or cancel the owned pending score before a reachable replacement. Preserve useful loaded/cued sources and deliberate timbre. If both decks end unintentionally, report the gap and establish the intended ready source through an available route, then rebuild the phrase plan. An ended clock cannot launch future work. Honor a genuine user stop/takeover and retain the original finish time.

Executed receipts establish controls; source trajectories and listening establish their musical consequences. Recording and durable library/account changes require their corresponding intent and exposed capabilities.

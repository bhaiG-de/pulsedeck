---
name: pulsedeck-control
description: Operate PulseDeck through pulsedeckctl for deck preparation, direct performance controls, supported native transitions, optional autonomous queues, recovery, audio setup and recording. Pair with djing-art for musical judgment and pulsedeck-library for source knowledge.
---

# Playing PulseDeck

Use the installed `/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl` for the configured Applications instance, or the checkout client for a development instance. `pd` in examples means the intended client. Read [CLI semantics](../references/cli.md) for unfamiliar transport or ownership details; discover an unfamiliar method with `commands describe METHOD`.

The agent is free to choose songs, passages, tempo strategy, loops, EQ, filters, FX and transition style within the actual tool capabilities and user's brief. A supported recipe is convenient, not compulsory. There is no requirement to delegate the set to AutoDJ, quit after the first arm, keep a universal BPM/key limit, or use a fixed number of bars. Artistic suggestions come from [DJing — the art](../djing-art/SKILL.md).

## Choose the appropriate control path

| Approach | Reference |
|---|---|
| Select a proposed move and let the native executor perform it | [Assisted next moves](references/assisted-next-moves.md) |
| Choose and perform individual phrase/loop/EQ/FX gestures | [Transition execution](references/transitions.md), [persistent session contract](../references/timed-execution.md) |
| Prepare the next deck, tempo and Sync | [Deck preparation](references/deck-preparation.md) |
| Let the application choose subsequent songs and moves | [Optional continuous queue](references/continuous-sets.md) |
| Keep a custom performance running to a requested endpoint | [Sessions and continuity](references/session.md) |
| Inspect routing, audio, MIDI, headphones or login | [Setup and recovery](references/setup-and-recovery.md) |
| Make an authorized recording | [Recording](references/recording.md) |

Use individual moves or direct controls when you want to decide the actual phrase relationship. The autonomous queue chooses its own windows from numeric evidence; it does not consume the art manual or a spoken musical plan. It is useful when that level of automation fits the request. Avoid competing owners for the same deck/controls; stop or yield the owned automation before replacing it with another workflow.

## Prepare ahead and follow the music

A performance request authorizes the necessary preparation and playback. Planning alone stays read-only, and preparation alone leaves the incoming deck paused. Protect an audible or user-prepared source from accidental replacement. Source/load revisions bind actions to the intended recording; a preparation or accepted queue is not yet an executed handover.

Use native beat/event scheduling for known musical actions so model latency does not determine their execution. Follow lifecycle feedback for meaningful musical decisions and failures; inspect current state when it resolves an ambiguity. Observing a live set, choosing successive moves and following an event stream are allowed. There is no policy requiring an agent to disappear while the set is running.

For a queue, `--wait-ready` is an optional bounded startup check. An agent can hand off after the first successor is armed when unattended operation was intended, or remain involved when performing/observing was requested. Retain the queue/operation identity and original finish time. A stopped or failed queue may leave ordinary music playing; prepare the next move before that carrier runs out. The current host may need to deliver an actionable event to resume an idle agent; a passive log alone does not do so.

## Recover from what actually happened

Inspect failed receipts and actual deck positions before retrying. Preserve healthy music and do not replay a partly completed sequence blindly. A source rejection is not a musical instruction: another song, another passage, another technique or another supported workflow may be appropriate.

If both decks end unintentionally, report the gap, establish a ready incoming entry with audible controls and plain-play it before rebuilding a phrase plan. An ended outgoing clock cannot execute a future launch. Preserve the original endpoint rather than starting another full hour. Yield to a genuine user stop/takeover. Recording and durable library/account edits still require the corresponding user intent. Execution receipts do not certify listening quality.

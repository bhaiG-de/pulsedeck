---
name: pulsedeck-control
description: Operate PulseDeck through pulsedeckctl for deck preparation, tempo/Sync, timed transitions, continuous sessions, audio/headphone/MIDI setup, recovery, WAV recording and takeover. Pair with djing-art for musical choices and pulsedeck-library for saved knowledge.
---

# Controlling PulseDeck and workflows

Turn a musical plan into supported, verified controls while preserving healthy playback. PulseDeck owns the audio clock. Use the running app and public CLI contract for real performance.

**Continuous music is a hard requirement when the user asks for a mix or set. The incoming track must be audibly established before the outgoing track finishes. Never allow silence because preparation, planning or scheduling ran late.** A loaded track, a playing deck behind a closed fader, or an armed item on an ended clock does not satisfy this requirement. Use the [assisted move workflow](references/assisted-next-moves.md) for continuous sets; consult [custom-session continuity](references/session.md#continuity-is-a-hard-requirement) when building unsupported transitions yourself. Honor intentional musical breaks, the requested endpoint and explicit user stops.

## Continuous sets: start, then hand off

Read [continuous sets](references/continuous-sets.md) for a mix/set request. It is the complete normal workflow: check the intended running instance and native playback, choose one opener from `mix.openers`, load/play it, then submit **one `pd mix queue start` without `--follow`**. The accepted receipt owns subsequent selection, preparation, transitions, recovery and duration. **End the agent turn after acceptance.** Healthy continuous queues require no model monitoring, timed status reads, follower process, sleep loop or reads of an idle stream. PulseDeck emits `mix.queue.notice` for actionable outcomes, retains queue status and shows an outcome notice in the app. A host may subscribe outside inference; do not emulate a host subscription by keeping a model alive.

Use the installed `/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl` (or `~/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl`), or the repository's `target/debug/pulsedeckctl` for a development instance (`pd` in examples). The continuous-set reference supplies exact commands; read [CLI semantics](../references/cli.md) only for unfamiliar transport/ownership details. Discover one unfamiliar method with `commands describe METHOD`; `commands list` is concise and supports `--prefix`/`--search`. Full schemas need `--detail`.

For **one individually chosen move**, read [assisted next moves](references/assisted-next-moves.md): request `mix.options` once and submit `mix execute`. Only that single-move workflow may need a terminal-event wait before the next musical decision. For custom transitions use [continuous sessions](references/session.md). Read [state and ownership](references/state-and-ownership.md) for unfamiliar live material.

| Work | Reference |
|---|---|
| Choose and execute a supported next move | [Assisted next moves](references/assisted-next-moves.md): shared Suggest options, one-call execution, lifecycle events and optional audition |
| Load, isolate, set an entry, prepare tempo/Sync | [Deck preparation](references/deck-preparation.md) |
| Perform the chosen blend or exchange | [Transition execution](references/transitions.md), [persistent command contract](../references/timed-execution.md) |
| Keep a set running through repeated handovers | [Continuous sets](references/continuous-sets.md); hand off after acceptance |
| Diagnose audio, routing, headphones, MIDI or login | [Setup and recovery](references/setup-and-recovery.md) |
| Capture and finalize an authorized WAV | [Recording](references/recording.md) |

Use [DJing — the art](../djing-art/SKILL.md) for the sound, entries, arrivals, layer transfers and exits. A smooth-set plan includes a positive audible overlap, prepared beatmatching, staged role transfer and outgoing clearance. A grouped Play B / fader B up / fader A down action is a cut; successful execution cannot establish that a blend happened. Use [Manage Library](../pulsedeck-library/SKILL.md) for collection reads and saved metadata.

“Plan” stays read-only. “Prepare B” leaves it paused unless audition is requested. “Mix now” or “play a set” authorizes necessary preparation and performance. Honor existing authorization. Recording, account setup and durable library edits need that intent.

## Keep execution ahead of the music

Protect contributing and user-prepared decks, global output and routing. Retain source/load, grid and position identities; source guards alone do not protect manual mixer changes. Load only into an available isolated deck without replacing playing material. Verify the intended source and current revision using the [loaded-source readiness check](references/deck-preparation.md#loaded-source-readiness); paused meters and provider placeholders cannot diagnose a failed load.

Use the shared [assisted next-move service](references/assisted-next-moves.md) for supported recipes. Submit the complete move once; PulseDeck performs preparation, selects a validated short fallback if needed, arms native timing and verifies clearance. For a single move, consume only feedback needed for its next decision. A continuous queue needs no model follower. Do not start a low-level persistent queue alongside it, reconstruct returned beat coordinates or read status after each lifecycle event.

On a fresh start, prepare a suitable opener with enough music for the first load and transition. Start it, then start the requested successor queue immediately, with ordered sources or a fully indexed collection, energy/tempo/technique limits and the requested duration. The app prepares each successor when a deck is released. Hand off after acceptance. Resume only for an actionable outcome delivered by the host or a user request. For an individually chosen move, execute it immediately and honor its continuation/decision budget. The app preserves the healthy carrier when unavailable audio prevents progress.

For genuinely custom transitions outside the supported recipes, use one persistent `pulsedeckctl session start --stdio` process and read the [transition](references/transitions.md) and [persistent command](../references/timed-execution.md) contracts. Those references describe the low-level path, not the default agent workflow.

`registered` and `queued` are not completion. Verify acknowledged action/version `executed`, advancing incoming audio, intended routing/roles and final mixer state before pausing the outgoing deck. A `running` job can contain a failed launch: respond to the failed action immediately. Receipts and meters establish execution, not musical quality or speaker audibility.

## Recover or yield

On uncertainty, inspect receipts/state before retrying the same request. On failure, preserve healthy playback, cancel conflicting owned pending work and revise from actual positions and inherited controls. Applied steps remain applied. Do not replay an entire partly completed transition or open a failed incoming source. Use [session recovery](references/session.md#recover-before-the-healthy-track-ends).

If both decks ended unintentionally, an ended clock cannot launch the successor. Verify a ready coherent entry and audible controls, plain-play it, then resume preparation; report the gap. On takeover, stop conflicting owned automation and preserve the user’s values. Session `op:stop` and broader `automation stop` cancel pending work; ordinary playback and recording have separate stop/finalization actions. Do not restart after an explicit stop.

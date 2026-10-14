# Timing and recovery for a prepared handover

Read when automating a transition. This is an execution contract, not a generic mixing recipe or a supplied live executor. The musical plan comes from [DJ musicianship](dj-musicianship.md); command guards and completion semantics come from [CLI](cli.md).

## Prepare before the clock becomes critical

Keep selection, analysis, controller construction and model reasoning ahead of the handover. Complete the bounded controller and its checks before it queues the native launch. Choose from already inspected, musically eligible future events using a fresh status snapshot and queue in the same invocation. Recheck after preparation if it consumed the lead budget. When every candidate has expired, take the prepared fallback while outgoing audio still remains; do not spend that margin inventing another plan.

Retain the requested first buildup/payoff when selecting entries. Skipping its opening to obtain an easier grid is a changed musical plan. Prefer another eligible song or a later outgoing boundary when needed. Short tracks require earlier preparation and less optional analysis, not a rushed last-bar selection.

For an authorized subagent, carry the user's model choice and session constraints into its prompt. A smaller model can reduce decision latency. Exact starts still belong to native scheduling, and mixer phases still need bounded execution and monitoring. Use one owner for live controls; shadow agents inspect only.

## Space sequential phases before arming

Record source timestamps and actual beat events for launch, isolated verification, each ramp boundary, each arming deadline, completion and recovery. Quantization may defer a ramp to the next grid boundary. Count that wait and the longest concurrent ramp in the phase duration.

For sequential phases, require:

`latest phase completion + observation/dispatch margin < next arming deadline`

Use a margin based on observed tool latency plus uncertainty. Converting wall-time latency into outgoing source seconds requires its playback rate. A phase ending exactly on the next intended exchange does not leave time to inspect and arm that exchange. Redesign the earlier start/duration or use an independently verified controller that supports the required concurrency. Do not remove endpoint checks merely to make a schedule fit.

Validate coordinated batch steps with source guards. Wait for the dispatch job's terminal state and inspect every step, then wait for active ramps and verify endpoints. Waiting for ramps while a batch still has undispatched steps can miss later ramps or delay dispatch. Parse every JSONL response envelope; a parser error after an accepted mutation is not evidence that nothing ran.

## Recover an audible partial blend

A deadline guard prevents stale actions; it does not repair controls already applied. Before launch, declare a simple fallback for an isolated failed start and another for a partially audible overlap, with an outgoing-time recovery deadline.

- **Incoming still isolated:** retain the healthy outgoing deck, cancel only this operation's pending work, and use an already prepared eligible fallback. Keep enough outgoing audio to execute it.
- **Incoming audible and ready:** select the healthy deck to carry the mix from current source, position, readiness and musical evidence. Settle its intended fader/EQ/filter and clear the other contribution with a brief appropriate ramp. A playing incoming track with its low or mid band still cut must not be abandoned when the old track ends.
- **Source/position/control change or manual takeover:** stop this operation's future mutations and re-inspect. Do not run a cached restoration against the user's new state. Only cancel work this operation owns, using supported cancellation surfaces.
- **Uncertain command result:** inspect state/job before another mutation; do not replay the whole batch. Bound recovery and report any gap or failed handover.

An automated controller must perform its prepared stabilization promptly on a timing or own-phase failure, within the same running controller. Logging an assertion and waiting for another model turn leaves a partial blend unattended. If recovery cannot be implemented and verified with the available controls, simplify the handover before launch.

## Verify and report

Check advancing incoming playback, expected source/revisions, final tone/fader, outgoing isolation and remaining jobs/ramps. Record actual openings, exchanges, clearance, gaps and protected payoff coverage. Differentiate native scheduling precision, successful control endpoints and listening quality. Neither a key label nor clean telemetry proves a seamless, full or unmuddy mix.

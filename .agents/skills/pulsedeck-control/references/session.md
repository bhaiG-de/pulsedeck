# Continuous DJ sessions

**Default:** for supported multi-track sets use the [app-owned successor queue](assisted-next-moves.md#app-owned-successor-queue). Choose explicit repertoire and musical policy, start it once and follow its events; the app selects/prepares successors and owns native timing, validation, bounded skips and fallback. Single selected moves use `mix.execute`. The persistent-queue procedures below apply to custom low-level sets; do not combine them with an owned successor queue or `mix.execute` move.

Apply `$djing-art` for selection, phrasing and set flow. Use `$pulsedeck-library` for requested collection reads; do not change saved cues/playlists or start recording unless that intent is present. Read [deck preparation](deck-preparation.md), [transition execution](transitions.md) and the [persistent contract](../../references/timed-execution.md) as needed rather than every reference before a straightforward set.

## Continuity is a hard requirement

A request to mix or play a set requires continuous audible music until its intended endpoint or a user stop. The next track must be advancing through its intended master route before the outgoing track exhausts its useful audio. A prepared deck, a successful Play behind a zero fader, or a queued event whose clock has ended is not continuity. Preventing an avoidable gap takes priority over optional discovery, a preferred long blend, perfect phrasing and progress reports. Preserve intentional breaks in the music; do not manufacture silence between tracks.

For every active pair, set a hard fallback deadline before the last usable outgoing passage, allowing the measured preparation/check/registration cycle plus one recovery attempt. Use at least 60 seconds of margin when turnaround is unknown, and increase it when actual cycles take longer. If the complete handover is not acknowledged and viable by that deadline, stop normal planning and execute the prepared supported fallback immediately while useful music remains. Do not spend the remaining passage chasing another nearby boundary with the same slow workflow. Verify the incoming deck's advancing source, audible fader/EQ state and master route before clearing the outgoing deck.

Make audible entrance, role transfers and outgoing mute/pause depend on the incoming launch's successful completion as well as their musical timing. A failed or missed launch must leave the healthy outgoing track audible; independent timed fader actions must not clear it while the destination is paused. Consume the launch failure immediately and cancel or revise its owned transfer actions before they can run.

Treat any source load/change or yielded native queue as invalidating the previous armed-state assumption. Immediately consume the affected receipts and re-establish the current handover and requested endpoint against the actual sources/revisions before waiting or doing further selection. A session that still says `running` is insufficient evidence that its launch or finish timer survived.

## Start and keep a rolling horizon

Inspect one compact snapshot of live ownership, sources/revisions, remaining audio, faders, master/Sync, loops, output routing, source jobs and pending work. Preserve someone else's playing source and prepared deck. If starting from silence, choose the initial song and start once ready with the requested output state; a set request does not justify speculative device reconfiguration.

Start one `pulsedeckctl session start --stdio` process and keep its stdin open. Retain its writable process handle, session/job IDs and output cursor. A PTY runner needs echo and canonical buffering disabled as documented in the contract; canonical truncation can corrupt long JSON appends. A second simultaneous native session is not another mixer lane and may fail for capacity.

Choose an initial pair and a useful alternate from the requested collection. Respect exact order when specified. Missing scoped cache or readiness requires a targeted recovery, not silently switching to the full library. Availability flags cannot replace the real load result.

Keep the plan small: current source, next source, intended handover, alternate and preparation deadline. On a fresh start from silence, load and prepare the first pair while paused. Select their complete passages, seek the incoming entry, set its tempo/key lock/Sync and isolate it. Validate the art coordinates with `phrase_check.py --preflight`, then save the complete first session append and run `pd batch validate --file "$task_first_plan"`. Correct schema/group/dependency errors while no song clock is advancing. Submit opener Play and the complete handover together, with the incoming timed launch depending on opener Play. Do not start the opener with only a promise to construct the first blend later.

While a set is already playing, prepare the released deck immediately after clearance and inspect only the active pair before arming its complete handover. Empty decks and unarmed transitions are unfinished live work. Apply the hard continuity deadline above; preparation margin is separate from overlap and dwell. Choose musical statements and useful overlap rather than treating EOF as the normal mix point.

Choose a consistent tempo/Sync strategy. Absolute playback BPM is entry intent, not a permanent local tempo lock. Use valid master/follower roles for ongoing local adaptation, verifying roles after master source changes. A readiness dependency alone does not align a launch: retain the musical clock condition when revising preparation.

For an explicitly requested fast capability trial, choose short, credible entries and overlaps as preparation and musical material allow. Do not play full songs merely to exercise controls, or impose trial pacing on an ordinary musical set. Track dwell time, audible overlap and final fader gesture are different durations.

## Register, consume, revise

Register known linked phases before waiting. Use atomic appends where partial registration would leave a gap, and native groups for controls that should move together. The runtime executes between model turns. Do not use shell sleeps, repeated transport polls or a separate DJ script to time every fader movement.

Consume actual completion/failure events and prepare the released deck as soon as isolation completes. An idle stream may simply be waiting on accepted future timing. Queue status contains unfinished work and eight recent terminal receipts; request older full job detail only for recovery.

Track `prepared`, `armed` (acknowledged launch and complete handover IDs), `launched`, `audible` and `cleared` separately. A ready isolated successor with no pending launch is not an armed transition. Record the outgoing remaining audio and next deadline alongside those states; base progress reports on receipts rather than preparation alone. Keep the OS PID distinct from the tool's writable process handle so another supervisor can stop the actual owner.

Give every new item a session-unique ID. Link only acknowledged IDs/versions. Updates replace pending work and retarget pending dependents; follow the replacement version, not an old cancellation receipt. Executed work cannot be rewritten. A rejected/reused name does not become a new successful action; explicit `after_version` is only for an intended known historical completion.

Use `op:update`/`op:remove` while work is pending, respecting dependency conflicts. Do not keep issuing fresh-ID copies after an uncertain reply. Identical request IDs/bodies retrieve receipts within the runtime window; changed-body retries conflict. A failed session job cannot accept fresh work: stop/close it, inspect actual inherited state and replace only after the old owner settles.

## Recover before the healthy track ends

Keep failed incoming material isolated. Preserve already-applied fader, bass and filter values in the recovery assessment; cancellation does not reset them. A paused deck can have an open channel, so re-establish isolation before any new preview/start. Do not open a channel whose prerequisite failed.

Inspect an ambiguous failure once, then use a supported alternate or simpler feasible handover while A has enough audio. A future event on an ended/paused clock cannot fire. Revise a missed boundary from actual remaining opportunities. Avoid expensive optional analysis near the deadline.

Keep the fallback executable from known controls while preparing the normal mix. On a failed launch, check the matching action and dependents even if the overall job still says `running`. Get one fresh compact snapshot: which source is actually advancing, how much musical support remains, and whether the prepared destination is still ready with inherited fader/bass cuts. Stop conflicting **owned** pending work through the existing session before a replacement owner; verify cancellation rather than launching competing sessions or trying unrelated batch sealing.

If enough useful passage remains, revise to a shorter supported exchange on an actual future event. If the window is nearly gone, favor a prepared coherent entry and simple handover over further schema exploration. If silence has already occurred, restoring audible music is the immediate next task: verify the prepared source/revision, remove stale owned automation, restore the selected entry's required EQ/fader/master state, and plain-play it without an ended-clock condition. Do not wait for another phrase, continue browsing, or merely start transport behind a closed fader. Verify advancing audio through the master route before reporting or resuming preparation. Loading does not reset a zero fader or low EQ. Report the gap as a failed continuity requirement; do not claim an uninterrupted mix.

Yield to genuine manual control changes affecting owned work. Do not restore remembered knobs over a user's adjustment. Headphone listening or master-level changes alone do not require disrupting unrelated queued preparation. During a performance, use skill references and public CLI output rather than inspecting source/tests or modifying skills/code to bypass a gap. Keep a concise failure note for later maintenance.

## Resume, finish and stop

After compaction, consume the existing process output and use `op:status` once if required. Keep process handle, session/job IDs, source ownership, logical item IDs/versions and musical intent in compact notes. Lost context is not permission to replay a launch or start another session.

If the process is lost, inspect the known job and current state before cancellation/replacement. App restart interrupts unfinished work without playback replay. Do not claim completion from connection loss.

Continue the event-driven set until the requested endpoint. “Stop automation” or a control handover means promptly stop the owned queue and leave healthy ordinary playback unless the request also stops music. “Stop the music/trial” also requires pausing owned decks and cancelling owned in-flight preparation if active. Neither implicitly finalizes the user's recording; stop/finalize capture when requested or when an already-authorized bounded capture reaches its endpoint. Never restart after explicit stop without renewed intent.

Report current/next song, verified handovers, actual elapsed time where measured and concrete failures briefly. Keep executed timing distinct from musical feel, audible quality and unverified hardware behavior.

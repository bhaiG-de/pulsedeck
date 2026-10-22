# Persistent command session

Keep one `pulsedeckctl session start --stdio` process running. Send JSON lines to its stdin and consume the JSON lines it continuously writes to stdout. Each queue item is an existing software command or a simultaneous native command group; the CLI and UI share the same backend. The agent chooses the music and phrasing, while the runtime executes accepted commands and timing conditions. Planning-only requests remain read-only.

Use the tool runtime's persistent process handle to send more input and receive output. Do not pipe a complete set into a short-lived process: stdin EOF closes this session and cancels its pending work. A live process is essential, not a shell polling loop or a separate DJ script. End a PTY session with the JSON `stop` operation; do not rely on a terminal EOF keystroke.

```sh
pulsedeckctl session start --stdio
```

If the execution tool needs a PTY to keep stdin writable, disable echo and canonical line buffering, retain its process handle, and send later JSON through that same handle. Canonical terminal input can silently truncate longer JSON groups before the CLI sees them. Noncanonical input still uses newline-delimited JSON in the CLI. A pipe-based runner must keep its stdin pipe open. For a PTY shell:

```sh
stty -echo -icanon min 1 time 0
exec pulsedeckctl session start --stdio
```

The first output has `type:"session_ready"`, `session_id`, `job_id`, `revision`, `cursor` and supported `operations`. Keep the process handle and these IDs in compact notes. There is no mandatory handover object, completion carrier, recovery program, expiry or seal for this command session.

## Add commands immediately

Objects with `command` or `commands` enqueue by default; `op:"add"` is optional. Give every new item a session-unique `id`, especially when another item will depend on it. Use `op:"update"` to revise an accepted pending item; do not reuse an old/rejected ID as a new action and attach dependents to its stale receipt. An omitted ID is generated and returned. These are stdin JSON lines, not separate shell invocations:

```jsonl
{"id":"load-b","command":"deck.load","args":{"deck":"B","uri":"spotify:track:TRACK_ID"},"request_id":"load-b-once"}
{"id":"start-b","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"load-b"},"request_id":"start-b-once"}
```

Replace `TRACK_ID` with a selected real source. This illustrates readiness chaining, not a complete musical mix: isolate the incoming channel and prepare its tempo/entry before scheduling an audible start. The queue does not select songs, invent phrase boundaries or automatically move faders.

Queue items are independent by default. Use `when.after` whenever one command requires another to finish. For example, queue load → tempo/entry preparation → play → fader ramp, using each relevant completion ID. A load's completion means its existing preparation path is ready; a ramp's completion means the renderer reached its endpoint. Do not put a fader opening merely after registration of an asynchronous load or ramp.

## Register a linked plan atomically

Use `{"op":"add","items":[...]}` to register 1–256 related items in one existing queue append. Give every item its own unique `id`; put a shared retry `request_id` on the outer object. Items can refer to other IDs in the same append. Each item retains its own command/group and timing; atomic registration does not mean simultaneous execution. One invalid item rejects the whole append, leaving existing accepted work unchanged.

For example, with A playing and B available/paused, register isolation → load → chosen entry → tempo/bass preparation → an absolute clock launch → parallel blend together. Replace the illustrative local ID, entry, target BPM and A source timestamp with current selections. The absolute launch point must be a real inspected future beat with enough preparation/headroom; this is a bounded timing example, not a song-selection recipe:

```jsonl
{"op":"add","request_id":"handover-1","items":[{"id":"isolate-b-1","command":"engine.command","args":{"type":"set_channel_fader","deck":"B","value":0}},{"id":"load-b-1","command":"deck.load","args":{"deck":"B","track":"local:42"},"when":{"after":"isolate-b-1"}},{"id":"entry-b-1","command":"engine.command","args":{"type":"seek","deck":"B","seconds":8},"when":{"after":"load-b-1"}},{"id":"prep-b-1","commands":[{"command":"engine.command","args":{"type":"set_playback_bpm","deck":"B","bpm":124}},{"command":"engine.command","args":{"type":"set_eq","deck":"B","low":0,"mid":1,"high":1}}],"when":{"after":"entry-b-1"}},{"id":"play-b-1","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"prep-b-1","type":"at_seconds","clock":"A","seconds":160}},{"id":"blend-1","commands":[{"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"B"},"value":1,"seconds":4}},{"command":"automation.ramp","args":{"target":{"type":"eq_low","deck":"B"},"value":1,"seconds":2}},{"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"A"},"value":0,"seconds":4}},{"command":"automation.ramp","args":{"target":{"type":"eq_low","deck":"A"},"value":0,"seconds":2}}],"when":{"after":"play-b-1"}}]}
```

This explicit preparation keeps inherited fader/bass settings from becoming an accidental audible start; loading itself does not reset them. Add valid-role Sync preparation when the chosen overlap requires local tempo following. An intentional cut can use unequal tempos; launch never adds Sync or tempo matching automatically.

The single acknowledgment has `atomic:true`, `item_count`, `registered_count` and `items`. On success, every item entry has `type:"registered",ok:true,item_id,action_id,version`; on failure the outer response is `type:"rejected",ok:false,registered_count:0` and no inner item is registered. There are no accepted prefixes. Actual per-item execution still arrives through the normal stream. An identical outer request/body retries its receipt without replay; aliases/versions change only after the full patch is accepted.

For a fresh set, save the complete first `op:add,items:[...]` object as a JSON file and run `pd batch validate --file "$task_first_plan"` while both sources are paused. The CLI accepts the same session object and validates its commands, groups and dependencies without executing it or reserving IDs. Send that unchanged object to the persistent session after it passes. Include opener Play as the first item and make the incoming timed launch depend on it. This registers the entire known first handover before the opener clock advances. Coordinate validation belongs to the art checker; batch validation does not certify phrases or future live timing.

## Move several controls together

Use one `commands` array when controls must start together. The group has one ID and one shared `when`. All members begin on the same audio frame, and ramps keep their own durations. For example, opening volume while restoring highs:

```jsonl
{"id":"open-b","commands":[{"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"B"},"value":1,"clock":"A","beats":8}},{"command":"automation.ramp","args":{"target":{"type":"eq_high","deck":"B"},"value":1,"clock":"A","beats":4}},{"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"A"},"value":0,"clock":"A","beats":8}}],"when":{"after":"start-b"},"request_id":"open-b-once"}
{"id":"pause-a","command":"engine.command","args":{"type":"pause","deck":"A"},"when":{"after":"open-b"}}
```

This assumes B is prepared, playing and healthy before the group. `open-b` finishes opening B and isolating A only after all three ramp endpoints, so `pause-a` depends on the longer member. Do not chain the two ramps if they should overlap. A group supports 2–64 `engine.command` or `automation.ramp` members with disjoint target controls. Each member uses the canonical request shape, including its own `args`; nested groups, per-member `when`, per-ramp `quantize` and provider loads are unsupported. Put launch quantization on the shared outer `when`; a member's own `clock`/`beats` can still measure its ramp duration. Put loading/analysis and other software operations in separate readiness-dependent items. Do not combine outer `command`/`args` with `commands`. Invalid members or overlapping targets reject the whole group before any member is registered.

Bass and playback tempo can also move together: use `automation.ramp` targets `eq_low` and `tempo_percent` on the selected deck. `tempo_percent`'s value is absolute percent relative to the analyzed source BPM, just like `deck tempo set`; it does not edit analysis or base BPM. Starting that ramp takes manual tempo ownership, disables follower Sync and clears its correction; do not expect Sync to keep following during the ramp. Choose the target from the musical plan rather than repeatedly chasing displayed BPM. Discover command-specific fields with public `commands describe` when needed.

Update/remove the whole pending group using its existing ID. Updates can switch between a single command and a group. The acknowledgment echoes `commands` and `member_count`; a running group's `members` receipts expose individual progress, while group `executed` requires every member to finish. Dependents use the group's latest actual renderer completion, including when members have different durations. Member failure/cancellation prevents successful group completion. Execution already underway cannot be undone by replacing the pending plan.

For an absolute entry tempo after load, queue the existing setter; replace the illustrative target with the selected musical tempo:

```jsonl
{"id":"tempo-b-1","command":"engine.command","args":{"type":"set_playback_bpm","deck":"B","bpm":124},"when":{"after":"load-b"}}
```

Verify the accepted `target_playback_bpm` and current `actual_playback_bpm`, not source `bpm`. The renderer uses its current four-beat local fit at execution, rejects missing/unreachable rates and takes manual tempo ownership (Sync off, adaptive offsets cleared). Source analysis/grid stay unchanged. This sets entry tempo once; it does not maintain a BPM lock as real music varies. Use valid master/follower Sync for ongoing local adaptation during overlaps, and judge receipt success by its accepted intent/ownership rather than repeatedly comparing a moving local measurement. Gradual parallel tempo ramps still use `tempo_percent`; their endpoints are source-relative percent, not BPM.

Timing is resolved by the runtime, not by model wall-clock estimates:

| `when` | Meaning |
| --- | --- |
| omitted or `{"type":"now"}` | Execute as soon as eligible. |
| `{"after":"load-b"}` | Execute after that item completes successfully. |
| `{"type":"after_beats","clock":"A","beats":16,"quantize":4}` | Resolve a future beat target on A; optional quantize aligns the target lattice. |
| `{"after":"start-b","type":"after_beats","clock":"B","beats":8}` | Resolve the beat delay after B's start actually completes. |
| `{"type":"after_seconds","seconds":2}` | Execute after a renderer-clock delay. |
| `{"type":"at_seconds","clock":"A","seconds":160}` | Execute at the selected timestamp in A's source audio, preserving that target as preparation completes. |
| `{"type":"at_event","clock":"A","event":64}` | Use an already selected native event index. |

Clocks are `A` or `B`. A beat count is not a musical phrase until meter and origin are credible; `quantize:4` alone does not establish bar or arrangement alignment. If inspection identifies a musical boundary at a particular source timestamp/event, queue `at_seconds` or `at_event` directly with ample lead. A readiness `after` may accompany it; the absolute target stays fixed and a missed target is a failure to revise, not a reason to start late. Use `after_beats` for a delay genuinely measured from registration or a named completion. Do not convert an inspected absolute boundary into a relative beat delay from an earlier snapshot: model/tool latency and intervening dependencies move that delay later. Use the inspected `beat_index` for `at_event`, not `beat_position`: on an anchored grid they differ by `beatgrid_anchor_index`. Do not reconstruct an event index by multiplying seconds by whole-track BPM. Bind the current source/grid revision and inspect the resolved source timestamp before the launch; if it differs from the intended musical boundary, revise while the action is still pending. Use actual inspected events for `at_event`. A queued launch aligns the selected clock beat and incoming entry without a BPM-equality gate, automatic Sync or rate change. Choose close tempos/Sync for a sustained beatmix or deliberately unequal tempos for a cut; retain the real source/grid, readiness, future timing and ownership guards. A paused/ended clock cannot carry a future beat delay; do not wait until the current song is almost over to start preparing its successor.

Submit known commands back to back, including their future timing and dependencies. They can execute while the agent plans or is compacted. Do not repeatedly read transport to decide when to fire Play or a fader command. Do not add an engine wait after every step; the queue's completion dependency already provides it.

## Read feedback and revise pending work

A registration acknowledgment echoes `command`/`args` or grouped `commands`/`member_count`, optional `when`, and the returned `item_id`, native `action_id` and `version`. `ok:true` with `type:"registered"` means accepted, not executed. Continuous events have `type:"job.action"`, a `sequence`, and `data` containing `item_id`, `action_id`, `version`, state and any result/error. Events and acknowledgments may interleave. A rejected add/edit/removal uses `type:"rejected"`, `ok:false` and its original operation; it is never a successful registration. Internal progress/CAS bookkeeping is omitted from this stream.

- `registered`: accepted into the queue.
- `queued`: armed in the renderer, waiting for its timing/dependency; still eligible for pending edits.
- `running`: actually started, still in progress.
- `executed`: the command's actual completion was verified.
- `failed` or `cancelled`: it did not complete successfully; inspect the error and affected dependencies.

Queued/running native receipts expose resolved timing when available: `resolved_clock` (0/1), `resolved_clock_seconds` (source-media position), `resolved_frame`, and `clock_source_headroom_seconds` (remaining source-media time at that trigger). `ramp_end_frame` is the actual fixed renderer endpoint once the ramp has started; it is not a preflight musical estimate. Dependency-based targets appear when they resolve. Typed `failure` contains `cause`, affected `deck` and failing native `action_id`. These are native facts, not wall-time or audibility promises. If a receipt's resolved source point differs from the claimed musical point, inspect once and revise pending work using the actual absolute source boundary; do not keep adding relative delays to catch up.

Treat a failed dependency as a reason to revise dependent work, not to open the incoming channel blindly. Read one concise state snapshot when a failure is ambiguous. Prefer a simpler supported next action over retrying an already missed plan.

Update repeats the replacement command/arguments or `commands` group; it atomically replaces a pending item. The public ID remains stable and its version increases. Pending items that depend on it follow the replacement atomically; they wait for its new completion rather than the cancelled old version. A replacement that creates a cycle or would alter execution already underway is rejected atomically, leaving the existing plan intact. The backend accepts replacements only while the actual action is still pending; executed items cannot be rewritten. Removal cancels a pending item; it does not undo music already played. Removing an item still referenced by pending dependents rejects with `dependency_conflict` and leaves the plan unchanged. Remove or revise those dependents first. If the whole dependent branch should be cancelled together, the existing canonical `batch.invalidate` can atomically invalidate all affected internal action IDs; ordinary updates need no lower-level patch.

A changed-body add that reuses an accepted ID is rejected and makes that name ambiguous for future unversioned dependencies. Its original accepted work remains intact. A new `when.after` using that name returns `dependency_ambiguous` with `accepted_versions`; `status.dependency_ambiguities` lists affected names/versions. To intentionally use a known accepted version, select it explicitly:

```jsonl
{"id":"follow-original-pause","command":"state.get","args":{},"when":{"after":"pause-b","after_version":1}}
```

`after_version` is a positive acknowledged version selector; the CLI resolves it to the existing backend dependency ID. Unknown/unacknowledged names or versions return `dependency_unknown`. A versioned selection does not clear ambiguity for later callers. Only a successfully accepted update clears that name; ordinary update replacement still retargets pending dependents atomically. Identical retries never create ambiguity. A rejected atomic append creates no acknowledged versions for its new IDs.

After an update, follow the returned `item_id` and `version` together. Events from the replaced version may still arrive, including its cancellation; they do not describe the replacement. A rejected edit leaves the last accepted version authoritative.

```jsonl
{"op":"update","id":"start-b","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"load-b","type":"after_beats","clock":"A","beats":8},"request_id":"revise-start-b-1"}
{"op":"remove","id":"start-b","request_id":"remove-start-b-1"}
{"op":"status"}
{"op":"call","command":"state.get","args":{},"view":"compact"}
```

`status` inspects this queue: pending items, counts and eight recent terminal items, with omitted-history counts. Logical item IDs/versions match the stream. Use `jobs get JOB_ID --detail` only when older full receipts are needed. Explicit `op:"call"` makes an immediate canonical request, useful for one-off library inspection or recovery reads; an ordinary `command` object always queues. Reuse an identical `request_id` and input after an uncertain acknowledgment to retrieve its receipt without replay. Changing its body is a conflict. The CLI handles queue revisions; do not copy CAS counters into the DJ loop. Duplicate acknowledgments carry `duplicate:true,replayed:false`.

Consume the already-running process's output to await progress. An idle stream means accepted future work is still pending, not that the agent must poll. Completion/failure notifications are the prompt to prepare a released deck, append the next known actions or revise the pending plan. Keep short notes: process handle, job/session IDs, chosen sources, logical item IDs, current live deck and musical intent. `session_resync` contains authoritative queue status after an event-stream gap.

## Stop, takeover and reconnect

```jsonl
{"op":"stop"}
```

`stop`, or stdin EOF, cancels only this session's pending work and closes the stream. It does not generally pause ordinary playback or restore knob values. If the user says stop the music, explicitly pause the owned playing decks as well. If they only request control handover, leave healthy ordinary playback alone. Manual edits invalidate conflicting work; inspect and respect takeover instead of restoring remembered controls. A listening-level adjustment alone does not require rebuilding the mix.

After compaction, reconnect to the existing process output and use `status` once if needed. Never launch a second copy of the set to recreate lost context. After process loss, inspect the known job with `jobs get JOB_ID` before cancelling or replacing it. App restart interrupts unfinished jobs and never automatically replays playback. Connection loss is reported as `session_error`/`session_closed`; do not claim pending commands completed.

The older `session --stdio` is a raw synchronous request stream and does not own this queue. The lower-level `batch.run/append/invalidate/seal`, guarded handover schedules and native ramps remain available for advanced existing clients; discover their installed schemas with `commands describe`. They are not the default live agent workflow.

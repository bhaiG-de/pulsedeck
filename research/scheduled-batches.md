# Scheduled handovers through the unified control interface

`batch.validate` and `batch.run` support two modes. An ordinary `steps` array remains a sequential job: an applied ramp may still be running when its step completes, failures retain earlier controls, and steps have no future time anchors. Adding `schedule` compiles the same `engine.command` and `automation.ramp` requests into the bounded native queue. Existing sealed handovers remain supported; open queues accept later revision-guarded append, invalidate and seal operations. There is no separate timeline namespace or model planner.

The generated schedule/prerequisite/step schemas are exposed by `commands describe batch.run` and `commands list`. The schedule schema also appears in `input_schema.properties.schedule`. All native action definitions come from EngineCommand. Standalone/immediate requests reject scheduled-only step fields.

## Required preparation and plan

Prepare both sources, incoming entry, tempo/key lock, grid and fader/EQ isolation first. Wait readiness and inspect the **loaded deck's effective** events. Cached source analysis and section/cue labels do not supply the clock. Choose musical windows and protect the requested first payoff before constructing the batch. Save preferences, first-playback UTC/frame baseline and prepared follower in `schedule.intent`; the runtime persists this data without interpreting it as a recipe.

A schedule binds `operation_id`, `clock` (A/B or0/1), both `expected_load_revisions`, `expected_position_revisions`, `expected_grid_revisions`, `expected_engine_epoch`, `expected_control_revision`, `arm_before_event`, `completion_event`, `recovery` and optional `pause_outgoing`/`intent`. The backend captures omitted mechanical guards from one coherent snapshot and defaults the arm deadline to the first event; explicit guards remain enforced. Read explicitly supplied revisions from one fresh status; load revision and position revision are different guards. Grid revision changes for grid edits/applied reanalysis, independent of vocabulary metadata edits. Runtime/audio epochs change on renderer lifecycle changes.

Each step retains `command`, `args` and its target `expected_load_revision`, and adds the clock's actual `at_event` index, optional `deadline_event` (defaults to the same event), and optional typed `require`. Neural indices refer to the loaded detected event sequence; manual uniform indices refer to the explicit constant-tempo lattice. Indices cannot extrapolate beyond native detected events or the track's range.

Every audible fader phase requires the carrier playing/ready and advancing from the declared entry by a positive `min_advanced_beats`. Phase tolerance and optional meter-sized bar tolerance can be asserted; the latter requires credible grid origins on both decks. `arrival_before_event` asserts the planned incoming arrival remains ahead. These are operational checks, not listening evidence or arrangement inference.

Sealed final controls must fully open the completion carrier, isolate the other deck and settle every declared endpoint. Open queues declare `open:true`, a renderer lease `expiry_seconds` (0.1..600), a hard future source `completion_event` and optional `completion_carrier`; expiry invokes the declared recovery. Recovery and eventual completion carriers can differ. Recovery declares a healthy carrier, a renderer-seconds duration0.02..5 and up to eight existing controls; it must restore full faders and every affected carrier EQ band. Recoveries run inside the armed native executor, including after outgoing EOF; they never wait for another model turn. Before an isolated failure, preserve healthy outgoing audio. Manual/source/position/grid/tempo changes yield and cancel owned work without restoration. Renderer stop interrupts work; the queue cannot advance without rendering.

## Exact fixture and submission

[The tested fixture](examples/scheduled-handover.fixture.json) is for isolated generated audio at120BPM, a1000Hz test renderer and zero load epochs. It is **not a live-session plan**. The native fixture tests verify its event coordinates, guards, source continuity, full endpoints, recovery and grouped control behavior. Bind all identities/revisions/entries/event indices and actual durations from the requested live sources before using this shape.

```sh
pd commands describe batch.run
pd status --view handover
pd batch validate --file prepared-handover.json
pd batch run --file prepared-handover.json
pd jobs get JOB_ID
pd jobs wait JOB_ID --timeout 120s
pd status --view resume
```

Validation is read-only and does not reserve time/resources. Run atomically rechecks all guards/deadlines at the render boundary and either arms every phase or none. All same-event steps are checked before the group applies on one render frame. Overlapping same-target ramps are rejected; adjacent future segments are supported. Capture each segment's start value at actual start. The current bounded capacity is one active queue with 64 lifetime action slots; prepare repeated selection ahead through the AI session loop. Use `pause_outgoing:true` to pause the clock only after the verified completed handover; do not insert a premature clock pause as a scheduled step.

## Receipts, cancellation and continuity

Accepted `batch.run` returns `job_id`, operation ID and lifecycle. `jobs.get/list`, `events.get` and `status` expose native state, reason, event receipts and actual rendered frames/source times. `completed` proves requested endpoints and healthy carrier playback, whereas `recovered` reports a failed routine that stabilized; `failed`, `cancelled`, `yielded` and `interrupted` are distinct terminal outcomes. Ramp inactivity alone is never the scheduled job's success criterion. Jobs wait exits7 for yielded/interrupted/cancelled,5 for failed,6 for wait timeout, and0 for completed/recovered; inspect the terminal JSON state to distinguish the latter.

`jobs cancel JOB_ID --mode yield` removes only owned pending controls/launches and leaves knob values. `--mode stabilize` requests its still-valid predeclared recovery. Explicit `automation stop` preserves existing global takeover semantics. Same operation ID/content resolves to the existing job without replay; changed content conflicts. A timeout does not authorize redispatching a new ID.

The existing Control store persists intent, compact source identities, first-playback frame/time baseline, schedule, job/native receipts, request receipts and event history through an atomic fsynced journal outside the renderer. A model/tool disconnect or compaction does not stop an armed queue. `status --view resume` returns the most recent scheduled job plus current real state. On app restart all nonterminal saved jobs become `interrupted` and retain their history/intent; controls are never automatically replayed. Reconcile current epoch/sources before preparing a new operation.

Read-only `--fields` accepts up to32 JSON pointers relative to `result`, preserving envelope identity, stages and structured errors. Unknown pointers fail explicitly. `--view handover|compact` trims peripheral state; `--view resume` includes continuity. `commands list --prefix batch. --names-only` provides compact discovery. Stdout remains one JSON envelope per line for JSON/stdio/watch; parse all lines rather than treating watch output as one JSON document.

## Incremental updates and completion notifications

Open-queue steps have unique string `action_id` values. `batch.append` takes the existing job `id`, stable `patch_id`, `expected_revision` and new future `steps`. Optional `invalidate_action_ids` atomically replaces pending work in that same patch; replacement actions get new IDs. `batch.invalidate` cancels named pending actions, respecting atomic event groups and launch dependencies. `batch.seal` fixes final completion and validates the complete handover endpoints. All edits recheck CAS, source/position/grid/engine ownership and deadlines at the render boundary. Rejected edits preserve accepted work; applied actions never replay. Exact patch retries return their durable receipt, including after uncertainty; a restarted queue remains interrupted.

`jobs.get/list` default to compact action/patch receipts, queue revision and next event. `--detail` requests full plans/results. Waveform arrays and model activations stay out of default load-job responses. Ordinary queued `engine.command play` compiles to the existing source-guarded native launch. Omitted fader prerequisites use the completion carrier's captured position with ready/playing and one-beat advancement requirements; explicit phase/meter/arrival prerequisites remain available.

`events wait --since CURSOR --job-id JOB_ID --timeout 60s` waits on the existing event stream. An off-callback observer publishes `job.action`, `job.patch` and `job.updated` semantic changes without a CLI status request. Native receipts distinguish dispatch frame from verified completion frame, so a ramp start is not reported as its endpoint finishing. The agent can wait, prepare ahead, append or atomically update while native timing continues. The observer sleeps while idle and wakes on control events; it does no analysis or waveform work.

See the concise [DJ execution reference](../docs/CLI.md) for the agent workflow. `tools/skill-trials/incremental_queue_workflow.py` exercises the CLI against generated muted audio in an explicitly isolated profile; it is not a musical strategy.

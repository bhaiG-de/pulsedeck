# Execute the musical handover

For supported recipes, use [assisted next moves](assisted-next-moves.md): `mix.execute` owns preparation and the full handover in one action. The procedures below are for custom transitions that need primitive controls.

Use `$djing-art` to choose the musical relationship, and [deck preparation](deck-preparation.md) to establish a ready isolated incoming source. Read the [persistent command contract](../../references/timed-execution.md) once for exact input/output shapes. Planning-only requests do not enqueue changes.

## Bind the plan to observed state

Retain both source identities and load revisions, current positions and position revisions, effective grids, meter/bar origins, actual event timestamps, remaining audio, preparation and loop state, roles/rates, routing, mixer settings and pending work. Use one simultaneous snapshot for comparing native `beat_position` values. Save large metadata to files.

For a beatmix, require credible local timing and review any `rhythm_error`, repaired pulse, ambiguity or stale-grid warning. Read [analysis semantics](../../references/musical-evidence.md) rather than assuming a high score validates the clock. Confirm bar and arrangement progression separately from beat phase. For same-meter validated origins, a two-beat difference in 4/4 can be fractional-beat aligned and still in the wrong part of the bar.

Bind the musical plan's chosen absolute launch, B entry/arrival, audible entrance, band/foreground transfers and A clearance to actual source events. Allow verification before B reaches master. Preparation completion is a dependency, not a phrase cue. A queued `deck launch` alone schedules only the start, not the subsequent fader/EQ movements.

## Check that the controls deliver the requested technique

For a smooth blend, the musical plan supplies first audibility before final foreground ownership, a positive useful audible overlap, role-transfer events and outgoing clearance. Map each to supported controls. A positive source pre-roll can remain silent; a group that opens B and closes A simultaneously is still a cut. A sharp bass exchange inside a developed overlap is different from swapping both whole channels instantly. Do not pause A at B’s playback entry when A is meant to support B’s approach.

Build gain/EQ/fader stages around actual content. Incoming lows can stay restrained while A supplies bass, then replace A when B’s bass exists. If A withdraws earlier than expected, revise the musical plan rather than mechanically retaining a bass cut until EOF. Ramps take their durations from the selected musical intervals; a fixed six/eight seconds does not establish phrasing. Musical rationale and role selection live in the art skill; this reference owns translating and verifying the plan.

Translate the art plan's bar count into actual beat intervals and source anchors. In 4/4, an 8–16-bar audible blend spans 32–64 intervals; it is not an 8–16-second fade. Keep silent pre-roll, individual ramp lengths and the full audible overlap distinct. Use supported beat-clock ramps for chosen beat spans, with absolute source-event triggers for the planned stages; use exact event timestamps for duration conversion when needed, especially with variable tempo. Arm the whole entrance → exchange → clearance before waiting, then verify completed receipts and actual audible contributions. The bar count never makes the model responsible for firing each stage live.

## Register the complete known handover

Use the current persistent process or start one for a standalone transition. Submit preparation → launch → audible opening → role exchanges → clearance → pause as dependent items. Use one atomic `op:add,items:[...]` append when an accepted prefix alone would leave the mix incomplete. Each item still executes at its own condition.

Preserve an inspected future boundary with `when.type:at_seconds` or `at_event` on the clock deck. A relative `after_beats` counted from an earlier snapshot slips during tool/model latency. Use completion-relative timing only when that is the intended relationship. Readiness `when.after` can accompany the absolute launch; if readiness misses it, revise instead of starting late.

Inside the persistent session, enqueue `engine.command` with `args:{"type":"play","deck":"B"}` and put the chosen absolute trigger on that item's outer `when`. Do not combine it with inner `play_quantized` clock scheduling or a standalone `deck launch`. Use the exact inspected event timestamp, not a mean-BPM estimate or rounded substitute. This gives one owner and one timing condition to inspect on failure; it does not guarantee that every runtime/provider launch succeeds.

Use a `commands` group for disjoint controls that should start together: for example both sides of a bass exchange, or volume plus high EQ. One trigger starts members on the same renderer frame; each ramp keeps its duration and group completion waits for all endpoints. Loads stay separate. Put launch quantization on the outer timing, not individual grouped ramps. See the shared contract for schemas, supported sizes and dependency versions.

Native launch aligns selected beats without requiring equal BPM, enabling Sync or changing rate. Prepare tempo/Sync deliberately for sustained rhythmic overlap; an intentional cut can use unequal tempos. The source-clock revision for standalone `--clock-revision` is the clock deck's load revision, not its position revision. A snapped B entry still needs to represent the chosen musical entry or pickup.

## Verify without making the model the clock

Consume `job.action` feedback. `registered` acknowledges input; `queued` awaits timing; `running` has started. Only `executed` for the acknowledged item/version proves completion, including all grouped ramp endpoints. Native timing continues across model turns/compaction.

Keep B isolated while validating its scheduled start and phase, with enough lead before its intended audibility. The verification start advances its musical position. Do not open it after an arbitrary delayed read and assume the original arrival is preserved. For a sustained blend, compare phase over time through appropriately timed verification, without polling to fire each fader stage.

Wrap simultaneous native `B.beat_position - A.beat_position` into `[-0.5,0.5)` for beat phase. For credible same-meter origins, also inspect modulo bar length and the countdown to the actual arrivals. Do not reconstruct neural beat position with average BPM arithmetic. There is no one phase threshold proving all timbres sound aligned.

Known actions should already be armed before a model pause. Inspect only on meaningful ambiguity, failure or a required musical check; do not insert a blocking wait after every queued item. Watch source identity, actual playback, remaining time and intended role transitions as well as numeric alignment.

## Signal path and final state

Native ramps directly change their engine targets and do not reproduce Smart Fader/Smart CFX compound moves. Avoid crossfader gestures that unintentionally engage those behaviors. Verify final incoming EQ/filter/FX as well as fader/bass; a successful bass exchange does not establish full spectral balance.

PulseDeck has independent main and headphone echo/reverb returns. Main sends follow channel/crossfader gain; headphone sends are pre-fader. Closing the send or pausing/replacing its source leaves existing return audio to decay for at most eight seconds. Use assisted `echo_out` or `reverb_exit` for the native guarded recipe; custom sends can use the `echo`/`reverb` ramp targets. Returns remain under master gain/protection. Reset effect sends deliberately; do not mistake a lingering return for an advancing incoming carrier.

Before pausing/unloading A, verify B source/revision, healthy advancing playback, readiness, intended tempo/Sync/master/loop state, final EQ/filter/FX/fader and actual master route. Verify A isolated and all relevant ramps complete. An `applied` zero target alone does not prove smoothing settled. Prepare the released deck promptly after verified clearance.

## Failure and takeover

Inspect structured errors once and correct the specific problem from discovery. If timing or preparation misses its boundary, update pending work to a credible alternative; do not replay stale Play or the entire blend. Cancellation leaves already-applied knobs in place. Inspect current audible contributions before choosing a supported stabilization within the requested task.

Source/position/grid/tempo changes and manual takeover can cancel conflicting native work. Preserve the user's values rather than resetting them. An unrelated headphone or listening-level change need not cancel independent work. Respect ownership; do not silently cancel another operation's ramps.

For session stop use `op:stop`; for a requested global takeover use `automation stop`, then inspect actual pending work. Playback and recording require their own requested stop/finalization actions. Report completed actions, current audible ownership and any unverified musical questions rather than a generic “success.”

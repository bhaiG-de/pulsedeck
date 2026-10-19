# From incoming data to musical decisions

Use a rolling source-specific map, not a whole-library dump. Inspect the current passage, the next useful outgoing opportunities, and a few incoming entry/arrival candidates. Read [analysis semantics](../../references/musical-evidence.md) for field limitations and [track inspection](../../pulsedeck-library/references/track-knowledge.md) for bounded reads.

| Incoming evidence | Musical inference and decision | Reconsider when |
|---|---|---|
| Source identity, duration, load/grid/position revisions | Bind every cue and calculation to this edit and effective clock. | Reload, relink, seek or grid change invalidates the old relation. |
| Beat events, beat positions, meter and downbeat origin | Count entry-to-arrival intervals; check bar relation and resolve exact launches. These locate musical events once chosen. | Missing/truncated events, uncertain origin, irregular pulse or live phase disagreement. |
| Whole-track BPM plus local BPM, actual rate and Sync/master state | Shortlist pulse-compatible songs, then establish whether their selected passages can sustain the intended groove. | Local interval changes or role changes alter arrival travel time. |
| Human cue spans, origin/confirmation and neighboring changes | Identify candidate beginnings/endings, protected statements, sparse approaches and arrivals. Check internal changes, not only section endpoints. | The marker belongs to another edit, or context/listening contradicts the proposed musical role. |
| Low/mid/high RMS and peaks over neighboring windows | Locate changes in weight, body and top rhythm; decide when one track can replace a layer supplied by the other. | A loses weight earlier than the planned bass transfer, or B remains sparse. Bands alone cannot identify a vocal. |
| Key labels, confidence, actual tempo/pitch and key lock | Assess sustained tonal overlap; reduce tonal exposure or choose another passage when compatibility is uncertain. | Pitch state changes, conflicting listening feedback or uncertain key estimates. |
| Current position, audible contributions, readiness and action receipts | Determine which musical opportunities remain; prepare early, confirm actual execution, and shorten/revise after failure. | A launch fails, an arrival passes or the clock stops. A running job can contain failed actions. |
| User response or listening | Assess groove, tonal fit, foreground conflict and whether the transition served the requested moment. | Feedback contradicts a numerical prediction. Preserve the observation rather than defending the score. |

## Work backward from an arrival

Suppose A's effective beat 704 is an outro start at 346.267303s; beat 736 is a bass withdrawal at 362.001648s. B's desired arrival is beat 96 at 46.223789s. Candidate entry beat 30 is 14.758048s, while beat 32 is 15.739364s. Both origins are credible 4/4 downbeats at beat zero.

Starting B30 on A704 aligns pulse but offsets the bars by two beats. B96 is 66 beats away. Calling this a 64-beat approach is false. A candidate B32 gives 64 beats to B96 and preserves the downbeat relation with A704. This corrects the count; it still does not prove that either musical entry works.

Next inspect the full proposed overlap. If A's low band falls sharply at beat736 while B has bass throughout its lead, a plan that cuts B's lows until A ends will leave the foundation unsupported. Options include transferring bass at A736, choosing a shorter/later entry, or deliberately using the reduction as a release. Check what B contributes then and what happens at B96. Do not treat the file endpoint as A's best foreground handover merely because it is available.

Mean-BPM arithmetic can estimate available wall time, but cannot replace these event differences. With nonuniform grids, equal mean BPM does not imply equal local intervals. Inspect local timing and verify native progression through the overlap. A bar-aligned count also needs arrangement evidence: an arbitrary beat32 is not automatically a phrase start.

## Read the stream as a changing plan

After preparation, retain exact anchors and queue the known stages ahead. On launch feedback, establish whether the planned source actually advanced. Compare simultaneous beat/bar phase and remaining beat distance to the named arrivals. A ramp ending at source45.248 when the intended arrival is46.224 is evidence to inspect, not proof that the arrival landed.

After clearance, inspect the next useful passage and prepare the freed deck while healthy playback continues. For a bounded set, select a closing passage with a deliberate endpoint; for an ongoing set, repeat preparation and handover. Do not confuse “the next transition was queued” with a ready successor or completed launch.

If launch fails, its planned arrival relationship no longer holds. Re-read the actual advancing sources and remaining musical opportunities. Carry the revised purpose and boundaries to [session recovery](../../pulsedeck-control/references/session.md#recover-before-the-healthy-track-ends), which owns cancellation, inherited controls and restoring continuity. Report a gap if one occurred.

These coordinates illustrate an observed failure and its arithmetic; they are not track presets or a claim that the corrected blend has been auditioned.

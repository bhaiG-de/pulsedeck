# Check a selected musical timeline

Use [phrase_check.py](../scripts/phrase_check.py) to validate coordinates for a supported, same-meter, linear transition with selected effective event anchors. It only reads `status` and `track.inspect`; it never loads, seeks, changes knobs, launches playback or writes library metadata. Standard Python is sufficient. It is a timing validator, not an automatic DJ or a listening substitute.

Choose A's complete outgoing statement and a future handover, B's intended arrival and a coherent entry. Retrieve exact timestamps from effective beat events, using any human-authored cue knowledge and surrounding passage evidence. Inspect the neighborhoods for musical support: a named `Drop` marker does not establish the right handover; a bass withdrawal or level increase is evidence about texture, not a lyric ending. For a blend, prepare the incoming final entry and outgoing-master/incoming-follower Sync through the control workflow before checking. Both source identities and grids must be ready.

Set `DJ_ART_SKILL` to the directory containing this skill's SKILL.md, `PD` to the installed CLI executable, and the four timestamps to observed **source seconds**. These variables are selected values, not wall time or average-BPM estimates:

```sh
python3 "$DJ_ART_SKILL/scripts/phrase_check.py" \
  --cli "$PD" --outgoing A --incoming B \
  --launch "$A_LAUNCH" --handover "$A_HANDOVER" \
  --entry "$B_ENTRY" --arrival "$B_ARRIVAL" \
  --technique blend --save "$task_phrase_evidence"
```

Reverse the outgoing/incoming deck arguments on the next handover; the variable names here describe roles. A smooth-set brief uses `--technique blend` after designing positive audible overlap. Do not change to `cut` merely to obtain a pass. Use `--technique cut` only when a real non-overlapping ownership exchange serves the brief, not an unequal-tempo multi-second fade. No extra `--json` flag is needed: output is JSON. `--save` is optional and saves the check, raw selected inspection packets and compact state for later verification. Reuse the running CLI instance; do not restart an app or switch profiles for this check.

The result explicitly reports `scope:clock_coordinates_only` and `audible_overlap:not_evaluated`. A positive beat lead can be completely muted, so it cannot prove a blend.

Exit 0 means `timing_pass:true`; exit 1 means the proposed relation fails; exit 2 means the evidence/input could not be checked. Read `errors`, exact anchor positions, both beat leads, meter and source-owned `anchor_evidence`. A failed check requires a different entry/handover or corrected preparation. Do not round it away or submit the old plan while investigating other tracks.

Typical corrections:

| Failure | Musical correction |
|---|---|
| B entry beat30→arrival96 gives 66 beats against 64 on A | Consider a complete entry at beat32, or a different A opportunity. Confirm the new entry still makes musical sense. |
| A702→800 gives 98 beats while B96→192 gives96 | Find the actual matching launch beat704 on A **and** verify its musical position, or choose another entry/arrival. Similar source seconds cannot reconcile the count. |
| A's selected exit is beat633; B starts at352 | A633 is not a downbeat in the published 4/4 origin. Use a genuine phrase completion, such as an evidenced later breakdown, not whichever beat makes the target duration fit. |
| Both clocks have similar mean BPM but no live following | Prepare valid master/follower Sync, then recheck. Inspect pulse uncertainty rather than forcing a long blend across repaired or ambiguous timing. |
| Launch already passed | Choose a viable future musical opportunity. Keep the healthy deck and prioritize the currently loaded pair. |

The checker uses published meter, effective beat positions and downbeat flags. It reports available manual/imported cues as optional context. If these are wrong, a numeric pass can still be musically wrong. It rejects truncated event intervals, unequal leads, incompatible bar positions and an unprepared sustained-beatmix strategy. It does not require saved cues or create any. It does not choose tracks or decide whether a vocal, harmony or emotional release works.

After the runtime launches, use native simultaneous `beat_position` values for fractional beat **and modulo-meter bar** checks. Compare both remaining beat distances to the selected arrivals. Keep the incoming isolated during a useful verification lead; the known audible exchange must already be armed, and a failure calls for revising pending work before it opens. Sync aligns pulse, not phrases. Do not fix a live disagreement by silently redefining which phrase was intended.

## Before the first song starts

Prepare both sources while paused, including the final incoming entry and valid master/follower Sync, then add `--preflight` to the same checker command. This checks the same intervals, downbeats, pulse warnings and Sync strategy while allowing the outgoing clock to remain paused. The response has `preflight:true` and `execution_ready:false`: it validates a prepared timeline, not a performed mix. A preflight on an already playing outgoing deck is rejected; use the normal live check then. Validate the complete control append separately through `pd batch validate --file`, and include opener Play plus the full handover in one submission. Avoid spending the opener's musical runway debugging a plan that could have been checked before playback.

## Scope limits are not artistic verdicts

The helper checks same-meter, linear beat-aligned anchors. Missing manual cues do not make those coordinates invalid; a numeric pass also cannot make them musical phrase boundaries. It does not cover active-loop plans, genuinely beatless bridges, offbeat pickups not represented by an effective event, or source positions not represented by effective beat events. It cannot certify the first audibility, layer timeline, fader curve, tonal fit or preserved payoff. Select supported passages for an ordinary autonomous blend where practical; do not modify library cues just to satisfy it.

For a separately supported arrangement outside that scope, retain an explicit manual interval/bar audit and uncertainty rather than claiming the helper passed. Preserve real pickups and irregularities; never truncate them to satisfy a checker. A real count mismatch or stale source is still a defect to resolve. A checker limitation is not a reason to replace the requested smooth mix with an abrupt cut.

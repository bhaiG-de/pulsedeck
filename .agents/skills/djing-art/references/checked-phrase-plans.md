# Check a selected linear timeline

The existing [phrase_check.py](../scripts/phrase_check.py) is a read-only coordinate checker. It uses status and track inspection; it never loads, seeks, plays, changes controls or edits metadata. Its output scope is `clock_coordinates_only`, with `audible_overlap:not_evaluated`. A pass cannot identify a phrase or certify a blend.

## When it applies

Use it for selected effective beat-event anchors on a same-meter, linear custom handover. Choose the musical relationship first using [phrasing and arrangement](phrasing-and-arrangement.md). Inspect the complete intervals and retain exact source timestamps. The checker also examines ready grids, incoming paused state, future launch, outgoing playback and the sustained timing strategy.

For `blend`, establish the incoming final entry and a supported outgoing-master/incoming-Sync-follower relationship, or genuinely matched uniform clocks, through control. The helper checks prepared timing strategy; the execution plan still has to place B at its intended entry and open it appropriately.

Set `DJ_ART_SKILL` to the directory containing this skill's SKILL.md and `PD` to the intended CLI executable. Set the four timestamps from observed effective events in **source seconds**, and optionally choose an evidence output file:

```sh
python3 "$DJ_ART_SKILL/scripts/phrase_check.py" \
  --cli "$PD" --outgoing A --incoming B \
  --launch "$A_LAUNCH" --handover "$A_HANDOVER" \
  --entry "$B_ENTRY" --arrival "$B_ARRIVAL" \
  --technique blend --save "$task_phrase_evidence"
```

Reverse deck arguments when their roles reverse. No extra JSON flag is needed. `--save` is optional and saves the check, raw selected inspection packets and compact state. Reuse the intended running instance.

Exit 0 reports a timing pass, exit 1 a failed relation/preparation, and exit 2 unusable input/evidence. Read the errors, both exact interval counts, meter and anchor evidence. A 66-beat B approach does not fit a 64-beat A window; find another complete entry or another handover rather than rounding. Missing manual cues alone do not fail the checker.

`--technique cut` describes a direct joint arrival with zero lead on both decks. Use it for an actual cut that serves the brief. It cannot validate an unequal-tempo multi-second fade or excuse missing blend preparation.

## Paused preparation and live verification

With both sources paused and prepared before the opener, add `--preflight`. It applies the coordinate and timing-strategy checks while allowing A to be paused. It returns `preflight:true` and `execution_ready:false`; it has not performed anything. Preflight requires a future A launch after its current position. On a playing A, use the normal live check.

Control owns batch validation, source guards and arming the full sequence. Do not spend the opener's available musical runway debugging a plan that could have been checked while paused. After launch, compare simultaneous native beat/bar positions and remaining intervals to the actual arrivals. If a rehearsal or launch failed, reconcile the real timeline before opening B.

## Scope ends before musical judgment

Active loops, offbeat pickups without event anchors, beatless bridges, different meters and other nonlinear arrangements need a separately counted plan and a supported execution path. The helper cannot certify vocal completion, tonal fit, first audibility, bass ownership, fader curves, post-arrival pacing or effects.

Do not remove a pickup or alter a library grid to fit this checker. Do correct genuine unequal leads, stale identities and unintended bar offsets. A checker limitation and a wrong musical count are different findings. Keep the explicit manual audit and uncertainty when performing a valid plan outside its scope.

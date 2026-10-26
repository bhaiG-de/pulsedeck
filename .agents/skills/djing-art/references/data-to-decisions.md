# Decode structure and make a musical decision

Book basis: Morse's timing, three elements, loops and EQ chapters; Broughton/Brewster chapters 10, 13–14, 28 and 31. Their DJs learn records by listening. The following is an agent adaptation using available waveform structure, key and tempo. It supports musical inference without pretending a band is an instrument stem.

## Read at three scales

1. **Whole source:** bind exact version, duration, analysis/grid identity, meter, local tempo and normalization. Overview recurring dense and sparse regions. Repeated low/mid/high shapes are candidate statements and returns. Keep a few useful regions as a working shortlist rather than re-reading the whole song each move; preserve all intervening changes in the course you actually select.
2. **Counted neighborhoods:** inspect before and after prospective changes in four/eight-bar blocks, then compare longer groups. Find a stable pattern, a departure, its duration and the subsequent state. A fill-like isolated peak differs from a sustained new region. Repetition plus a persistent boundary supports a phrase-origin hypothesis; a multiple of 32 alone does not.
3. **Exact events:** narrow around the chosen launch, entry, transfer and arrival. Use complete effective beat timestamps and source positions, including pickups and irregular extensions. Split truncated event reads. Keep the future development visible; stopping inspection at clearance hides the musical consequence.

The [offline passage condenser](../scripts/passage_map.py) reads captured `track_inspect` MCP packets and returns a compact counted map while retaining exact event anchors and provenance. Obtain additional bounded tool reads for truncated ranges before condensing them. Choose the requested source/range yourself; it never chooses music, entrances or controls. The lower-level [structure-map helper](../scripts/structure_map.py) condenses already saved JSON into counted band blocks. It reads files only. Give it an overview and bounded event reads where needed; inspect its coverage/provenance output. Its blocks are measurement intervals, not automatically musical phrases. Exact source reads and the model's interpretation remain authoritative.

## Infer functional states positively

Compare relative changes within the track and normalization-aware levels between tracks. Use patterns, not a universal numeric threshold:

| Observed trajectory | Useful inference | Arrangement consequence to consider |
|---|---|---|
| Repeating low-band pulses/weight with stable body | Continuing foundation | Retain it under an incoming sparse approach, or exchange it at a suitable boundary |
| Sustained low withdrawal, thinner body, continuing upper activity | Reduced foundation / possible preparation or relief | Count its length and locate the return; choose deliberate space or support it from the other deck |
| Increasing upper/body activity or repeated accents while weight is withheld | Possible increasing tension | Preserve anticipation; time an arrival to resolve it, rather than clear into another unrelated long wait |
| Sustained low/body return after withdrawal, with renewed peaks | Candidate impact / release | Compare the approach and post-return state; align or replace a payoff intentionally |
| Similar weight with a new band balance or attack pattern | New color without necessarily more drive | Introduce it as development rather than claiming every change raises energy |
| Both sources withdraw together | Composite relief, or an unintended hole for a continuity brief | Design the waiting interval and return, move the offset/entry, or retain a supported rhythmic bed |

Call these measured/inferred functions. “Strong low return after 16 bars” is already actionable; it does not need the verified semantic name “drop.” Sustained mid/high energy alone cannot identify a voice or complete lyric. Use heard/user-annotated content for that claim, or manage foreground exposure so the plan does not depend on it.

## Combine two trajectories

Build a small table at changes, not just an average:

```text
A beat/time | B beat/time | A foundation/body/activity | B foundation/body/activity
           |             | audible treatment and ownership | predicted composite change
```

Include the approach, the full overlap, clearance, and at least the next developed phrase/known return. For a long withdrawal, include the entire waiting interval. Predict continuity, intentional reduction, increasing anticipation, impact and settled continuation separately. Loudness, drive and tension are distinct: a quieter passage can intensify expectation; raising gain cannot replace absent rhythmic material.

When comparing level, apply documented normalization gain to source amplitudes and consider faders/EQ, attack and density. Raw RMS across differently mastered tracks is not a played-energy comparison. Summed amplitudes or bands are approximate support estimates, not a calibrated listener model; phase, masking and arrangement affect the actual result. Inspect denser later material before setting trim so a quiet entry does not cause overload on return.

Compare the first plausible plan with a concrete alternative when the musical result is uncertain: launch earlier to let B develop while A carries; enter B at another complete approach; retain a clean A unit; or choose a different successor. State what changes in the combined course. More continuation time is useful only after the resulting course serves the brief.

## Read the scored transition forecast

`performance_validate` returns a `forecast` of the actual score against the loaded sources' cached envelopes. Window seconds are relative to `forecast.origin_rendered_frame`, with its declared `sample_rate`; source seconds and effective beats are separate coordinates. Use that origin to align the forecast with the score observation and actual execution. Read the approach, exchange, clearance and following development together: source seconds/effective beats, normalized source bands, fader/EQ endpoints, nominal contribution cues, band dominance and phase progression. Source identities and uncertainty explain what supports each projection. Follow `omitted_ranges` and coverage limits back to bounded source inspection when an important musical event is missing.

Use the forecast to answer a musical question before committing. For example, A's low envelope falls four seconds after launch; B already contains useful low weight but its low EQ stays at zero until eight seconds, then opens over another eight. For a continuous groove, move the foundation exchange to the measured withdrawal, reshape the envelopes or change the placement. Raising B's fader while its foundation remains attenuated does not solve that relationship. For intentional relief, retain the withdrawal and identify its length, developing upper activity and actual return instead. Compare the revised trajectory with the first one.

The low/mid/high contribution cues are coarse envelope estimates: analysis and mixer EQ crossovers differ, and a zero matching EQ control does not imply perfect silence across that analysis band. They are useful for locating held-back support and changes in ownership, not for predicting summed loudness, vocals or perceived smoothness. Filters, FX, loops and changing source clocks can make projections partial or unavailable. Keep those tools when they serve the idea, then reason from the specific unsupported transformation and verify its native outcome. Separate an irregular detected grid from a proven playing-pulse mismatch; inspect local intervals before interpreting a phase jump.

## Retain knowledge that transfers

Keep version-specific maps in session notes: exact events, the recurring shape, inferred function/confidence and practical uses. A pair note records the two windows, relative timing, roles, controls and measured/listener result under that brief. Reuse the relationship on new music rather than replaying the same pair. Refresh stale evidence after relink, grid edits, seeks or loops; permanent cue/tag edits follow library authorization.

Review a failure by cause: wrong structural inference, wrong offset, lost foundation, premature payoff, crowded foreground, rate/phase drift, tone/level, or execution failure. Execution success does not settle musical quality, and missing audio perception does not invalidate structural reasoning.


## Turn the inference into a counted role map

Retain a compact source-event score before arming an unfamiliar pair. For each important boundary, write the effective beats/times, who actually contains the required support, which source is exposed, and the predicted change. This is a decision record, not a required user-facing narration.

If A loses foundation at beat 544, B's foundation returns at its beat 112, and B112 will meet A560, leaving B lows closed to A576 creates a further unsupported wait. Either expose B's return at the intended arrival, change the offset so it meets A544, retain suitable A support, or choose a deliberate bounded reduction. Compare those actual courses before choosing a ramp. An arbitrary “bass swap after 40 beats” is not a substitute for the source events.


## Efficient live evidence path

Use one work directory per source window. Save the selected `track_inspect` tool JSON as `overview.json`, adding bounded event packets until coverage is complete. Run `passage_map.py overview.json --events events-1.json --save-dir WORK_DIR` from this skill's `scripts` directory. Choose bounds inside the actual duration and cover the approach, exchange and next meaningful return/development. The default compact output carries exact event anchors, normalized/raw amplitude context, ordered blocks, coverage and effective-clock provenance. Full source reads and the uncompressed map remain in WORK_DIR; `--full` returns the full map if needed. Use the exact saved events for finer internal boundaries rather than typing rounded display times.

Read the rows as a trajectory: what persists, withdraws, develops, returns and continues? Place the two ordered courses together, explain the intended combined change, then choose the offset and control shape. The helper supplies measurements and counts, not the musical answer. Reuse an unchanged packet; call `track_inspect` for a narrower window only for the unresolved event. For a genuinely unclocked/beatless passage, retain the raw waveform trajectory and design its exposure under a supported clock rather than invent a bar origin.

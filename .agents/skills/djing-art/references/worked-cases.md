# Worked cases for musical decisions

These invented cases illustrate the policy and provide review criteria. They have not been run as independent agent or audio trials. For evaluation, provide the scenario and raw evidence first; keep the reasoning criteria hidden until grading. Accept other coherent solutions that honor the constraints.

Bars below are one-based and intervals exclude their ending boundary. Unless otherwise stated, meter is confirmed 4/4 with steady tempo, and the described musical content is known. A bar number is not a source timestamp.

## 1 Protect the payoff and hand over before a breakdown

**Scenario:** Maintain drive and hear A's first payoff. A plays at 124 BPM; its first payoff is bars 81–96, its following groove is 97–112, and its breakdown begins at 113. B is originally 128 BPM: drums at 9–16, bass groove at 17–24, main hook at 25. The later A groove has no competing vocal and its motif is compatible with B. Preparation can finish before A97.

**Reasoning:** B's hook can own A113, maintaining momentum as A would break down. B9→25 is 16 bars or 64 beats; launch at A97. B17 arrives at A105, so exchange bass there rather than waiting until A113. Keep A's useful body until B can replace it, then clear A before its breakdown. B's audible drum entrance can be at or after launch once alignment is established, with enough remaining runway.

**Arithmetic:** Lead `64 × 60 / 124 = 30.9677` wall seconds. B entry is 15 source seconds and arrival 45 source seconds at 128 BPM. Rate `124/128 = 0.96875`; `(45−15)/0.96875 = 30.9677` seconds.

**Reject:** Mixing out at the first beat of A81 because the requested drop technically occurred; launching B at A113 and losing drive; swapping bass at launch before B has any.

## 2 An 18-bar approach does not fit a 16-bar window

**Scenario:** At 120 BPM, B's known entry-to-hook span is 18 bars. A113 is the desired handover. A95 is inside an unresolved lead; A97 is a usable entry boundary. B has a confirmed alternative entry two bars after the original, starting a complete instrumental unit.

**Reasoning:** The original lead is 72 beats or 36 seconds, so its launch would be A95. Reject that long overlap because it obscures the unresolved lead. The verified alternative entry leaves 16 bars, allowing launch at A97 and arrival at A113. If that alternative were not known to be complete, choose another handover or technique instead of inventing it.

**Reject:** Rounding 18 to 16; using 32 seconds because that duration is a familiar default; skipping two bars without checking the musical content.

## 3 A pickup begins before the downbeat

**Scenario:** At 120 BPM, B has a one-beat vocal pickup before its first downbeat. The desired B arrival is 32 beats after that downbeat. It should land at A's native beat position 448, the downbeat of bar 113. There is room for the pickup in A.

**Reasoning:** Preserve all 33 lead beats. Launch the pickup at native beat 415, which is bar 104 beat 4. B's first downbeat then coincides with A416, bar 105, and its arrival coincides with A448. Lead is 16.5 seconds. Opening B may need to include the pickup if the audience is meant to hear the whole lyric.

**Reject:** Treating the first decoded event as bar one; chopping the pickup; describing an intentional pickup launch as an accidental bar mismatch.

## 4 Same key and tempo, competing vocals

**Scenario:** A and B are 124 BPM and 8A. A sings through its proposed exit; B begins an important lyric eight bars before its main hook. The proposed blend puts the lyrics together for eight bars. The brief asks for clean vocal transitions.

**Reasoning:** Prefer a different B entry after A's lyric resolves, a later A handover, or a short exchange that leaves each line intelligible. Verify any alternative entry preserves a complete phrase. The key relationship supports tonal fit, but does not solve competing semantic content.

**Reject:** Approving the blend because Camelot and BPM match; claiming a high-pass filter removes A's singer; assuming a two-second fade automatically resolves the lyric conflict.

## 5 Higher BPM can lower intensity

**Scenario:** The user wants a sustained lift at a steady playing 124 BPM. Candidate B is a sparse airy track tagged 128 BPM and 8A. Candidate C is a denser, driving track at 123 BPM and 9A. These descriptions come from audition, not inferred metadata.

**Reasoning:** C is a plausible stronger lift, subject to checking its actual entry/arrival and harmonic overlap. Hold the intended playing tempo if it suits the set. B's higher source BPM does not establish greater intensity; it may instead suit a later release.

**Reject:** Selecting B merely because 128 is larger than 123; speeding every successor to the top of an allowed range.

## 6 The bass arrives before the intended foreground handover

**Scenario:** B needs 16 bars from entry to its main hook, but its bass enters after 8 bars. A's bass remains strong until the handover. Their mids are compatible and A's lead resolves at the hook boundary.

**Reasoning:** Trace the internal 8-bar boundary. Hold one bass foundation, transfer it deliberately when B's bass becomes useful, then give B the foreground at its hook. A staged exchange or appropriate attenuation may fit. If it weakens A's protected material, revise the entry or shorten the overlap.

**Reject:** Both lows at full throughout because handover is still eight bars away; lowering both lows before either replacement exists; discussing only the two endpoints.

## 7 A section label cannot answer a vocal question

**Scenario:** A saved marker is named `Drop`, and numerical inspection shows a rise in low-band energy. No listening, vocal labels or confirmed arrangement knowledge are available. The user requests the first vocal hook before moving on.

**Reasoning:** The data supports a candidate bass/energy arrival, not the first vocal hook. Inspect adjacent context and seek available audition or user cue information while preparing the next deck. Preserve uncertainty. For a live deadline, choose a conservative supported fallback without claiming the vocal requirement was verified.

**Reject:** Saying the first hook played because a marker was named `Drop`; treating loud mids as proof of a singer; repeated identical waveform requests until the file ends.

## 8 A matched grid still drifts locally

**Scenario:** A live recording's summary BPM is 120, but its intended overlap varies locally. B is steady at 120. The inspection's effective events show growing phase drift; its bar origin is also unverified.

**Reasoning:** Equal summary values are insufficient. Validate the local pulse and bar identity. Use supported local synchronization only if it preserves a credible musical result; otherwise shorten, choose a more stable passage, or use a different appropriate transition. Do not flatten the source grid merely to make the numbers equal.

**Reject:** A fixed 32-second fade labeled a verified 16-bar blend; using numeric phase zero from a missing-grid fallback as evidence; treating Sync as phrase detection.

## 9 Preparation misses the intended launch

**Scenario:** B had to start at A97 to reach its hook at A113. Loading finishes at A101. There is a verified later A handover at 129, and A continues healthily through it.

**Reasoning:** A fresh 16-bar approach can begin at A113 and arrive at A129, if the passages still fit. Revise the timeline and ensure readiness. Alternatively use a verified shorter B entry if A113 remains musically important. Do not launch immediately at A101 and pretend B will still reach its hook at A113.

**Reject:** Triggering the stale launch; an arbitrary speed-up to catch the hook; assuming a completed load is an appropriate musical trigger.

## 10 The requested payoff needs a release

**Scenario:** A's tension-heavy buildup leads to a widely expected payoff. The user asks to hear that payoff. B's similarly named buildup lasts a different number of bars. The agent has an effect available.

**Reasoning:** Let A deliver the requested resolution and choose a later exit. A prepared replacement drop would be a different artistic decision and must fit the brief. An effect is not a reason to remove the payoff. If a replacement was requested, count both true approaches and clear the old foreground at the replacement event.

**Reject:** Substituting B simply because both sections are labeled `buildup`; treating a brief first kick as the entire requested payoff.

## 11 Large tempo change with a natural bridge

**Scenario:** Requested order takes a 100 BPM song into a 128 BPM song. A ends its chorus with a known beatless sustained tail; B has a coherent direct entry. Both should retain their intended character.

**Reasoning:** Consider a deliberate transition through the beatless boundary, preserving a complete chorus and controlling tonal overlap. Select an exact B entrance that makes the new pulse understandable. Verify supported effect/tail behavior if used. Continuous kick blending requiring a 28% acceleration of A is not necessary.

**Reject:** Calling the transition beatmatched when it intentionally changes tempo; rewriting analyzed BPM to avoid the mismatch; assuming key lock prevents every artifact.

## 12 Correct execution cannot establish musical success

**Scenario:** Every queued command completes, phase error is small, and A is isolated. The user reports that the handover sounded thin.

**Reasoning:** Accept the listening observation. Inspect whether B was still sparse when A's body was withdrawn, whether both mids were cut, or whether the passage itself was an intentional release. Revise the next plan according to the actual cause and desired result. Preserve execution facts separately from what was heard.

**Reject:** Claiming the handover was good because the queue succeeded; adding arbitrary gain/effects without identifying the missing role.

## 13 An exactly aligned cut fails a smooth-set brief

**Scenario:** The user asks for a smooth continuous house set. A has a credible outro downbeat at beat800. B has a credible developed arrival at160, plus a complete compatible 32-beat approach beginning at128. Both clocks and tonal relationship can support that approach; preparation finishes well before A768. The agent instead starts B160 at A800 with B full and A immediately inaudible.

**Reasoning:** The cut is numerically aligned but fails the requested transition. Explore the complete approach B128→160 against A768→800, introducing useful B support and shaping the layer transfer before A clears. Check the actual internal content before adopting that candidate. No universal minimum seconds is needed to identify that the proposed instantaneous swap contains no developed blend.

**Reject:** Defending the cut because it passed the checker; claiming anti-click smoothing is a musical crossfade; skipping a feasible blend to avoid preparing Sync.

## 14 Silent pre-roll is not an audible blend

**Scenario:** B starts 64 beats before its intended foreground arrival. Its channel remains completely closed until that arrival. At the arrival, the plan closes A and opens B together. The receipts show two playing decks for the full approach and exact timing. The brief still asks for a smooth blend.

**Reasoning:** The logs establish a long source lead but zero developed audible overlap. Name a useful first B contribution during the approach, trace foreground and bass ownership, and revise the audible stages to create the requested relationship. A sharply timed bass transfer inside that overlap can remain appropriate. If the approach has no useful audible material, select a better complete passage or successor.

**Reject:** Counting simultaneous transport as audible overlap; calling muted preparation a gradual reveal; assuming a positive beat lead certifies musical quality.

## Review criteria

Evaluate musical reasoning and execution separately. For each case, look for source-specific anchors, protected material, a justified arrival, correct counting, full-overlap conflict checks, a feasible deadline and an honest account of evidence. Multiple artistic answers can pass. An answer fails if it breaks an explicit musical constraint, invents listening or content, or hides an infeasible timeline behind plausible vocabulary.

These cases are teaching examples. Use new track maps and unseen arrangements for a behavioral evaluation; do not grade by matching their wording. Real transition quality still requires audition.

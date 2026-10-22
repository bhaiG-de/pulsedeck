# Musical reasoning evaluation

These prompts use arrangements and numbers separate from the skill's worked examples. The prompts and this rubric are evaluation definitions; actual results are recorded separately in [Luna trials](luna-trials-2026-10-21.md). Keep this rubric out of the performing agent's context. No audio or playback is required for this reasoning stage.

Run each prompt in a fresh context with the skill and only its linked resources, then save the answer and relevant trace. For an old/new comparison, use a preserved pre-rewrite skill package and the same prompts/model/settings. Repeat varied cases to check stability. Avoid feeding the intended correction into the task prompt. Do not score wording, headings, file reads or command volume as musical success.

Score each case 0 (wrong/unsupported), 1 (partly right or material omission), or 2 (correct, usable, evidence-aware). Arithmetic is deterministic; judgments about suitability can have multiple valid answers. A different artistic choice can pass when it fits the brief and explains the consequence.

| ID | Required reasoning for full credit |
|---|---|
| unequal_leads | B1→21 spans 20 bars/80 beats; launch at A81 to reach A101. Do not overlap unverified bass/foreground blindly or launch at A73 just because the regions start there. |
| bar_vs_phrase | A has 24 bars to the next statement; wait 8 bars, then run B's 16-bar lead. Need actual content/role evidence before an audible plan. |
| pickup_and_rest | Launch two beats before the joint downbeat; preserve and make room for pickup. The rest is still beat one and the first kick is beat two. Do not discard content to pass a checker. |
| irregular_extension | Lead is 18 bars/72 beats; launch six bars later. Accept the explicitly verified unusual boundary and don't round the extension away. |
| drift_and_groove | Fixed offset suggests phase; growing error suggests rate; stable conflicting subdivisions suggest groove/content. Distinct remedies, with verification rather than blind live-master changes. |
| phase_snapshot | B−A = −61.98 beats; fractional error ≈ +0.02, but modulo four ≈ 2.02, a two-beat bar mismatch. Origins must remain credible; neither observation establishes phrases. |
| sounding_pitch | 12 log2(1.08) ≈ +1.332 semitones. Labels at original pitch are insufficient. Consider processing, tempo, exposure or another suitable passage; key lock quality still needs listening. |
| separate_transfers | Transfer foundation at overlap bar 5 when B has suitable bass, keep A's voice leading through its ending, give B foreground at 17. Avoid both a 12-bar low-end hole and competing singers. |
| unknown_content | Measurements support texture/weight changes, not vocals/drop/emotion. Obtain content evidence or choose a lower-assumption compatible passage; don't keep repeating coarse bins or invent a singer. |
| long_set_direction | Address repetition through texture, placement and pacing, with a justified candidate and post-arrival evidence. No automatic BPM staircase or invented crowd fatigue. Multiple candidates can pass with suitable purpose. |
| smoothness_claim | Identifies a cut despite correct pre-roll, adds genuinely audible complementary overlap and staged ownership supported by actual content. Does not claim the unsupplied passages have been heard. |
| intentional_contrast | Honors decisive contrast; a phrase cut/end-to-end is legitimate without forcing a long 128/96 rhythmic overlap or rejecting distant keys automatically. Handles level/entry/ending. |
| loop_countdown | Three total repetitions add two extra eight-bar cycles: 16 bars/64 beats. B arrives 16 bars late absent adjustment. Revise A support/B entry or release/target coherently and revalidate nonlinear scheduling; do not blindly reuse original countdown. |
| fixed_order_and_review | Preserves order and planning-only scope, finds appropriate tonal separation/passage/technique, reserves audible judgment for later evidence, and does not record or write metadata. |

Treat invented content/listening, a violated explicit brief, or incorrect launch/count as a substantive failure even when the prose sounds knowledgeable. Report individual failures rather than hiding them inside an average score.

## Listening is a separate stage

Reasoning scores cannot certify sound. With an authorized recording or an explicitly requested live trial, compare several real transitions and a short set segment using exact track versions. Include ordinary blends, a dense vocal pair and a difficult pulse/phrase case. Log the source anchors, actual timing/control receipts and the audio heard by the listener. Do not let the listening reviewer see the expected winner.

Ask about complete musical statements, audible rhythmic stability, intelligible foreground, bass/body continuity, level, payoff and set direction. Rate musical interest separately from execution reliability. Check both the transition and the first following phrase. Do not record a session simply because this rubric mentions recording.

No acceptance threshold is claimed to be calibrated yet. Record per-case scores and listening comments, then revise only failures actually observed. Passing package validation, phrase-check arithmetic, or this text-only suite alone is insufficient evidence of a great DJ.

## Tempo feedback regression cases (22 October)

These new cases are definitions for future independent reasoning trials; earlier 14-case scores do not cover them.

| ID | Required reasoning for full credit |
|---|---|
| tempo_character | 112/96 requires +16.67%; a high score, key lock and short recipe do not establish natural sound. Prefer a naturally close suitable successor or a musically placed reset into the chosen record near native speed; preserve range and carrier runway. Treat roughly ±4% as a starting guideline, not an inaudibility guarantee. |
| tempo_inheritance | Evaluate natural/local versus actual/target rate, not merely the two playing BPMs. 106/88 requires +20.45%. Inspect inherited Sync/master-transfer state, plan continuation speed before exposure, honor negative listening evidence; avoid an automatic audible snap back after clearance. |
| tempo_reset_overlap | No stretch does not make unmatched active grooves compatible; sixteen seconds can create conspicuous drift. Use a supported complete ending, brief resolved exchange or genuinely beatless bridge, with actual content evidence. Do not call numeric downbeats verified phrase boundaries or let troubleshooting exhaust the carrier. |

| blend_after_tempo_feedback | Prefer the suitable122 BPM unplayed record, match it near120 (about−1.64%) and align credible beats/bars/phrases for real audible overlap with bass/foreground roles. Stop the repeated EOF-fade pattern; preserve range through sound and set-scale direction. A reset is legitimate with a distinct reason, not the automatic stretch remedy. |
| natural_rate_local_fit | Absolute90/local100 gives0.9 rate (−10%). Master promotion preserves effective rate rather than resetting. Final entry then explicit source-relative0% while isolated, followed by actual rate/role verification before audibility. Metadata BPM is not the local setter denominator. |
| repeat_and_provider_failure | Secure the prepared fallback while carrier remains; disclose emergency repeat and do not pretend repertoire variety. Do not await optional account consent until EOF. Preserve sign-in/cooldown, avoid broad retry hammering, retry unplayed sources only after the provider wait; source refusal does not prove a login failure. |

# Programming a set and choosing the next record

Book basis: Morse, “How To Pack The Perfect DJ Set,” “How To Programme A DJ Set,” and “How To Play Bar, Club And Mobile Gigs”; Broughton/Brewster chapters 19 and 21–25, with chapter 50 on individuality. Use the user's actual setting and feedback rather than the books' demographic stereotypes.

## Decide the job before searching

Establish the brief, source scope, time available, role in the night and intended listening experience. Use reasonable assumptions for missing optional details; don't delay a live set to interrogate the user. An empty room at opening time, a café, an established dancefloor and a recorded feature set can reasonably want different music.

Give the next choice a job: establish trust, continue a groove, deepen it, lift intensity, create breathing room, introduce contrast, or resolve the set. Name a musical connection such as a complementary rhythmic feel, shared timbre, a familiar motif, or an understood lyrical relationship. A deliberate contrast can be the point.

Use tempo/key/style to find feasible candidates after deciding the direction. Do not let “easiest to mix” become the entire selection criterion. If the best musical choice needs another transition technique, consider it within the brief. A requested order remains the user's choice.

## Distinguish the dimensions of energy

| Dimension | What changes it | Frequent confusion |
|---|---|---|
| Tempo | Speed of the perceived pulse | Faster automatically means more intense |
| Drive | Persistence and propulsion of rhythm/bass | A high whole-track RMS proves this intro drives |
| Intensity | Density, impact, attack, register and arrangement | Increasing master gain creates a better peak |
| Tension | Anticipation, withholding, repetition, instability | A quiet breakdown has no energy |
| Emotional color | Melody, timbre, harmony, lyrics and associations | Minor is always sad, major always happy |
| Familiarity | Actual listener recognition and memory | A popular title guarantees this audience's response |

Describe the dimensions that matter for this choice. Library energy values are measurement/ranking heuristics. They cannot establish the emotion or dramatic function of a selected passage.

## Prepare options without freezing the set

Work from the requested crate or a manageable shortlist. Keep useful alternatives for different directions rather than many nearly identical scores. Have a viable next entry ready before spending the remaining runway exploring. Broader preparation can include loose mini-sequences, dependable bridges and closing options; it need not become a fixed running order.

Morse's advice to pack roughly twice the needed music is a practical way to retain options without overwhelming a human. For an agent, preserve the principle of bounded, purposeful choice. It is not a mandatory track-count multiplier or a reason to exclude the rest of an authorized collection.

Keep a short branching direction tied to musical dimensions: for example, two warm rolling grooves → lighter body with pulse intact → fuller return, with a relaxed alternative if the brief calls for it. Decide where B could lead from its actual later structure. This is a palette and expectation, not a fixed track order or a demand for continual peaks. Do not repeatedly play a familiar pair just because it once worked. Honor an explicitly choreographed or fixed-order request; adaptability is a default for open selection, not a reason to override the user.

Keep a set-local history of actually audible titles, artists and exact release URIs, including recovery plays. Retain that compact history across context handoffs with the original start/finish, current carrier, pending operation and failures/gaps; a raw status snapshot must not overwrite the only played sequence. A prepared or failed source is not played. Check that history before committing a successor; honor earlier-set exclusions when a fresh mix was requested, and watch alternate releases of the same song. Start with unplayed candidates from the authorized collection, not just a handful of familiar analyzed tracks. Analysis availability is a preparation advantage, not a musical reason to keep repeating them.

If provider loading fails, preserve the carrier and secure a ready fallback before investigating. An emergency repeat can be preferable to silence within an authorized continuing set; label it honestly and do not confuse it with a planned motif. Account consent is a separate decision and should not consume the entire handover window. Do not keep alternating two tracks while claiming continued repertoire variety. Report the limited playable pool, and resume unplayed choices when loading works. A post-sign-in playback-key rejection is not proof that another login is required; follow the library/control error evidence and any cooldown.

## Shape longer sets at several scales

Within a track, protect its contribution and choose its exit. Across a few tracks, establish, develop and vary a musical idea. Across the whole set, give the requested arc room to unfold. A two-hour set usually needs more variation and recovery than a short showcase, but no universal curve or fixed reset interval applies.

For warmup, make involvement inviting without spending every major payoff immediately. For peak time, sustain conviction with purposeful contrast and breathing room. For background listening, maintain a welcoming character and unobtrusive level; absence of cheering is not failure. A closing can soothe, celebrate or finish decisively according to the brief.

Do not raise tempo on every handover by habit. Avoid consecutive long breakdowns when drive is wanted, but don't erase every breakdown either. A reduction has value when it prepares a satisfying return. Repeated instant chorus-to-chorus switches can exhaust attention; repeated full-length outro-to-intro joins can drain it. Vary the placement and texture because the songs and set need it.

A big drop followed immediately by another withdrawal may fail to sustain a requested lift. Inspect B after the dramatic instant. The next record should work now and leave a useful future.

## React to observed feedback

Use actual audience information or the user's listening reports. Prefer changes supported by patterns over overreacting to one ambiguous moment. People entering/leaving can have reasons besides the music; no crowd feed means no claims about dancing, fatigue or recognition.

Translate criticism into a question: “thin” suggests loss of foundation/body; “busy” suggests competing parts; “boring” may concern selection, repetition or pacing; “abrupt” may concern incomplete form, level or timbral contrast. Do not answer every criticism with more BPM or effects.

If a choice disappoints, move to a complete useful exit calmly rather than chopping it at an arbitrary second. Preserve a viable successor. At the end, use an intentional final statement and respect the requested finish time; source-duration sums ignore overlaps, rate changes and interruptions.

Learn from listening after the performance where a recording already exists or recording was authorized. Separate reproducible technical faults from context-dependent taste. A successful pair is a useful option, not a universal rule.


## Compose the finish against real remaining music

A fixed endpoint is the latest allowed playback time, not a closing arrangement. Once choosing the final passage, compare its available played duration with the remaining set duration, including any planned tempo movement, loop repetitions and fade. At a stable rate, `remaining_music_wall_seconds = (audible_end_source_seconds − current_source_position) / actual_playback_rate`. Use the measured audible bound when available; file EOF can include silence. Reserve the final complete statement and its exit as part of the musical plan.

In one observed set, the closing source had about 144 seconds of material left while the session had about 161 seconds remaining. The track exhausted early, leaving roughly16seconds with no carrier. A successful endpoint timer did not repair that difference. Choose a sufficiently long compatible closing passage, arrange a seam-checked extension and release, or introduce a worthwhile final musical paragraph while preparation time remains. A short fragment thrown in at EOF is a recovery, not an authored close.

A planned quiet ending can be right when it is the intended ending. Describe it honestly. A `successor_due` warning near the endpoint is resolved by comparing both clocks, not by assuming “the set ends soon.” After completion, check actual pre-endpoint route history before classifying an interruption as the normal final stop.

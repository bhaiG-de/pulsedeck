# Native launch fix, blind set retest and cache persistence

15 October 2026 (Asia/Kolkata). The final blind Luna trial completed approximately **15:02** with four liked songs, three completed handovers, **zero observed transport gaps**, no failed queued actions and no reported audio callback overruns. This establishes continuity for this run, not consistently correct musical phrasing.

## Native launch defect and fix

The earlier abort was at No Fun source **375.579s**, between published beats; the nearby next beat was **375.739105s**. The command queue accepted an explicit `PlayQuantized` action with that outer source-time trigger. Unlike plain `Play`, it did not resolve the trigger onto the grid. At dispatch it replaced the intended event with the current renderer position. The lower renderer then rejected the off-grid launch and feedback reported `launch_aborted`, late in the outgoing outro.

Changed `crates/deck-engine/src/scheduled.rs`, `scheduled_runtime.rs` and `dsp.rs`:

- Resolve both plain and explicit queued quantized launches forward onto the selected clock's effective grid. Reject a conflicting launch/outer timing clock before registration.
- Preserve the resolved event through dispatch and grouped/dependent execution. Untimed explicit launches retain their own native timing.
- Allow the bounded two-sample crossing needed to execute a scheduled event on the first renderer sample at/after it. Ordinary late standalone launches still reject.
- Avoid advancing an exact event a whole beat because grid conversion round-trips a few floating-point ulps above an integer.

Regressions cover both deck directions, explicit/plain launches, normal/double playback rate, dependent controls, parallel groups and clock mismatch. The off-grid regression failed before the fix and passes afterward. Existing source, takeover, deadline and allocation tests remain passing. The app was rebuilt and restarted before live testing.

## Blind protocol and intermediate failure

Every fresh `gpt-6-luna` agent received only:

> Play a 15-minute DJ set using my Spotify liked songs.

The final two trials used medium reasoning. No parent history, coaching, follow-up prompt, timing hint or parent deck operation was supplied during a trial. Observation sampled public deck state approximately every 0.5 seconds and retained native jobs/events and source metadata. No music recording was made.

The first retest with the fixed engine still failed continuity. Luna played a short opener, researched later selections, then submitted its successor load after EOF with an already-passed launch time. The native launch fix could not solve that planning failure. Once the gap-free condition was falsified, the parent interrupted the trial and stopped its owned automation; those cleanup actions were outside the observed performance.

The skills were revised before the next fresh trial: register the successor's isolated load alongside the opener's load/play chain; inspect and arm the active pair before expanding the set plan; immediately prepare the released deck; and refresh the actual clock before submitting an absolute target. Library browsing now uses a bounded shortlist for a live set rather than repeated collection-wide scans.

Intermediate evidence: [failed retest directory](../artifacts/skill-trials/luna-native-launch-fixed-20261014T230752Z/).

## Completed continuity trial

Final evidence: [trial directory](../artifacts/skill-trials/luna-native-launch-fixed-20261014T231816Z/), including frozen skills, binary hash, `snapshots.jsonl`, events, loaded metadata, `job-55459-44-final.json`, `assessment.json` and `audible-overlap-phase.json`. The skills remained unchanged throughout this trial. All four requested source IDs were present in the saved liked-song response.

| Stage | Observed execution |
|---|---|
| Opener | Calvin Harris, Clementine Douglas — Blessings. The initial append also isolated/loaded B. |
| First handover | Lola's Theme - Tripolism Extended Remix started on Blessings **206.801854s**, from **108.473127s**. The 8-second outgoing ramp completed before A was paused at **214.801854s**. |
| Released deck | No Fun loaded on A while Lola's Theme continued. |
| Second handover | No Fun started muted on Lola **339.763210s**, from **46.223810s**. The audible exchange began at Lola's **386.381622s** outro and completed at **392.381627s**, before B paused. |
| Released deck | It's Over Now - Extended Vocal Mix loaded on B while No Fun continued. |
| Third handover | Luna revised the pending final launch once, without replaying completed work. The replacement executed; a 6-second exchange completed with No Fun at **317.411043s** and It's Over Now at **179.245216s**, before A paused. |
| Close | It's Over Now reached its full **377.768526s** EOF. Both decks stopped; Luna closed the session. |

The first play receipt is `1792019958126` Unix milliseconds. Observer renderer frames bound actual time to final EOF at **901.793–902.293 seconds**, approximately **15:02**. Luna reported 15:07 after noticing the ending; that later observation time is not the audio duration.

Across first audible playback through the final live sample, there were **zero samples without a playing, open contributing deck**. Native receipts show the three launches and their linked ramp/pause stages completed. There were no failed queued actions. One cancelled `start-final` receipt is an intentional pending replacement; `start-final@2` executed. Closing the session leaves its overall job cancelled even though its performance actions completed. Audio counters reported zero callback deadline exceedances and zero invalid signal frames.

Half-second observation cannot rule out every sub-sample acoustic artifact. This was a transport/mixer continuity check supported by native receipts and meters, not an audio listening assessment.

## Musical quality remains a separate limit

Published-grid phase in the audible overlaps still exposes weaknesses. The first unequal-tempo fade drifted through about 0.8 beat of bar phase. The second had roughly 0.33–0.39 beat error during audibility. The third held small fractional beat error but about one beat of bar offset. Its selected launch was an inspected beat, not a verified matching downbeat. The second also used a muted lead with unequal published beat counts.

Thus this run passes the requested no-gap/native-launch check. It does **not** establish reliable bar/phrase matching, vocal clarity or a musically seamless set. The retained phase evidence should inform further musical evaluation without confusing it with the fixed abort.

## Separate cache fix and restart verification

At the user's request, a separate agent investigated persistence during the set without changing the live app/profile. Existing analyses were still in SQLite. It reproduced a real restart-specific invalidation: section dependencies contain exact BPM/grid floating-point bits, and JSON parsing without accurate float round-trip support could change one bit on hydration, invalidating current cached sections.

`serde_json` now explicitly enables `float_roundtrip` in both the engine and desktop app. Fresh-child-process regressions verify durable local/Spotify timing, stage stamps and manual anchors, and that all six analysis-stage computation counters remain zero on an unchanged persisted cache hit. No cache deletion, migration or loss of saved cues was required. Spotify progress now exposes reused cached counts and distinguishes saved analysis from playback preparation.

After the DJ agent completed, the app was rebuilt/restarted with this fix. All four real set tracks' saved BPM/key summaries were identical before/after restart. A guarded, isolated load of the already-analysed It's Over Now returned **`content_match`**, preserved every analysis stage stamp, and remained paused. Native diagnostics reported about **0.011ms base analysis**, **0.165ms neural handling**, **4.29s content fingerprinting** and **12.88s provider preparation**. The latter work prepares/verifies playback audio; it is not fresh beat/key/section inference. Spotify audio is not durably stored by this metadata cache. Evidence is in `cache-*-restart.json`, `cache-postload-*` and `cache-restart-assessment.json` in the final trial directory.

## Checks

- Integrated engine suite: **163 passed, 1 ignored**. A separate model integration test remains ignored without its explicit installed-model test setup.
- Cache-focused engine suite: **10 passed**.
- Desktop library suite: **17 passed, 1 ignored**; the ignored test requires a supplied real-library copy.
- Skill validation: exactly three packages, links resolved, **26 examples parsed**; **7 packaging tests passed**.
- Frontend build and packaged debug app build passed. Targeted whitespace checks passed.

The final running app includes the launch and persistence fixes. The DJ trial used the launch fix and frozen updated skills; the persistence change was deployed only after that trial finished.

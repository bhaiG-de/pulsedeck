# Beatmatching and groove

Owns the pulse relationship during overlap. Phrase selection belongs to [phrasing and arrangement](phrasing-and-arrangement.md); applying tempo/Sync belongs to the control skill.

Separate the speed of the pulse from its phase. If alignment stays offset, correct phase. If the error grows, investigate local tempo, pulse interpretation or the grid. A nudge can fix an offset but does not cure a persistent tempo mismatch. Verify the intended clock before layering two rhythmic passages.

Preserve the live groove unless the set calls for tempo movement. When a common tempo works, set the incoming playback rate rather than rewriting the track's analyzed BPM. Tempo drift, swing, human timing and groove differences can make a long blend unsuitable even if average BPM matches. The control workflow determines how PulseDeck's installed Sync and tempo controls maintain alignment.

Half/double relationships can support some transitions, but 87→174 is not a complete arrangement plan. Identify the perceived pulse, where accents land, and how many actual beats separate entry and arrival. Explain any pulse reinterpretation rather than silently redefining bars.

There is no universal percentage that guarantees good stretching. Consider transients, voices, sustained notes and key-lock artifacts at the intended rate. Prefer a better-fitting successor when the required processing harms the material. For a large tempo change, a deliberate section cut, beatless bridge or other prepared transition can be preferable to continuous kick layering. Use only capabilities the installed system supports.

## Verify five timing scales separately

| Scale | What passes | What it cannot establish |
|---|---|---|
| Tempo | Pulses progress at compatible local rates | That the beats coincide |
| Beat phase | Corresponding pulses coincide | That beat1 meets beat1 |
| Bar phase | Matching metrical positions coincide | That complete phrases meet |
| Phrase progression | Chosen arrivals have equal remaining beat intervals | That their content belongs together |
| Arrangement | Complementary content develops through and after the overlap | That it sounds balanced through the mixer |

Sync addresses the supported clock relationship. It does not know that A owes the listener another vocal answer or that B’s intro contains an extra bar. Validate the musical grid origin rather than equating the first decoded onset with beat1.

For simultaneous native readings, small wrapped fractional phase can coexist with a one-beat bar error. Compare modulo meter and the countdown to the actual chosen arrivals. With credible 4/4 origins, a B–A difference of1.02 beats has a near-zero fractional error but the wrong bar relationship. Phase zero can also occur at different points in a longer phrase.

## Read the behavior of an error

| Observation | Likely question to resolve | Musical response |
|---|---|---|
| Fixed offset, no growth | Was the start or phase wrong? | Correct/relaunch while isolated when useful runway remains |
| Increasing offset | Do local tempos, Sync roles or grids disagree? | Fix the supported clock strategy or choose a more stable overlap |
| Pulse fits, snare/accent feels displaced | Is bar origin or pulse interpretation wrong? | Re-establish meter/beat identity, not another arbitrary nudge |
| Early alignment, later unexpected arrival | Did an extra bar, loop or seek change the lead? | Recount the remaining real intervals and revise the plan |
| Clicks line up but groove fights | Do swing, bass rhythm or busy transients conflict? | Choose complementary material or shorten the overlap |

A numerical observation proposes a cause; listening and actual source evidence establish it. Do not repeatedly change the live master to chase ambiguous follower readings.

## Prepare a sustainable blend

Use a suitable common playing tempo and credible effective events across the whole window. In PulseDeck, a one-time absolute BPM setting establishes entry intent; nonuniform clocks need the supported ongoing master/follower strategy for a sustained beatmix. Settle the incoming final entry and verify roles before launch. Do not flatten a real variable-tempo recording by saving a constant grid merely to make a blend convenient.

Keep musical and technical fallbacks separate. If one passage is unreliable, inspect a steadier complete passage or choose another compatible source early. If the brief asks for a smooth blend, a checker-friendly cut is not the first alternative. If a genuine failure destroys the runway, a recovery exchange may preserve music but should be identified honestly.

[Native Instruments’ beatmatching lesson](https://blog.native-instruments.com/beatmatching/) treats tempo adjustment, phase correction and checking the incoming signal as distinct work. PulseDeck-specific Sync behavior comes from the installed control contract, rather than assumptions about another product’s Sync button. The five-scale review and diagnosis table are an agent reasoning framework synthesized from these distinctions.

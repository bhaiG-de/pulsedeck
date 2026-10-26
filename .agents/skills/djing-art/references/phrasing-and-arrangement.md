# Phrase placement and musical form

Read this when choosing where to mix, counting an entry, or diagnosing a join that is on beat but feels misplaced. Book basis: Morse, “Beatmixing Part 1: Timing” and “The Three Elements”; Broughton/Brewster, chapters 10 and 13. See [sources](sources.md) for locators and qualifications.

## Count the right thing

- **Pulse / beat:** the reference tick you count, including beats where no instrument sounds.
- **Bar / measure:** a metrical group of beats. In 4/4, count four quarter-note beats. Establish the actual meter; a first audible kick can fall after beat one.
- **Phrase:** a musical unit with a beginning, continuation and some sense of completion. DJs often use the word for repeated groups of bars. Four, eight, sixteen and thirty-two bars are useful search lengths in much dance music, not proof of a boundary.
- **Section:** a larger arrangement function, such as a verse, refrain, breakdown or outro. It can contain multiple phrases and smaller changes.

Count bars as “1-2-3-4, 2-2-3-4, 3-2-3-4…” while preserving which larger unit you are in. A 16-bar interval in 4/4 is 64 beat intervals. Do not call 16 beats sixteen bars. A fill often announces the next boundary; starting on the fill can be one bar too early.

Distinguish total phrase length from remaining length. At the **start** of bar k inside an N-bar phrase, `remaining_bars = N − (k − 1)`. For example, at the start of bar 5 in a 16-bar phrase, four bars have elapsed and twelve remain. The next phrase begins at the original phrase origin plus N bars, not at “now plus N.” Resolve whether a number is an absolute source bar, a position within the phrase, or an interval before adding/subtracting it. Cross-check that elapsed plus remaining equals the original length.

Use a **local phrase origin** supported by the music. File zero, an arbitrary multiple of 32 beats, a detector's downbeat, and the first acoustic onset can all differ from it. Beatless intros, pickups, rests, edits and extra bars are ordinary reasons to investigate rather than force the count.

## Build a small passage map

Map the current statement, the next useful exits, and B's entry through its arrival and first following phrase. For each relevant point retain source time / effective beat position, what enters or leaves, and how you know. Useful content includes bass entry/withdrawal, lead/vocal onset and completion, fills, tension, and the return of a full groove.

Evidence for a boundary combines context: a repeated pattern completes, a fill resolves, a lyric ends, harmony settles, or a new element enters and remains. A crash alone could be an accent inside a phrase. Inspect before and after. With band evidence, infer the functional relationship: a recurring foundation, sustained withdrawal, developing activity or strong return. Neighboring and repeated patterns support candidate phrase boundaries. Keep the semantic label provisional, but use the measured function to place and shape the mix; a verified section name is not a prerequisite.

## Choose the outgoing statement

Decide why A was played and what would be lost by leaving now. A familiar hook may need its full answer; an instrumental may need room to develop. A buildup promises an arrival: normally let that arrival and a worthwhile portion of its groove happen. A deliberate replacement can instead supply a new payoff, but a sparse unrelated intro usually frustrates the expectation.

Compare useful earlier and later exits. Mixing before a long breakdown can maintain drive; allowing it to unfold can create a needed release. Waiting for the final percussion outro can create a tedious gap between the songs' main ideas. There is no universal playback percentage or mandatory whole-song duration.

## Align the desired changes, not just the starts

Pick B's target first: a bass entry, a developed groove, a vocal opening, or a drop. Then choose a complete B entry that leads to it. Place that target at the useful change in A. With the same metrical pulse and a common advancing beat rate:

```text
B lead = B arrival beat position − B entry beat position
A launch = A handover beat position − B lead
```

Check that A's resulting launch and B's entry occupy the intended bar positions AND compatible local phrase positions. Beat positions count intervals: endpoints at 0 and 64 span 64 beats even if the event list includes 65 events. Read the entire interval when the grid is nonuniform.

Example, using one-based source bars whose numbers denote their first beats: A's last refrain begins at bar 65 and finishes at the start of bar 81. B's 16-bar intro leads from bar 1 to its main groove at bar 17. Start B1 at A65 so B17 arrives at A81. Starting B1 at A81 puts two thin intros/outros together and delays the new main idea by sixteen bars. Both launches could be perfectly beatmatched; they create different pacing.

Now suppose B's selected entry leads to its arrival in **24 bars** and A offers **32 bars**. Starting both region beginnings puts B's main idea eight bars early. Options include launching B eight bars later on an evidenced subphrase, choosing a later complete B entry with a shorter lead, moving the handover, or intentionally extending a clean eight-bar B passage. Recheck all internal changes; do not stretch “24” into “32” by relabeling it.

## Include what happens between the anchors

Matching endpoints can still hide a collision. B's singer might begin eight bars before its target; A's hook might return halfway through its apparent outro. Keep these events on the shared timeline. [Mixing and blending](mixing-and-blending.md) determines which parts should be audible then.

At constant playing BPM, an N-beat lead takes `N × 60 / BPM` wall seconds. At constant playback rate r, a source interval takes `source_seconds / r`. Those are different coordinate systems. Variable tempo, live Sync adaptation, loops and seeks require actual event progression and the control contract; a whole-track mean BPM is only an estimate.

## Pickups, odd lengths and repeats

A pickup belongs to the phrase it introduces even when it precedes beat one. For a one-beat pickup, begin B one beat before its downbeat must meet A's target; decide when the audience should hear the pickup. Make room in A's closing line. Do not discard the word to obtain a downbeat-only launch.

A two-bar extension after sixteen bars makes an eighteen-bar lead. Count it. Select an offset entry/launch only if its content is complete, or use another passage/technique. An inaudible missed beat still advances the meter. Half/double BPM labels do not establish equal musical bars.

Loop repetitions change the source-to-arrival relationship. Know the loop's content, number of extra repetitions, and release. Do not keep using the original linear countdown after a loop or cue jump. The [timing helper](checked-phrase-plans.md) has a narrower scope than these musical possibilities.

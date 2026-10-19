# Research for the DJing art skill

Research date: 15 October 2026. Scope: musical decisions for an agent using PulseDeck, especially where and why to enter and leave a track. The resulting [DJing art skill](../.agents/skills/djing-art/SKILL.md) is now part of the [three consolidated skills](../.agents/skills/README.md).

The central design change is to make the agent choose **a relationship between two evolving passages**, then calculate its timing and execute it. BPM, key, section names and waveform energy supply evidence. They do not, by themselves, choose a good transition. Mathematics can place a selected event precisely; choosing which event deserves that placement requires an explicit musical objective.

## What the current skills already know

Before consolidation, the musicianship reference already distinguished beats, bars, phrases and sections, counted backward from an arrival, protected requested payoffs, and considered spectral balance. The transition skill already requested evidence and completion checks. These responsibilities now live in [DJing — the art](../.agents/skills/djing-art/SKILL.md) and [transition execution](../.agents/skills/pulsedeck-control/references/transitions.md). Adding another sentence saying “mix in phrase” would have repeated existing instructions.

The likely instruction-design weaknesses were the distribution of musical reasoning across seven entrypoints, the amount of software detail surrounding it, and the lack of enough concrete examples where a mathematically valid transition must be rejected. This is a repository review inference, not a demonstrated explanation of every previous performance failure. The old seven entrypoints and shared references contained roughly 23,000 whitespace-delimited words.

The new draft therefore puts the musical decision first, makes choosing to wait a valid action, and teaches candidate rejection with worked cases. It keeps exact command syntax outside the art skill.

## What the teaching and research sources establish

### Timing has several independent scales

Beatmatching aligns the local pulse and tempo. A phase correction fixes an offset; a tempo correction addresses accumulating drift. Preparation should include checking the selected passages while the incoming deck is isolated. Native Instruments teaches this distinction through pitch adjustment, tempo bend and relaunching before opening the incoming channel. [Native Instruments beatmatching lesson](https://blog.native-instruments.com/beatmatching/)

The Mixxx manual treats gain staging, beatmatching, Sync and transition markers as distinct tasks. This is useful software documentation for understanding those mechanisms; Mixxx's defaults and controls should not be assumed to describe PulseDeck. [Mixxx DJing manual](https://manual.mixxx.org/2.5/en/chapters/djing_with_mixxx.html)

For the agent, verify these separately:

| Scale | Question | Failure despite passing the previous scale |
|---|---|---|
| Local tempo | Do pulses continue at compatible rates? | Equal whole-track BPM can hide local drift |
| Beat phase | Do the intended beats coincide? | Matching fractional beats can put the snare on the wrong beat |
| Bar phase | Do corresponding beats within the bar coincide? | Two aligned bars may sit in different parts of a phrase |
| Phrase progression | Do the intended musical units reach their boundaries together? | Phrase boundaries can still introduce incompatible content |
| Arrangement and purpose | Does the incoming content belong at this outgoing moment? | A perfectly timed new breakdown may remove the drive the set needs |

The final two rows are where the art skill must do more work. Common 8, 16 and 32 bar units are useful search hypotheses. They are not a license to manufacture boundaries from file zero.

### A phrase is audible organization

Crossfader teaches phrase recognition through recurring changes, entrances, withdrawals, fills, groove and available space. Its listening lesson is particularly relevant because it explains how to anticipate changes rather than merely count. [Crossfader on listening](https://wearecrossfader.co.uk/blog/learning-to-listen-like-a-dj/)

For an agent, translate this into a passage map: where an idea starts, how it develops, what signals its ending, and what follows. A section label describes a broad role; internal phrases may offer several useful entrances or exits. A large waveform change may mark an important event, but its musical role needs context before and after it.

Keep bar numbering and beat indexing explicit. In 4/4, 8 bars contain 32 beats; 32 bars contain 128 beats. Tutorials sometimes use “phrase” for different hierarchical units. The draft specifies actual counts instead of relying on the word alone.

### The correct exit depends on what the track needs to say

Phil Morse's transition lesson considers the event, audience, pacing, track response and whether the song has delivered its defining material. It supports leaving at suitable structural boundaries, including well before a file's outro, rather than applying a fixed playback percentage. [Digital DJ Tips on where to transition](https://www.digitaldjtips.com/how-pro-djs-know-where-to-transition-plus-a-big-cheat/)

Operational interpretation: identify what should be protected before looking for an exit. That might be a sung line and answer, a chorus, a developing groove, or a buildup and its payoff. “The drop started” is insufficient evidence that the payoff has had room to land. Conversely, playing every repeated section is not intrinsically better. A requested quick set can use an earlier complete musical unit.

### There is more than one transition timestamp

Zehren, Alunno and Bientinesi distinguish first audibility, incoming dominance and outgoing disappearance. Their study identifies candidate structural switches using novelty, metrical position and the salience of what follows. Crucially, it studies candidate points within tracks; it does not establish that an arbitrary pair works together. Its EDM and seamless-transition scope limits generalization. [Automatic Detection of Cue Points for DJ Mixing](https://arxiv.org/html/2007.08411)

Adopt precise terms in plans:

- **Entry:** the incoming source position used when playback starts, possibly in headphones only.
- **Audible entrance:** when the incoming contribution reaches the audience.
- **Handover:** when the incoming musical idea becomes the foreground.
- **Clearance:** when the outgoing contribution ends. Its vocal or lead may need to clear earlier than its supporting percussion.

The bass exchange can coincide with the handover or precede it. Do not infer musical dominance from a 50% crossfader position.

### Real mixes support phrase structure without establishing one formula

Kim and colleagues' computational analysis covers 1,557 real-world mixes. Extracted transition lengths cluster around multiples of 32 beats, while cue choices also vary between DJs. The results describe patterns in a particular corpus and extraction method. They do not prove that every transition should last 8 bars, or that there is one correct cue for a track. [Real-world DJ mix analysis](https://arxiv.org/html/2008.10267)

CUE-DETR studies cue estimation using 4,710 EDM tracks and 21,461 cue annotations. Its dataset uses constant tempo and 4/4 and excludes outliers. The authors discuss subjectivity, incomplete annotations, preludes and irregular structures. A phrase-aware candidate model is a relevant future research direction, but its published results do not validate PulseDeck's detector or complete mixes. [Cue Point Estimation using Object Detection](https://arxiv.org/html/2407.06823)

### Harmonic mixing concerns the passages as played

Mixed In Key's basic Camelot moves are same cell, adjacent number with the same letter, or the same number with the opposite letter. Thus 8A suggests 8A, 7A, 9A and 8B; number adjacency wraps between 12 and 1. These are shortlist relationships, with audition of the overlapping passages still required. [Official Camelot workflow](https://mixedinkey.com/workflows/how-to-use-the-camelot-wheel/)

Native Instruments emphasizes when tonal compatibility matters most and when percussive material gives more freedom. Its Open Key notation is a different numbering system. Do not compare Open Key and Camelot numbers directly. [Native Instruments harmonic mixing lesson](https://blog.native-instruments.com/harmonic-mixing-rules-and-how-to-break-them/)

The agent should distinguish three things: a track-level key estimate, the actual harmony in the overlap, and pitch after playback processing. Two same-key lead vocals can still compete. Relative major/minor does not require identical chord progressions. Percussive material can include pitched kicks or resonant tones. Missing key evidence means unknown compatibility.

For playback without key lock, the rate-induced shift is approximately `12 × log2(playback_rate)` semitones, before any separate pitch shift. This is a frequency-ratio calculation, not a Camelot-number increment. With key lock, assess processing quality and any explicit pitch offset. Do not impose a universal stretch limit as an artistic law.

### Energy is not a BPM ramp

Laidback Luke's lesson contrasts tracks at different tempos to show how arrangement density changes perceived intensity. Use its distinction between speed and intensity; waveform thickness alone is not a reliable cross-track energy measurement because mastering and display scaling also affect it. [Energy versus tempo lesson](https://www.digitaldjtips.com/energy-vs-tempo-luke-lesson/)

Mixed In Key describes its energy score as a preparation aid and explicitly advises comparing the intended intro, breakdown and drop. Its track score does not predict every section or audience response. [Official energy workflow](https://mixedinkey.com/workflows/use-energy-level-detection/)

For planning, maintain separate qualitative observations rather than collapsing them into one number:

| Dimension | What changes | Why it matters |
|---|---|---|
| Tempo | Pulse speed | Affects movement and timing |
| Drive | Rhythmic propulsion and continuity | Determines whether an exit leaves the dance pulse unsupported |
| Intensity | Density, attack, brightness and weight | Can rise at a constant BPM |
| Tension | Expectation of an arrival or resolution | Can increase while drums or loudness decrease |
| Mood | Emotional color and atmosphere | Can change without increasing intensity |
| Recognition | Familiarity for this audience | Requires actual audience knowledge or feedback |

This table is a design model synthesized for the agent, not a claim that PulseDeck measures these dimensions separately.

### Set flow needs contrast and context

Pioneer DJ's direct interviews with Saoirse, Laurent Garnier, Jane Fitz and Dusky discuss pacing a whole night, preparation and the need to take listeners through different moments. That supports planning beyond the immediate pair and reserving headroom for later peaks. [Artist accounts of all-night DJing](https://blog.pioneerdj.com/djtips/how-to-dj-all-night-long/)

Use a short horizon: what should this transition accomplish, what should the next few tracks do, and where is the next meaningful release? Holding an effective groove can be the right decision. A recorded mix, warmup, mainstage set and background session have different pacing goals. An agent without audience input should follow the brief and user feedback, rather than claiming to read a room it cannot observe.

### Balance is an arrangement decision before it is an EQ move

Digital DJ Tips' five-transition lesson explains why an ordinary overlap can either stack basslines or leave a gap before the incoming bass appears. Its bassline swap addresses that specific musical relationship. [Five basic transitions](https://www.digitaldjtips.com/rock-the-dancefloor/five-basic-dj-transitions/)

Crossfader's house demonstration assigns different frequency roles to a vocal loop and supporting rhythm. It illustrates purposeful layering rather than two full tracks remaining equally prominent. Its frequency descriptions are teaching simplifications, not specifications for PulseDeck's filters. [House mixing demonstration](https://wearecrossfader.co.uk/blog/house-mixing-tutorial/)

In the draft, each phase has a foreground idea and supporting roles. Transfer low-end weight when replacement weight exists. Clear the outgoing vocal before the incoming vocal needs attention. Adjust crowded highs or mids proportionately. Broad EQ is not instrument separation. Equal fader positions do not imply equal loudness; a limiter catching the overlap does not prove balance. Preserve natural musical contrast while avoiding accidental holes caused by mixer actions.

### Technique follows the arrangement

Pioneer DJ's genre survey describes materially different practices: long textural layering, melody-conscious blends, cuts, turntablism and fast exchanges. These are broad tendencies, not fixed genre rules. [Genre approaches](https://blog.pioneerdj.com/djtips/we-uncover-the-mixing-techniques-behind-every-major-genre/)

Crossfader's EDM lesson distinguishes a planned drop swap from ordinary blending, with the replacement payoff and the outgoing material's removal coordinated. It also warns about crowd fatigue from unrelieved peak material. [EDM transition lesson](https://wearecrossfader.co.uk/blog/how-to-mix-edm/)

Its drum and bass guide highlights the interaction of bass, busy percussion, phrasing and EQ. Matching fast BPMs alone does not establish that two dense drops will layer successfully. [Drum and bass guide](https://wearecrossfader.co.uk/blog/howtomixdnb/)

The resulting choice is conditional: use a long blend when complementary material can develop together; a short exchange when ideas need separate space; a breakdown bridge when a release serves the set; or a prepared payoff replacement when surprise is intended. A double drop needs complementary arrangements and preparation. It is not a mandatory upgrade from a clean bass exchange.

Ean Golden's discussion of adding value argues for restraint and purposeful interventions. Some harmonic “energy boost” moves in tutorials are creative recipes, not guaranteed perceptual results; the draft does not turn them into energy equations. [Value-added mixing](https://djtechtools.com/amp/2013/04/30/value-added-mixing-when-to-dj-more-and-when-to-do-less/)

### Cues remember musical knowledge

Native Instruments describes marking defining moments, vocal entrances, alternative starts and usable loops. This suggests storing cues with a reason and context, rather than treating a fixed pad number as a universal mix command. [Cue preparation lesson](https://blog.native-instruments.com/cue-points-djing/)

Lexicon's manual states that automatic cue detection depends on genre and correct BPM and can require manual correction. Imported or generated points should retain provenance. [Lexicon cue generator manual](https://www.lexicondj.com/manual/cue-point-generator)

DJ TechTools documents Sync/grid problems around fluctuating timing and extra beats. For the art skill, the transferable lesson is to preserve real musical timing and inspect exceptions. Its historical Serato-specific repair instructions should not be transplanted into PulseDeck. [Beatgrids and Sync](https://djtechtools.com/amp/2014/04/23/how-to-set-beatgrids-and-use-sync-properly-in-serato-dj/)

## A decision procedure for choosing the actual mix points

The following is the proposed agent policy synthesized from the sources. It is not a published universal DJ method.

1. **Set the purpose.** Hold the groove, lift intensity, build anticipation, release tension, change direction, or finish. Honor the user's selected order and pacing.
2. **Identify the outgoing promise.** What line, hook, groove development or payoff should finish before intervention? Mark the earliest appropriate intervention and the latest useful exit.
3. **Find plausible handovers.** Use a few upcoming musical changes with enough preparation time. Compare what follows each, not just its timestamp. A sparse outgoing section is useful only if the next material can supply the intended result.
4. **Choose the incoming arrival.** Locate the groove, lyric, motif, chorus or drop that should own the handover. Confirm it can carry the moment for the intended time; a momentary impact followed immediately by another breakdown may be wrong.
5. **Work backward to an entry and launch.** Count actual beats from the incoming entry to that arrival, preserve pickups, then locate the corresponding outgoing launch. Compute seconds only after the musical relationship is chosen.
6. **Inspect the complete overlap.** Track when bass, leads, vocals and percussion enter and leave on both sources. Reject candidates with competing foregrounds, unintended empty spans, unusable tempo changes, or a launch that has already passed.
7. **Choose the simplest fitting execution.** Specify audible entrance, role transfers, clearance and fallback. If the evidence supports a clean short exchange better than a blend, use it. If the current passage should develop, wait and prepare.
8. **Assess the musical result.** Timing receipts establish execution. Listening or user feedback is needed to assess continuity, intelligibility, timbre and impact. Preserve the useful observations for the next choice.

Do not score this with arbitrary weights pretending to measure taste. Reject hard conflicts first; then compare the remaining candidates against the stated musical purpose. Keep alternatives when tradeoffs are real.

## Worked timing example

Assume confirmed 4/4, steady grids, A at 124 BPM and B at 128 BPM. Bars are numbered from one, with bar 1 starting at source time zero. These are invented teaching tracks, not analyzed library recordings.

A's first payoff occupies bars 81–96; its following groove occupies 97–112; a breakdown starts at 113. The brief asks to hear that first payoff and maintain drive afterward. B has supporting drums at 9–16, a bass groove at 17–24, then its main hook at 25.

Choose B's hook as the foreground at A113. B9→B25 is 16 bars or 64 beats, so launch B9 at A97. At 124 BPM that lead is `64 × 60 / 124 = 30.9677` seconds. B9 is 15 seconds into its original 128 BPM file; B25 is 45 seconds. At rate `124/128 = 0.96875`, those 30 source seconds take 30.9677 wall seconds.

| Outgoing location | Incoming location | Proposed musical action |
|---|---|---|
| A81–96 | Prepared, isolated | Let A's first payoff develop |
| A97 | B9 | Launch and introduce supporting drums as verified |
| A105 | B17 | Transfer bass as B's groove arrives |
| A105–112 | B17–24 | Keep a single foreground; clear competing A material |
| A113 | B25 | Reveal B's hook and clear A before its breakdown |

The choice of A113 comes from the desired continuity of drive. The subtraction only places it. If B's hook conflicts with a late A vocal, the timings remain correct but the plan must change. If B's intro actually lasts 18 bars, rounding it down to 16 is an error; use a different complete entry or a different handover.

## How PulseDeck evidence should inform the art

The following observations come from the current source tree, not online tutorials:

- [Section analysis](../crates/audio-analysis/src/sections.rs) segments bar features with a preference for common lengths. Its `energy` and `groove` are relative features; `rise` is a level-change feature and `group` represents similarity. None identifies lyrics, emotional meaning or audience response.
- [Canonical cues](../crates/audio-analysis/src/cues.rs) carry source times, optional endpoints, origin, edits, confirmation and analysis anchors. Use individual point provenance; a name alone is insufficient. Confirmation records metadata status and is not proof of independently heard content.
- [Track inspection](../src-tauri/src/track_inspection.rs) returns named spans, grid freshness, actual bar times, bounded beat events and waveform bins. Account for truncated event responses. A section endpoint is more informative than reconstructing every region from an average BPM.
- [Suggestion scoring](../src-tauri/src/suggest.rs) combines key, tempo and a track-level waveform heuristic. Its energy score uses loudness and brightness; it does not supply a passage-specific tension curve. Treat the returned order as a shortlist, then compare the actual entry and exit passages.

This supports useful decisions without inventing observations. When only numerical evidence exists, describe candidate roles honestly, avoid assuming vocals are absent, and choose an overlap appropriate to that uncertainty. A more capable instruction cannot recover an unobserved lyric or validate a mislabeled drop by assertion.

## The three consolidated skills

| Skill | Owns | Material to consolidate |
|---|---|---|
| Manage Library | Sources, playlists, analysis, metadata, cue persistence, import/export | Library parts of inspection and set preparation; XML migration; metadata provenance |
| DJing — the art | Selection, timing intent, phrase and arrangement relationships, harmony, energy, role exchange and musical evaluation | Musical reasoning currently distributed through inspection, preparation, transition and session guidance |
| Controlling PulseDeck and workflows | State, routing, preparation, transport, Sync, native scheduling, guards, recording, recovery and stop | Prepare-next-track, transition execution, persistent session, troubleshooting and recording controls |

These are three discoverable capabilities with focused supporting references, not three enormous manuals. The art skill consumes observations and produces a musical plan. Control translates that plan into supported commands and returns execution evidence. Library preserves source knowledge. A live set needs art and control together; pure planning does not authorize playback.

## Musical validation after consolidation

The [worked cases](../.agents/skills/djing-art/references/worked-cases.md) cover regular and irregular phrasing, pickups, competing vocals, early bass arrivals, missing evidence, variable timing, tempo jumps and delayed preparation. They are a proposed evaluation set with expected musical constraints, not a claim of independent agent validation.

For musical validation of the consolidated skills, run unseen planning cases and audition representative real transitions. Judge whether the agent selects justified boundaries, protects requested material, catches internal overlap conflicts, and revises on missed deadlines. Check timing and musical quality separately. A valid skill file and correctly executed commands cannot establish that the mixes sound good.

# Deep research: teaching an agent to DJ from musical evidence

Research checked online on 15 October 2026. This extends [the initial research](djing-art-research.md) and the existing three skills. It is a research and instructional-design document, not a claim that a new skill has passed a listening test.

## The central correction

An agent can choose two authentic downbeats, schedule them accurately, and still produce a poor transition. The latest trial did exactly the wrong kind of thing for this user's brief: it replaced one deck abruptly with another. A mathematically valid phrase cut did not deliver the requested blend. That user feedback is decisive evidence about the requested result; it should not be argued away with the fact that cutting is legitimate in other DJ practices.

DJing combines **selection, arrangement, timing and sound shaping over time**. Teach the agent to construct a small arrangement from two passages: establish what continues, what is introduced, what becomes the foreground, what supplies the rhythmic foundation, and what disappears. Then place and execute that arrangement accurately.

There is direct research support for separating these tasks. Chen and colleagues distinguish cue selection from mixer control and compare different EQ/fader methods using identical cue boundaries. Their listening evaluation asks about preference, smoothness and conflicting musical sounds. Their small test does not establish a universal mixer recipe, but it demonstrates why cue correctness cannot stand in for the sound of the transition. [DJtransGAN paper](https://arxiv.org/html/2110.06525)

The operational policies and examples below are a synthesis for this agent. They are not presented as quotations or universal laws from any one DJ school.

## 1. Read musical evidence at the right level

The agent receives a changing stream of facts and estimates. The crucial skill is knowing which question each field can answer.

| Evidence | Useful question | Unsupported conclusion to avoid |
|---|---|---|
| Source ID, version and duration | Which recording do these observations describe? | Same title means identical intro, edit or cues |
| Beat positions, downbeats, meter and local intervals | Where are the pulse and bar boundaries in this passage? | Every downbeat starts a new musical phrase |
| Average BPM | Which successors are plausible to prepare? | These passages will stay aligned at the same displayed BPM |
| Effective playback rate, Sync mode and clock owner | How will source positions travel through wall time? | A lit Sync indicator guarantees phrase alignment |
| Section boundaries and roles | Where might texture, arrangement or intensity change? | A generated `drop` label proves the exact musical payoff |
| Named cues, endpoints and provenance | What reusable knowledge is available at this source location? | A cue named `mix out` works with every successor |
| Low/mid/high bands and peaks | Where does spectral weight change? | Low midrange proves there is no singer |
| Key label and confidence | Which tonal overlaps deserve investigation? | Same Camelot code guarantees compatible voices and chords |
| Live clocks, meters and action receipts | Did the intended preparation or action actually occur? | A job marked running means every scheduled step succeeded |
| Listening or explicit user feedback | Does the result sound balanced and coherent? | Machine logs alone establish a pleasing mix |

Crossfader's listening lesson emphasizes entrances, withdrawals, space, groove and structural change. Those are the observations an agent should seek or represent. When it has only numerical features, it must retain the distinction between an observed feature and a conjectured instrument or musical function. [Learning to listen like a DJ](https://wearecrossfader.co.uk/blog/learning-to-listen-like-a-dj/)

Use a **passage map**, not a verbose dump of a whole track. Record the important upcoming events, their source positions and beat positions, what each event changes, and how confident that interpretation is. A useful note is “low-band weight withdraws here; generated outro starts here; vocal status unknown.” “Clean instrumental outro” would overstate the same evidence.

## 2. Beatmatching, groove and Sync

Separate five tests: compatible pulse rate; aligned beat phase; aligned bar position; coordinated phrase progression; complementary arrangement. Passing an earlier test does not imply a later one.

Native Instruments' beatmatching lesson distinguishes adjusting tempo from adjusting phase. A persistent offset suggests a phase correction; increasing separation suggests a rate problem. For an agent, repeated tiny nudges are not a solution to uncorrected drift. [Native Instruments beatmatching lesson](https://blog.native-instruments.com/beatmatching/)

Serato documents a concrete distinction between tempo-only synchronization and synchronization that also locks position in the bar. This is evidence that “Sync” is not one universal behavior. PulseDeck's own supported mode and clock semantics must be inspected through its control skill. Neither mode chooses the musical phrase to align. [Serato Sync manual](https://support.serato.com/hc/en-us/articles/203056994-SYNC-with-Serato-DJ)

Variable tempo requires actual local anchors. Serato explains why live drums and tempo changes can defeat a single fixed grid; Algoriddim separately documents anchor points and setting a correct first bar. These manuals support checking the timing model rather than overriding a genuinely irregular performance to fit a convenient number. They do not validate PulseDeck's automatic grid. [Serato beatgrids](https://support.serato.com/hc/en-us/articles/202856014-Beatgrids-in-Serato-DJ-Pro), [Algoriddim beatgrid editor](https://help.algoriddim.com/user-manual/djay-android/dj-tools/beatgrids-bpm-sync/beatgrid-editor)

Groove is also more than metronomic alignment. Two perfectly aligned kick grids can carry different swing, backbeat emphasis and rhythmic density. Choose an overlap where those patterns complement each other; if they fight, revise the passages or successor. A half/double BPM relationship is a candidate connection, not proof that bars, snares or phrase lengths correspond.

For a steady-clock blend, the incoming interval and outgoing interval must traverse the intended number of beats together. At a changing clock, use the effective event mapping. Do not compare two source-second durations as if both sources ran at their original rates.

## 3. Choose phrases by musical function

A phrase is a coherent musical unit; a section may contain several phrases. Common dance arrangements offer repeated four-, eight-, sixteen- or thirty-two-bar relationships, but **the actual event has priority over the expected count**. Ableton's introductory composition material describes nested patterns and sections rather than a single mandatory phrase size. [Ableton song structures](https://learningmusic.ableton.com/song-structure/song-structure.html)

The cue-point research literature supports regularity with important limits. EDM-CUE contains 4,710 tracks and 21,461 expert cues, all selected for constant tempo and 4/4. Its authors find common eight- and sixteen-bar spacing, discuss irregular sections, and explicitly note that cue placement is subjective and annotations incomplete. This cannot justify a claim about every Spotify track or every genre. [CUE-DETR](https://arxiv.org/html/2407.06823)

Select the outgoing moment with four questions:

1. What worthwhile statement is this track making now?
2. What must finish before attention moves: a lyric and answer, a hook, a groove development, or the consequence of a buildup?
3. Which upcoming change gives the next song space without creating an unwanted hole?
4. Is there enough preparation and blend runway before that opportunity?

Phil Morse's transition lesson ties how long a song should play to context, audience response and whether its musical point has arrived. It also distinguishes useful patience from the DJ changing songs merely to be busy. Do not convert this into a fixed percentage of track duration. [Where professionals transition](https://www.digitaldjtips.com/how-pro-djs-know-where-to-transition-plus-a-big-cheat/)

Choose the incoming **arrival** before subtracting beats. The arrival is the groove, vocal, motif, chorus or drop that should take attention at the selected outgoing moment. Then find a coherent entry leading to it. The desired arrival need not be the first drop or the start of the file.

Inspect what follows the arrival, too. A convincing impact followed two bars later by a drum-free passage may be wrong for maintaining drive. Conversely, a restrained passage may be exactly right for a planned release. Good phrase selection involves the trajectory before and after the boundary.

### Pickups and irregular spans

A vocal or melodic pickup may begin before its strong downbeat. Preserve it when the complete line matters. If the incoming lyric starts one beat before its main arrival, the outgoing foreground must provide space by that earlier event. Aligning only the later downbeats does not prevent a vocal clash.

Count source-event intervals explicitly. An eighteen-bar approach in 4/4 contains 72 beats, not 64. Do not silently round it to sixteen bars. Consider another complete entry, another handover, a deliberately prepared loop, or a different track. A source may also contain an extra beat, a meter change, or an intentionally displaced accent. Missing evidence is a reason to inspect or select a safer candidate, not to invent a regular phrase.

DJ TechTools' phrase lesson is useful for identifying audible structural clues and for distinguishing beats, bars and phrase counters. Its fixed counter is an aid configured by a user; it should not be mistaken for a detector proving actual arrangement boundaries. [DJ TechTools phrasing lesson](https://djtechtools.com/2014/11/16/how-to-dj-101-why-you-must-understand-phrasing/)

## 4. Build an audible blend

For this user's ordinary continuous dance set, start by finding a feasible phrase blend. A plan with no meaningful period of audible contribution from both decks is a cut, even if both transports run silently for minutes. Anti-click smoothing over milliseconds is also not the musical transition requested here. This is a task-specific policy, not a statement that cuts are never artistic.

Crossfader demonstrates distinct results using the same two house tracks: a gradual blend, a more decisive EQ exchange, and a prepared layering transition. The transferable principle is that entry material, timing, spectral roles and the pace of the transfer jointly determine the result. The demonstration's bar counts and knob sequence are a recipe for those passages, not a mandate for every pair. [Three ways to mix house](https://blog.wearecrossfader.co.uk/blog/3-ways-to-mix-house-music/)

Model a blend as an evolving arrangement:

| Stage | Musical purpose | Questions to resolve |
|---|---|---|
| Prepare | Make B ready while A remains a complete musical experience | Is B the right passage, level and clock? Are controls reset? |
| Introduce | Give the listener a coherent first hint of B | Which texture or rhythm can enter without competing with A? |
| Develop | Let the relationship become intelligible | Does B add support rather than obscure the foreground? |
| Transfer foundation | Replace the rhythmic/low-end job | Does B actually provide the weight A is losing at this event? |
| Transfer foreground | Make B's musical idea take attention | Has A completed its statement? Will its remaining lead conflict? |
| Clear | Remove A while B stands on its own | Are leftover drums, vocals or effects now unnecessary? |

These stages may coincide, or foundation may move before foreground. They are distinct decisions, not six mandatory automation ramps. A good plan names the roles; a weak plan says only “crossfade for thirty seconds.”

### Low end, body and foreground

The bass exchange is a musical event, not a reflex to perform at the midpoint of a fade. If B's introductory drums have no bassline yet, taking A's low end away early may remove the foundation. If B's bass already plays, leaving both full can crowd or destabilize the bottom. Select a transfer point at which B supplies a useful replacement, or make the temporary reduction deliberate.

DJ TechTools explains that instrument harmonics cross EQ bands and that broad EQ is not source separation. Its lesson includes full-frequency balancing, modest EQ blends, bass swaps and tone matching. Consequently, no single top-to-bottom knob ritual is universal. An agent must consider what the selected controls actually change. [EQ mixing: techniques and theory](https://djtechtools.com/2012/03/11/eq-critical-dj-techniques-theory/)

Give the listener one clear foreground unless a known call-and-response or mashup is the intention. Two same-key vocals can compete linguistically; two leads can mask one another without being dissonant. Turning down mids also changes snares, synth body and other content. Prefer better passages over aggressive attempts to rescue a congested pair.

Clearance is not synonymous with an abrupt transport stop. Remove the outgoing musical contribution appropriately, then pause/reuse the deck after it is inaudible and no required tail depends on its signal path. Exactly how to implement and verify this belongs to the control skill.

## 5. Gain, headroom and perceived energy

Match usable incoming level before the audience hears it, considering the passage that will become dominant rather than only a quiet intro. Preserve headroom for overlapping material and transients. A limiter reacting heavily to the overlap is not proof of good balance. The Mixxx manual treats gain staging as a separate discipline from beatmatching and recommends recording and listening back to assess the result. Its meter colors and defaults are specific to Mixxx. [Mixxx DJing manual](https://manual.mixxx.org/2.5/en/chapters/djing_with_mixxx.html)

Complementary fader percentages do not imply constant perceived loudness. Tracks differ in mastering, spectrum, dynamics and density; the relationship between control position and gain depends on the mixer. Do not compensate for removing all useful body by merely raising gain. Restore the intended arrangement and then balance it.

Separate tempo, rhythmic drive, intensity, tension, mood and recognition. A buildup can get quieter while expectation increases; a faster sparse track can feel less forceful than a slower dense one. Laidback Luke's lesson illustrates the distinction between tempo and arrangement-driven energy. A visually dense waveform is a clue, not a calibrated measure across independently mastered songs. [Energy versus tempo](https://www.digitaldjtips.com/energy-vs-tempo-luke-lesson/)

Mixed In Key explicitly positions its energy number as a preparation aid and advises auditioning intro, breakdown and drop separately. The score cannot predict the crowd. PulseDeck's separate scoring formula must retain its own meaning. [Energy detection workflow](https://mixedinkey.com/workflows/use-energy-level-detection/)

At handover, compare the **sustained incoming passage** with what it replaces. A large first kick cannot compensate for an unwanted long loss of drive immediately afterward. Repeated peak replacements can also exhaust a short set; restraint and contrast make later changes meaningful.

## 6. Harmony and pitch as played

Camelot's same cell, neighboring number in the same ring, or same number across major/minor rings are useful shortlist relationships. They do not guarantee matching chord progressions, tuning, vocals or local modulations. Mixed In Key itself requires checking the intended overlap. [Official Camelot workflow](https://mixedinkey.com/workflows/how-to-use-the-camelot-wheel/)

Distinguish Camelot from Open Key notation. Native Instruments' guide uses Open Key and places harmony in the context of the material: sustained tonal overlap needs more care than predominantly percussive passages. Its named “energy boost” moves are creative examples, not a measured rule that a wheel jump raises perceived energy. [Harmonic mixing guide](https://blog.native-instruments.com/harmonic-mixing-rules-and-how-to-break-them/)

Evaluate the audio after rate/pitch processing. Without key lock, rate `r` changes pitch by `12 * log2(r)` semitones; any independent pitch shift adds to that. With key lock, account for explicit transposition and audible processing quality. No universal stretch percentage can guarantee clean voices and transients on all material.

Pioneer DJ's direct harmonic-mixing interviews show different professional priorities and warn against letting compatible-key filtering dictate every selection. Use harmonic evidence to improve the intended blend; do not sacrifice the set's musical purpose to a wheel traversal. [How DJs approach harmonic mixing](https://blog.pioneerdj.com/djtips/how-do-djs-approach-harmonic-mixing/)

Unknown key is unknown. A low-band cut reduces some tonal exposure but does not remove the entire harmony. When a sustained overlap is poorly supported, inspect another region or select another candidate before claiming a forced long blend is safe.

## 7. Selection, pacing and technique choice

Choose a successor for a job: continue a groove, lift intensity, deepen the mood, release tension, change direction or close. Then choose the passages and technique that achieve it. Keep a short horizon with alternatives; an analysis score should narrow a collection, not author the whole set.

Pioneer DJ's all-night interviews with Saoirse, Laurent Garnier, Jane Fitz and Dusky emphasize preparation and variation over an entire arc. Their artistic club context is narrower than all DJ work, but it is strong evidence against treating a set as an endless BPM/intensity climb. [All-night DJing interviews](https://blog.pioneerdj.com/djtips/how-to-dj-all-night-long/)

Ableton's composition discussion explains how tension can operate on multiple scales and can develop through subtle texture rather than a single obvious peak. Using that idea for set construction is an analogy: a small transition should serve the larger progression, not repeatedly reset it by accident. [Dramatic arc](https://makingmusic.ableton.com/dramatic-arc)

Genre informs expectations but cannot choose an overlap alone:

- **House and groove-centered techno:** complementary rhythm and texture can support extended blends. Avoid exhausting interest through endless equal-weight layering.
- **Trance and melody-heavy dance music:** give themes room to develop and make decisive foreground choices. Crossfader cautions against stacking busy melodies and distinguishes a long overlap from slowly blending every frequency. [Trance mixing lesson](https://wearecrossfader.co.uk/blog/how-to-dj-trance-music/)
- **Drum and bass / breaks:** busy patterns and bass content need attention beyond matching fast BPM. Crossfader's guide emphasizes structure, rhythm and EQ; a double drop is a prepared option, not a requirement. [Drum and bass guide](https://wearecrossfader.co.uk/blog/howtomixdnb/)
- **Festival-style EDM:** a prepared drop replacement deliberately redirects an established expectation. Crossfader's demonstration aligns approaches and removes the outgoing payoff at the replacement; this is different from arbitrary deck switching. [EDM techniques](https://wearecrossfader.co.uk/blog/how-to-mix-edm/)
- **Vocal pop, hip-hop, disco and open format:** protect linguistic units and recognize edit/live-timing differences. A short transition can be artistically coherent, but this user's smooth dance-set request should not automatically fall back to hard cuts.

Pioneer DJ's genre survey is useful orientation, not a taxonomy of binding rules. Different DJs mix the same genre differently. [Genre techniques survey](https://blog.pioneerdj.com/djtips/we-uncover-the-mixing-techniques-behind-every-major-genre/)

Real audience reading requires actual feedback. The direct interviews in Pioneer DJ's crowd-reading article describe attending to dancers and adapting selection. An agent without such observation should follow the brief and explicit user reactions, not claim to sense a room. [Crowd-reading interviews](https://blog.pioneerdj.com/djtips/how-to-read-a-crowd/)

## 8. Preparation, uncertainty and recovery

Preparation should preserve **reasons**, not just cue timestamps: complete instrumental entry, vocal pickup, bass arrival, point by which an outgoing lead must clear, useful stable loop, intended exit alternatives. Native Instruments' cue lesson connects markers with arrangement knowledge and avoiding vocal overlap. Do not assume its fixed pad numbering is a cross-application convention. [Cue preparation](https://blog.native-instruments.com/cue-points-djing/)

The Traktor manual documents cue types, reusable loops and pre-listening. Transfer the concepts, not Traktor-specific commands, into the PulseDeck skills. A loop extends the arrangement and changes later timing; it needs a release plan. [Traktor advanced usage](https://docs.native-instruments.com/ni-tech-manuals/traktor-pro-manual/en/advanced-usage-tutorials)

Under a deadline, musical risk and operational risk interact. An unprepared track with the perfect theoretical key is inferior to a prepared candidate that can perform a supported, coherent blend. Prepare a fallback while the current song is healthy. If launch fails, keep the healthy audible track intact, consume the failure, and reassess remaining opportunities. Do not keep following a timeline whose entry has already passed.

Pioneer DJ's back-to-back interviews supply a useful practical reminder: inspect inherited controls, listen to what is currently playing, and allow its valuable passage room. Resetting a silent deck matters musically because an inherited filter can make an apparently correct entrance inaudible. [Back-to-back workflow interviews](https://blog.pioneerdj.com/djtips/how-to-dj-back-to-back/)

If evidence is insufficient for a long complex overlap, improve selection/preparation or use a simpler supported blend. An emergency cut may preserve continuity when a true failure leaves no runway, but should be reported as recovery and a missed musical objective. It must not become the normal way to satisfy the smooth-set requirement.

## 9. Worked decision examples

These are invented teaching examples. Their instrument roles are stipulated observations, not deductions from waveform bands.

### A. A real phrase blend with several musical events

Confirmed steady 4/4. A's first statement has finished by bar 96; its groove continues through bar 112, then a breakdown starts at 113. B has compatible drums at bars 9–16, a bass groove at 17–24, and a lead hook at 25. Its useful passages have been checked at the intended playing tempo.

Here, A97 and B9 denote bars in their respective tracks, numbered from one. Choose B25 as foreground at A113. B9→B25 is 64 beats, so B9 starts at A97. Transfer low-end foundation at A105/B17; B's foreground arrives at A113/B25. Introduce B audibly during the approach rather than running it muted until a final switch. Clear any competing A lead before B25 and remove A as B becomes self-sufficient.

At 124 playing BPM the 64-beat approach lasts about 30.968 wall seconds. That number places an already chosen arrangement; it does not choose the bass exchange or prove the tonal fit. The low-end transfer can be decisive while the overall transition still develops gradually.

### B. Correct count, wrong arrangement

Keep the same 64-beat relationship, but discover B's vocal begins at B17 while A's defining vocal continues until A112. The grid still passes. Reject or redesign the pairing: locate a complete instrumental B entry, select a later handover with enough lead, or choose another successor. Lowering A's mids cannot be assumed to remove only its singer.

### C. An irregular approach

B's confirmed entry beat 32 reaches its desired arrival at beat 104: 72 beats, eighteen bars. A's desired handover at beat 448 requires launch at beat 376. If 376 is not a credible musical entry, a 64-beat launch is not a correction. An alternative complete B entry at beat 72 gives a 32-beat approach, launching on A's beat 416. That alternative is acceptable only if B's beat 72 is musically complete and its shorter overlap supports the intended blend. Otherwise choose another handover or candidate.

### D. A pickup before the headline arrival

B's hook downbeat is beat 96, but its complete lyric starts on beat 95. If B's beat 32 is launched against A's beat 384, the hook downbeat lands on A's beat 448 and the lyric begins on A's beat 447. A's conflicting foreground therefore must clear by beat 447. The pickup should be preserved within the prepared passage rather than chopped off for downbeat symmetry.

### E. An energy hole hidden by labels

A has a generated outro at its target handover; B has a generated intro at its selected entry. Both are downbeats. If A loses bass and drums while B remains atmospheric for another sixteen bars, a simple fader exchange loses drive. Use an earlier audible B approach and a developed B arrival, choose another entry, or intentionally program a release. Matching the labels `outro` and `intro` does not resolve the musical relationship.

### F. A numerically perfect cut that still fails this request

A's outro downbeat and B's drop downbeat are exact. B starts with its fader full while A immediately becomes inaudible. This is a correctly placed cut. It fails a requested smooth blend because no audible relationship develops before the foreground replacement. Changing the log label to `transition` cannot fix the sound.

## 10. Reference taxonomy with clear ownership

The three entrypoint skills should route a task and state a small number of invariants. Detailed teaching belongs in focused references loaded when needed. MECE here means each decision has a primary owner; music itself is interconnected, so references should link rather than repeat full chapters.

| Skill | Reference area | Owns | Leaves elsewhere |
|---|---|---|---|
| DJing art | Data to decisions | Evidence interpretation, confidence and musical questions | CLI field syntax and library mutation |
| DJing art | Beatmatching and groove | Pulse, phase, bar relation, swing and local timing implications | Exact Sync commands and native scheduling |
| DJing art | Phrasing and arrangement | Protected statements, entries, arrivals, pickups and irregularity | Fader/EQ implementation |
| DJing art | Mixing and blending | Audible roles, foundation/foreground transfer, balance principles | Deck load/transport guards |
| DJing art | Harmony and pitch | Tonal overlap and processing implications | Key metadata persistence |
| DJing art | Selection and energy | Successor purpose, pacing, set context and feedback | Playlist CRUD |
| DJing art | Transition design | Combine the preceding judgments into a source-bound staged plan; technique choice | Command scheduling API |
| DJing art | Checked plans / worked cases | Arithmetic checks and contrasting examples, clearly separated from artistic proof | Claiming checker success certifies sound |
| Manage Library | Collections | Find/import/organize source-specific tracks and playlists | Which track should play now |
| Manage Library | Analysis, grids and cues | Inspect provenance and persist/correct reusable source knowledge | Pair-specific musical judgment |
| Manage Library | Exchange | Import/export identity, cue/grid preservation and verification | Live deck setup |
| Control PulseDeck | Deck preparation | Routing, load readiness, isolation, tempo/Sync setup and reset | Choosing artistic arrivals |
| Control PulseDeck | Transition execution | Translate staged plan into guarded actions and verify actual results | Universal musical recipes |
| Control PulseDeck | Session continuity | Rolling preparation, supervision, deadlines and deck reuse | Energy arc theory |
| Control PulseDeck | Setup/recovery and recording | Devices, supported fallback mechanics and captured evidence | Inventing missing musical content |

`Transition design` should be an integration reference, not a second mixing manual. `Checked plans` should explain arithmetic validation and its limits, not re-teach musical choices. A compact task-trigger table in each `SKILL.md` can make this deeper material discoverable without forcing every reference into every live decision.

## 11. What to validate next

Assess four dimensions separately:

1. **Musical selection:** Were complete, justified passages chosen? Were requested payoffs protected? Did the next passage serve the intended energy change?
2. **Timing:** Did actual beat/bar/phrase relationships hold through the overlap, including pickups and unequal local timing?
3. **Audible blending:** Did both sources contribute intentionally before clearance? Were foundation and foreground transferred coherently without unwanted holes, collisions or level jumps?
4. **Operational continuity:** Did readiness, launches, clearance, deck reuse and recovery keep the set going?

Automated inspection can establish timing and control facts. It can identify clear violations such as zero intended overlap in a blend. It cannot certify that a singer is intelligible or a groove feels good without relevant audio/listening evidence. Keep human feedback and recording review in the evaluation.

Kim and colleagues' analysis of 1,557 real mixes finds transition-length regularity around 32-beat multiples and substantial variation in cue choices. These are useful priors, not gold-standard targets for every transition. Also note that papers use `cue-in`/`cue-out` differently: their cue-in can mean transition completion, whereas the GAN paper uses cue-in for incoming audibility. Prefer explicitly named entry, audible entrance, handover and clearance in agent plans. [Real-world mix analysis](https://arxiv.org/html/2008.10267)

Zehren and colleagues' cue-detection work considers structural novelty, metrical position and salience of material following a candidate. That supports looking around a boundary. Its reported cue quality concerns candidate points under a particular evaluation, not arbitrary pair compatibility or a complete DJ set. [Automatic cue detection](https://arxiv.org/html/2007.08411)

## Access and evidence notes

The linked manuals, original school lessons, direct interviews and research papers were accessed through web tools for this pass. No Reddit or anonymous forum advice forms the basis of the recommendations. Search also located Resident Advisor's Jeff Mills, Objekt and Function interviews. RA's open-page responses omitted their article bodies; indexed text was available for parts of Mills' interview, but the report does not rely on an unreviewed full article or claim complete access. [RA series](https://ra.co/features/series/art)

Teaching sources sometimes simplify all bars to four beats, all phrases to one fixed size, or pitch-wheel movement to an automatic energy increase. Retain the useful demonstration and qualify its scope. Producer arrangement material is used as supporting musical theory; set-level applications are expressly synthesis. None of these sources independently validates PulseDeck analysis or the rewritten skills.

## Applied reference rewrite and checks

The three skill entrypoints now route to focused topic references. DJing art separates data interpretation, beatmatching/groove, phrase/arrangement selection, audible blending, harmony/pitch, selection/energy and transition design. Checked plans remain a coordinate validator; worked cases include an aligned cut and silent pre-roll that fail a smooth-set brief. Library has an analysis/persistence reference, and control has a state/ownership reference. Recovery mechanics have one primary owner in control.

The blend guidance prefers a developed audible relationship for a continuous dance set while preserving cuts as contextual artistic or emergency choices. The helper explicitly reports that audible overlap is not evaluated and no longer suggests changing to a cut as a remedy for a missing blend approach. No native engine behavior was changed in this research pass.

Verification completed: exactly three entrypoints; all local reference links resolve; 25 shell/JSON example blocks parse; all three frontmatter checks pass; seven packaging/relocation tests and seven phrase-coordinate regressions pass. A separate research-integration review found no major musical inaccuracies; its minor ownership note was addressed by linking operational recovery to the control skill.

These checks do not establish musical performance. The preceding blind Luna trial failed this user’s mixing objective despite aligned cut coordinates. Its frozen inputs and native receipts remain under `artifacts/skill-trials/luna-phrase-checked-20261015T013226Z`. Remaining automation was cancelled; both decks were already paused at the post-stop observation. No new live DJ set was run after this reference rewrite. A fresh blind performance and listening review are still needed to assess the resulting behavior.

## Subsequent product decision

The user subsequently chose manual-only cues. Automatic section/cue generation and its settings were removed from the implementation and active skills. This research retains its evidence and historical examples; references to generated labels describe the studied failure mode, not a currently available PulseDeck feature. The next agent must reason from measured beats/key/waveforms and human annotations.

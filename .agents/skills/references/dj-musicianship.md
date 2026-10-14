# DJ musicianship: selection, phrasing and purposeful transitions

Read before selecting or executing musical handovers. This is a decision guide; [musical evidence](musical-evidence.md) covers analysis limits and [CLI](cli.md) covers execution. Keep a brief working plan, then act within the authorized session. A successful command verifies a control, not an artistic result.

## Hear the structure, then choose the controls

Four separate scales matter: **beat** (pulse), **bar** (meter), **phrase** (a musical unit), and **section** (an arrangement role such as intro, breakdown, buildup or drop). Matching BPM and fractional beat phase covers only the first scale. A downbeat detector predicts bars; it does not hear phrases or recognize a drop.

Map the passage, not just the whole-track BPM. For each candidate event, keep source timestamp/range, bar or beat position, role, evidence and confidence. Prefer heard music, user-confirmed cues or a trustworthy source-specific arrangement map. Numerical band changes support hypotheses: compare before, during and after, and look for repeated structural changes. A first kick, bass entry, loud bin or 16-second timestamp is **not** automatically the first drop. A drop hypothesis needs a preceding tension/build context and a following sustained payoff. If that context cannot be established, call it a candidate bass arrival and avoid reporting that the requested buildup/drop has played.

Do useful preparation when listening is unavailable. Use conservative arrangement hypotheses, state that limitation once, and avoid dense melodic overlaps or claims of seamlessness. If the exact first payoff is essential and cannot be located, request a cue while preparing the next deck. Do not manufacture certainty or stall a healthy live track waiting for an answer.

## Translate the requested pace

Keep three durations distinct: time spent on a song, audible overlap, and the final fader/EQ gesture. “Quick mixing” can mean reaching the next song sooner while still using a carefully aligned intro; it does not mean every handover is a subsecond crossfade.

For “intro/outro mixing, first buildup and drop, then next song,” protect the outgoing first buildup and payoff. Choose an appropriate phrase boundary after the payoff has had room to resolve; plan the incoming intro backward from that handover. An early mix-out region can perform the role of an outro without being the file's actual outro. Name it accurately. Do not erase the payoff by starting the next buildup over it unless a deliberate, prepared swap was requested.

House/progressive passages may reward a developing groove; festival-oriented passages may center on a clear payoff. Treat these as arrangement choices, not universal artist or genre formulas. The user's requested pace takes precedence over a conventional long blend. See [Pioneer DJ's genre comparison](https://blog.pioneerdj.com/djtips/we-uncover-the-mixing-techniques-behind-every-major-genre/) for contrasting practices.

## Write a handover plan before launching

Record these fields in working notes, not a long user-facing checklist:

| Decision | Required musical content |
|---|---|
| A: protect and exit | First payoff to preserve if requested; selected exit/handover event; evidence; latest useful exit |
| B: entry and arrival | Chosen entry, including any pickup; the groove/hook/section that should arrive at A's handover |
| Timing | Actual beats from B entry to arrival; corresponding A launch event; meter/bar phase; isolated verification deadline; last control-arming deadline; audible opening; bass exchange; A clear point |
| Sound | Playing tempo and percent change, key/pitch state, low/mid/high contributions through entry/overlap/handover/exit, dominant musical idea, density/vocal risks, gain/headroom |
| Execution | Sources/load revisions, ready-by deadline, planned native launch and bounded control phases, fallback boundary |

Count backward from **B's intended arrival**, not forward from an arbitrary file start. If B needs 32 beats to reach its arrival, find A's event 32 beats before the chosen handover. At constant 124 BPM this lead is about 15.48 seconds; on neural grids count actual event intervals and account for playback rate. If only 16 beats remain, choose a justified later B entry or a later A boundary; do not squeeze a 32-beat intro into 16 beats by wishful phrasing.

Protect the pickup: the first decoded beat may occur before B's first downbeat. Either preserve it and launch at the corresponding phase in A's bar, or deliberately choose a later musically complete entry. Do not snap file zero to its nearest beat and call that a phrase start.

Check all four scales:

1. **Tempo/local pulse:** compare the selected passages, including half/double-time and repaired-grid warnings.
2. **Beat phase:** from one snapshot, wrap `B.beat_position - A.beat_position` into `[-0.5, 0.5)`.
3. **Bar phase:** when both origins represent credible downbeats with the same Q-beat meter, wrap the difference into `[-Q/2, Q/2)`. A difference of two whole beats is beat-aligned but halfway through the wrong 4/4 bar. Account for an intentional pickup/offset in the plan. Do not apply modulo-Q to an unvalidated bar origin or mismatched meter.
4. **Phrase/section:** compare how many beats remain to the two selected musical events. Matching bars alone cannot establish this.

For a sustained rhythmic blend, repeated kick flams or growing phase error are a reason to keep B isolated and fix/relaunch or choose another boundary. Convert error to milliseconds using local beat length and inspect it over time; there is no one tolerance that proves every timbre sounds aligned. Native timestamp resolution also limits precision.

## Tempo and harmony serve the selection

Hold the live groove unless a tempo progression is intended. Choose a successor near the playing tempo before stretching a poorly matched track. Calculate the necessary adjustment, `100*(target/source_BPM - 1)`, rather than always driving the set to the top of the requested range. A 120–124 BPM request defines an allowed playing range, not a requirement that every track run at 123.98.

For a longer progression, schedule gradual tempo movement in a suitable passage after the old deck is isolated, observing the requested range. Do not change the clock during a queued launch or a beat-clock ramp. No universal stretch percentage guarantees acceptable transients, vocals or groove. Review changes relative to the material and key-lock quality; if the required change damages it, choose another track or a deliberate tempo-changing transition.

Compare keys **as played**. With pitch unlocked, rate changes transpose pitch: approximately `12*log2(rate)` semitones; explicit pitch shifts also matter. With key lock, verify the actual pitch setting and readiness. Same/adjacent Camelot cells or relative major/minor are useful starting points; same tonic major/minor is not automatically the same harmonic set, and different notation systems are not interchangeable. Consider alternative key estimates, modulation and the actual overlapping notes. For distant or unknown keys, choose low-tonal/percussive passages, reduce harmonic overlap, use a clear section cut, or select a different follower. A key label cannot certify a mashup. [Native Instruments explains key/tempo behavior and harmonic relationships](https://docs.native-instruments.com/ni-tech-manuals/traktor-pro-manual/en/global-concepts); its [harmonic mixing discussion](https://blog.native-instruments.com/harmonic-mixing-rules-and-how-to-break-them/) also emphasizes musical exceptions.

## Choose one coherent handover

| Approach | When it fits | Musical action |
|---|---|---|
| Intro into outgoing groove/outro | Compatible rhythms with room for B's intro | Introduce B's supporting elements, transfer bass at the chosen structural event, clear A before competing hooks |
| Short phrase exchange | User wants fast movement; a dense overlap would clash | Prepare B before the boundary, exchange at the musical one, use only the short fade needed by that passage |
| Breakdown bridge | A offers a real release and B's texture fits | Let a motif finish, manage pulse/tension deliberately, land B's arrival at the planned resolution |
| Prepared payoff swap | Both buildups/arrivals are known and their roles fit | Preserve or intentionally redirect the expected payoff; clear A's lead/bass so the new drop reads clearly |
| Tease and reveal | A known B motif can foreshadow its arrival | Brief, phrase-aligned tease; return space to A, then reveal B at the chosen arrival |

These are candidates, not a rotation to apply mechanically. Surprise should create recognizable tension and release while keeping one main idea legible. Pick one device, explain its purpose briefly, and keep a straightforward fallback. Do not invent acapellas, stems, cue edits or unsupported mixer features to perform a named trick.

Gain-match before opening the channel when cue/listening is available, and preserve headroom through overlap. Similar peaks do not establish equal perceived loudness. Keep a deliberate owner of the low end; choose whether the transfer is decisive or gradual from the arrangement. Don't fade down A's bass simply because B started: B's bass may arrive much later. Manage midrange melodies and vocal phrases as well as kicks. A high-pass filter is not a vocal remover.

### Preserve fullness and clarity across all bands

Plan low, mid and high contributions at entry, overlap, handover and exit. Keep useful weight, body and air supplied by one or both tracks as the arrangement permits; don't create an accidental spectral hole while clearing competition. Fullness does not require boosting every band, equal band energy, or filling a deliberate musical breakdown. Two full midranges can mask each other even when only one bassline is open. Likewise, removing both tracks' mids or highs can make the transition hollow or dull.

Choose which musical idea leads the midrange and which supplies supporting rhythm/top detail. If B's mids will carry the next hook, attenuate competing A mids as B's mids become audible, rather than relying solely on a low-EQ swap. Preserve A's body/air while B is still a sparse intro, then transfer those roles at the planned event. If both top ends are dense, reduce the supporting high band instead of stacking brightness. Use subtractive, proportionate changes before adding gain; no fixed mid/high cut suits every pair. [Native Instruments' DJ effects guide](https://blog.native-instruments.com/best-dj-effects/) discusses EQ for balancing all three bands and avoiding crowded mids.

PulseDeck's three bands are broad filters, not stems or isolated instruments. Attenuating mids can affect body, vocal, snare and synth content together; EQ cannot isolate a singer. Band RMS and peak meters can warn about density or lost contribution, but cannot certify masking, harshness, equal loudness or a full-sounding master. Treat the user's listening feedback as evidence and adapt the next unarmed handover; do not disrupt a timed phase with unplanned knob experiments. Restore the incoming intended final EQ/filter after the exchange.

When the user prioritizes mixing in key, filter the shortlist for same/adjacent/relative compatible keys before ranking convenience or tempo. Verify confidence, alternative estimates, key-lock readiness and pitch as played. A distant-key pair needs a deliberate low-tonal passage and a stated reason; a short fade does not automatically excuse it. Harmonic fit still needs space for the two arrangements and their registers.

## Make the musical schedule survive tool latency

Prepare the successor while the current track is playing, early enough for observed provider/analysis/key-lock latency and a fallback boundary. Load completion is not a musical trigger. If preparation misses the intended boundary, reassess the next plausible boundary rather than mixing immediately when the job turns ready.

Use the native future timestamp for the planned launch. `--clock-revision` takes the clock deck's **load revision**, not its position revision. Verify B while isolated with enough lead before its planned audible opening. The intro keeps advancing during observations: preserve the intended arrival instead of blindly opening B after a model/tool round trip. A next-beat launch is appropriate for a requested immediate cut or a mechanically demonstrated trial; it is not a default phrase plan.

Make launch, verification deadline, last control-arming deadline, audible opening and arrival separate events. Budget observed command/observation latency between them. If verification cannot finish and the controls cannot be armed before opening, change the plan while B remains isolated. Do not shorten the protected payoff to compensate for a missed deadline.

Keep the model outside the timed interval: finish the bounded controller, validate it, and start it before it queues the native launch. A saved plan is not a running controller. Choose among already-inspected eligible future candidates from a fresh snapshot and queue within that same invocation; a model turn between observation and dispatch can make a literal timestamp stale. Have a prepared fallback boundary/action and a recovery deadline with enough outgoing audio remaining. If the controller rejects the primary opening, execute that prepared fallback or stabilize promptly; do not spend the remaining track researching a replacement plan. See [timed execution](timed-execution.md) for phase spacing, recovery and controller requirements; this reference does not supply a verified live executor.

For already-decided controls, prepare short guarded batches/native ramps so sequential IPC and reasoning do not scatter coordinated actions. Quantization belongs to each ramp; separate calls can hit different boundaries if submitted too late. Verify all necessary controls can be armed before their boundary. The CLI does not make arbitrary scripts sample-accurate choreography. If that timing is unavailable, choose a wider musical window or describe an approximate cut rather than promising a precise drop swap.

Verify actual audible overlap from when B first contributes until A clears; this includes waits between commands. Ramp duration alone is not total transition duration. Recheck meter/beat drift and the planned arrivals during a long overlap. Skip stale launches and preserve the currently healthy deck on a failed preparation.

### PulseDeck effect limits

The current [deck DSP](../../../crates/deck-engine/src/dsp.rs) mixes echo/reverb into each deck before channel/crossfader attenuation. Closing the channel therefore attenuates the wet tail too; `set_fx` is not a separate post-fader send or a dry-kill echo-out control. Do not claim a free-standing echo tail after shutting that fader. Verify the installed signal path and available commands before a different effect design. Any purposeful FX gesture needs a chosen musical moment, known beat duration/depth, headroom, and a reset after isolation. More effects cannot repair a wrong bar or an unfinished vocal phrase.

## Learn from artists without pretending to reproduce them

Treat the user's artists as references for different musical priorities, not one combined preset. Capture documented principles from direct interviews/demonstrations, then choose the ones that fit this set. Distinguish a live electronic band/production arrangement from two-deck DJ technique. Research improves the decision procedure; it does not supply practiced artist-level taste, crowd feedback, audible audition, or undocumented performance tools.

Useful documented study directions:

- **Solomun:** his [conversation with Idris Elba](https://www.interviewmagazine.com/music/solomun-idris-elba-ibiza) describes simple low-cut transitions and occasional reverb. Study restraint and selection; it does not prescribe the same move for every track.
- **Lane 8:** his [direct interview](https://edmidentity.com/2018/12/31/featured-interview-lane-8/) emphasizes familiarity with the music, careful selection, context, and a clear musical idea. Study space around melodies and rejecting a favorite track when its tempo/key/moment does not fit.
- **James Hype:** his [Hï Ibiza interview](https://www.hiibiza.com/news/the-james-hype-hi-ibiza-interview) connects experimentation to extensive prior practice. His and **David Guetta's** comments in [Pioneer DJ's artist tips](https://blog.pioneerdj.com/djtips/here-are-8-pro-level-dj-tips/) provide examples of assigning layer/frequency roles and considering harmony. Study prepared, verified routines instead of substituting random FX for technique.
- **RÜFÜS DU SOL:** [their live setup demonstration](https://djmag.com/features/how-i-play-live-r%C3%BCf%C3%BCs-du-sol) concerns live electronic performance; its musical arrangement priorities do not imply an available two-deck routine. **Avicii's** [direct interview](https://www.gq.com/story/avicii-tim-bergling-profile-gq-april-2013) supports studying advance transition/energy preparation.
- For **Martin Garrix, Nicky Romero, Hardwell** or another reference, inspect an actual artist demonstration or direct explanation before attributing a signature technique. Production masterclasses and artist reputation alone do not establish DJ procedures. Use the requested festival/progressive energy priorities provisionally, without inventing artist-specific rules.

For basics, [Native Instruments' transition lesson](https://blog.native-instruments.com/dj-transitions/) connects arrangement-aware phrasing to fade/cut/bass-swap choices, and its [cue-point lesson](https://blog.native-instruments.com/cue-points-djing/) explains preparing structural markers. Session-only notes are sufficient unless the user also authorizes durable cue edits.

After a session, record the specific audible/user-reported issue and the relevant evidence: timing, competing ideas, lost payoff, pitch/groove damage, gain discontinuity, or ineffective FX. Use it to change the next decision. A clean JSON endpoint is an operational check; musical validation needs listening or user feedback. For eligible local material, use recordings only when requested/authorized and respect recording restrictions. Do not record restricted Spotify sources to obtain an audition.

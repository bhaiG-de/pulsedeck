# Beatmatching and a convincing groove

Book basis: Morse's two “Beatmixing” chapters and “Using Sync, Hot Cues And Loops”; Broughton/Brewster chapters 8, 9 and 14. Applying controls belongs to [deck preparation](../../pulsedeck-control/references/deck-preparation.md).

## Verify the timing layers separately

| Layer | Question | A pass still leaves open |
|---|---|---|
| Tempo | Do the local pulses advance at the intended relative rate? | Where they fall together |
| Beat phase | Do corresponding pulses coincide? | Whether beat one meets beat one |
| Bar identity | Do the metrical accents correspond? | Whether the relevant phrases meet |
| Phrase progression | Do the chosen musical changes have matching countdowns? | Whether the overlapping content works |
| Groove | Does the combined rhythmic pattern remain convincing? | Tonal fit, balance and set purpose |

Equal BPM displays address only part of the first row. Quantized launch addresses the boundary it quantizes to. Sync follows the capabilities and settings of this system; it cannot infer a lyric ending or repair an extra arrangement bar.

## Tempo, then phase, then recheck

For a sustained beatmix, choose the common playing tempo and prepare B while isolated. Preserve A's established pulse unless a tempo change is intentional. For a stable local source tempo, `rate = desired_playing_BPM / source_BPM` and `change_percent = 100 × (rate − 1)`. Use a credible natural local pulse at the selected passage, not an already stretched playback BPM as the denominator. Re-read actual playback behavior; inherited rate, local tempo changes and pitch processing matter.

Start at the selected metrical position, verify alignment over a useful interval, and return to the final entry if a rehearsal moved it. Let supported scheduling carry known musical events. Repeated model calls timed by wall-clock sleeps are a poor substitute for native timing.

| Observed behavior | Diagnose | Appropriate response |
|---|---|---|
| A small offset stays constant | Phase/start offset | Correct the quieter/isolated source with the supported phase control; avoid changing its steady tempo unnecessarily |
| Offset grows steadily | Rate mismatch | Correct rate/Sync strategy, then realign phase; repeated nudges alone cannot cure it |
| Drift changes through the passage | Live timing, changing tempo, faulty grid or adaptation | Inspect local events; use supported following, a steadier passage, or a shorter overlap |
| Kicks coincide, backbeat feels displaced | Wrong bar origin, pulse interpretation, or intentional rhythm difference | Check accents and meter before applying another fractional nudge |
| Timing is stable, pattern is crowded | Conflicting grooves or too many attacks | Reduce competing rhythmic layers, choose another passage, or shorten the blend |

In a simultaneous PulseDeck snapshot, fractional wrapped `B beat_position − A beat_position` can be near zero while the difference modulo meter is one or two beats. Check both, then compare remaining intervals to the chosen arrivals. A grid can also be consistently wrong: numeric agreement needs credible origins.

For compatible forward-advancing clocks, define fractional error as `delta = ((B − A + 0.5) % 1) − 0.5` using a nonnegative modulo. A positive delta means B is ahead in the beat cycle; a negative delta means B is behind. For A=100.20 and B=40.23, delta is +0.03: B is ahead, despite its smaller absolute beat number. Distinguish that direction from the separate bar/phrase relationship, and verify the supported correction's direction before applying it.

## Understand rhythm beyond the kick

The beat is the reference pulse; rhythm is the pattern of sounds and rests around it. A backbeat often emphasizes beats two and four in 4/4. Offbeats lie between beats. Syncopation shifts emphasis toward weak beats or subdivisions; swing changes subdivision timing. Neither requires a change in average BPM.

Do not nudge a deliberately laid-back snare onto the grid and break the kick relationship. Two grooves can share tempo and downbeats while their hats, shuffled subdivisions, bass accents or busy breakbeats fight. Similar or complementary patterns are promising. An overlay must still sound like an intentional combined rhythm.

For a half/double relationship such as 87/174 BPM, decide which pulse listeners follow and where the snares and phrase arrivals fall. Two fast beats can correspond to one slow beat; that does not make one fast bar equal one slow bar. Count the actual mapping. An octave-like relationship between numbers is not an arrangement plan.

## Choose a beatmix or a tempo reset deliberately

For a flowing café/house/disco blend, first find a musically appropriate successor with a nearby natural tempo. Make the small correction needed, align beats and bars, then place and expose compatible phrases. Feedback against extreme stretching is not a ban on tempo matching, Sync or audible overlap. Natural 123 and 124 BPM clocks drift by half a beat in thirty seconds; starting both on downbeats with Sync off does not keep them matched.

Morse's single-phrase beatmix makes both records audibly contribute for a musical phrase; muted pre-roll is preparation. His bassline swap controls incoming lows and transfers the foundation at a useful moment. Broughton/Brewster chapters 9 and 13 describe different overlap lengths and earlier entrances timed so the next main idea arrives as the outgoing statement resolves. Establish those relationships from the actual music; don't impose a universal bar count or invent a phrase from an RMS dip.

Place an ordinary blend while useful rhythm and body remain in A. Waiting until near-silence removes material you could have mixed over. Outro-to-intro is legitimate when it creates the desired space; repeating it on every track lets the recordings dictate the set. Try a different close-tempo successor or a better passage when that would preserve a requested blend. Reserve end-to-end fades and large resets for purposeful changes of scene, unsuitable rhythmic material, or disclosed recovery—not as the automatic response to the stretching complaint.

Range can develop through a few beatmatched grooves with changing timbre, familiarity or emotional color, followed by a deliberate reset into another coherent stretch. This is an option, not a quota of blends or a fixed genre sequence.

## Keep the processing musical

### Decide how much before matching

Morse's “Beatmixing Part 2” recommends records already close in tempo and gives roughly ±4% as a rule of thumb (about 5 BPM around house tempo). Use this selection guideline without treating it as a physical limit. It is a percentage of each source's own tempo: 4 BPM at 80 BPM is 5%, while 4 BPM at 125 BPM is 3.2%. Broughton/Brewster's chapters 15 and 31 warn that around ±3% already changes sounding pitch materially without key lock; that is a pitch warning, not a safe stretching allowance.

Before accepting a proposed move, retain each source's natural local BPM, target BPM and resulting percentage. Check both tracks, the entire intended overlap, and the incoming solo continuation. A small difference from the previous track's *playing* BPM can conceal a large change from the new record's natural speed. Repeated matches can carry a set far from later records' natural tempos. A planner's supported option, high score, short exchange or successful Sync does not certify musical suitability.

For ordinary blends, prefer changes within roughly ±4% and smaller when voices, acoustic instruments or distinctive grooves sound altered. Without listening evidence, use that neighborhood as the default selection range; first search for a different suitable close-tempo successor instead of approving a wider bend on numerical evidence alone. If a larger change serves the set, design a deliberate reset. A deliberately transformed sound can justify more when the brief and actual listening support it. There is no universal percentage that guarantees natural sound, and key lock preserves neither articulation nor groove. A listener saying the track sounds wrong is evidence to revise the rate/selection strategy, even if execution succeeded.

For example, 94 → 107 BPM is +13.8%, and 80 → 107 BPM is +33.75%. These are substantial reinterpretations, not routine beatmatching corrections. Splitting the difference between two distant sources does not solve the problem unless **both** resulting changes serve the music.

### Distinguish three kinds of change

- **Phase correction:** Broughton/Brewster chapter 8 distinguishes a brief nudge/pitch bend to align beats from a lasting pitch-fader adjustment. A fixed offset needs phase repair; progressive drift needs a small rate correction followed by realignment. Repeated nudges do not justify keeping a song at a grossly unsuitable speed.
- **Gradual set movement:** Choose naturally adjacent tempos and let the direction develop across records. If changing a playing record, use a deliberate small, gradual movement in a suitable less exposed passage; conspicuous vocals, sustained tones and characteristic attacks make changes easier to hear. Maintain the intended master/follower relationship while both rhythmic sources are audible. Do not impose an automatic acceleration every song or a universal BPM-per-minute ramp. Recheck cumulative source-relative changes.
- **Tempo reset:** For a larger stylistic turn, use a supported complete ending, phrase cut, resolved fade or genuinely beatless bridge, with the next record near its own natural speed. Morse's “Five Basic DJ Transitions” explicitly allows different tempos/genres without beatmixing. Avoid prolonged coexistence of mismatched drum patterns: disabling Sync while leaving two active grooves overlaid is not a fix. A reset need not mean silence or an abrupt gain jump. Placement still needs musical evidence.

Do not jump an audible incoming record back to native tempo immediately after clearance just to undo a forced match. Plan its solo speed before the blend; isolate before a large reset, make a justified gradual change, or use a different transition. When clearing Sync or promoting a new master, verify the resulting effective rate rather than assuming the control returns to zero. Check a suspected half/double BPM against accents and the actual pulse mapping; relabeling the estimate cannot make unsuitable playback sound natural.

### Preserve runway when the rate is unsuitable

Make the rate decision before arming, while a different source or technique is still feasible. If the proposal needs an unacceptable bend, reject that proposal and secure a reachable alternative while the carrier still has useful music. Merely shortening a synced overlap does not remove its large tempo change. After a rejected/failed handoff, reconcile its actual state before replacing it; do not leave the outgoing track to end while repeatedly troubleshooting the same pair. An accidental gap is a recovery failure, not an intentional tempo reset.

PulseDeck's one-time absolute playback BPM is an entry setting. Express an ordinary sustained beatmix as a prepared bounded master/follower Sync relationship plus `alignment.maintain:true` on the action spanning both rhythmic contributions through clearance. This runs the chosen relationship on the native clock, including corrections and recovery, rather than leaving later phase to another model call. For intentionally independent clocks, use the forecast’s phase progression across the complete overlap and design their exposed rhythmic relationship explicitly. Do not save a constant source grid just to flatten human timing for this mix. Grid editing and performance tempo matching are different actions.

If reliable rhythmic overlap cannot be established in time, choose a musically supported alternative. Preserve a requested smooth result by trying better passages or a better successor while runway remains. Name an emergency recovery honestly if that opportunity is lost.


## Derive an actual playing-pulse strategy

Suppose two rhythm-bearing passages naturally run at 112 and 117 BPM. Over 20 seconds at natural speed their relative progression changes by `(117 − 112) × 20 / 60 ≈ 1.67 beats`. Launching both on a boundary or closing one low band does not remove that difference; rhythmic material extends across the spectrum.

One plausible nearby common pulse is 114 BPM: A needs `100 × (114/112 − 1) ≈ +1.79%`, B needs `100 × (114/117 − 1) ≈ −2.56%`. If a gentle lift serves the brief, establish A's small gradual change in suitable material before B's audible overlap, prepare B at the chosen entry, and use valid master/follower Sync or demonstrated stable matching to maintain the pulse. Verify actual rendered rates and phase through the overlap. This is a candidate construction, not an instruction always to split the difference. Choose another closer source or a designed tempo exchange when a common pulse would damage character or the musical direction.

For an intentional unbeatmatched exchange, map the audible rhythm: A can finish its pulse while B contributes a genuinely noncompeting atmospheric approach, then B's complete rhythmic arrival establishes the new pulse. A long overlap of two exposed rhythms is a different construction and needs deliberate rhythmic compatibility; “café” does not make drift smooth.

## Establish what the clock represents

A valid effective clock can still be a manual uniform BPM override with no bar identity. Keep that provenance visible. In one failed live join, Don't Forget My Love had a manual 122.9 BPM uniform effective clock, a stored neural estimate around 121 BPM and `beatgrid_downbeat:false`. Matching its fractional clock phase to another record did not establish acoustic pulse or bar compatibility. A modulo-four difference is also inconclusive when a bar origin is unknown.

Resolve the chosen passage using available local evidence: compare recurring low/peak rhythmic attacks over a fine bounded waveform window with the effective event intervals and the stored analysis provenance, and establish a credible metrical/phrase origin. Distinguish an intentional human correction from an unverified override. If this cannot be established while a healthy alternative remains, choose a source with supported local rhythm or a complete scene exchange. Preserve the user's canonical BPM/grid; a playback test does not authorize rewriting it or treating the original neural estimate as automatically right.


Intentional ratio relationships/polyrhythms can also coexist when the recurring cycles, accents and exposure are designed and counted. Compare equivalent pulse units and shared cycle arrivals; fractional phase on unlike beat units is not by itself their success criterion. This is a deliberate construction, not a reason to ignore unexplained drift in an ordinary beatmix.

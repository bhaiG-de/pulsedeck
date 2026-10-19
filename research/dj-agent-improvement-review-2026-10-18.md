# PulseDeck DJ agent: researched improvement review

18 October 2026 · **Observations and recommendations only. No implementation authorized by this document.**

## 1. Scope and conclusion

This consolidates two observed Luna performances, the user's listening feedback, primary DJ instruction, source inspection and current music-retrieval research. It covers musical judgment, the three DJ skills, library discovery, control reliability, product capabilities and evaluation. The current performance was explicitly stopped; both decks are paused, no jobs remain running, and the observation heartbeat is paused. Do not restart it.

The central findings are:

1. **Unattended control is not reliable yet.** A proposed track load could remain blocked while Play executed on the previously loaded track. That source later ended without a successor. Queue ownership and readiness dependencies need product-level work; this cannot be solved by more musical instructions alone.
2. **Technical compatibility is being mistaken for musical fit.** BPM, key and waveform-derived energy do not establish drive, emotion, vocal focus, groove or a convincing next passage. The user's reported mismatch is a core requirement for better evidence and retrieval.
3. **The agent has a narrow performed vocabulary.** It changed entries and exits when asked, but repeatedly used paired channel fades plus a bass swap. A practical, passage-led repertoire is needed, including filters, foreground/EQ interplay, loops, teases, contrasts and supported FX. Scratching requires its own capability and listening validation.
4. **Successful command receipts are being overinterpreted.** Fractional beat lock can conceal bar displacement; zero underruns do not prove good musical timing or sound. A checker-approved plan can differ from the plan actually submitted or resolved.

Preserve what works: isolated preparation, cached analysis, exact effective beat events, native timed execution, keylock/Sync controls, paged library reads, useful suggestion features, source provenance, cues and user takeover. Do not reduce analysis quality, remove features or hide failures behind longer prompts.

## 2. Evidence and measured outcomes

### Evidence standard

| Evidence | Supports | Does not establish |
|---|---|---|
| Native action/version receipts and renderer frames | Which controls executed, resolved anchors, durations and source revisions | What a listener heard or whether the musical choice was good |
| One-second transport samples, faders, peaks and routing | Sampled progression, contribution, source identity and phase | Absence of every short glitch or sample-accurate audible onset |
| User listening feedback | The user's perceived fit, pacing and repetition concerns | An automatically identified source pair when none was named |
| Source/catalog inspection | Implemented controls, guards and plausible failure mechanisms | That an untested technique or proposed fix works in the running build |
| Primary instruction/model literature | Technique principles, model inputs and research precedent | Local runtime performance or DJ-quality guarantees |

The observer did not listen to the master output, record the performances or audition the research examples. Books/courses were accessed through public publisher material, excerpts and readable instruction; complete paid works were not read. Videos were not played. No embedding model was downloaded or benchmarked.

### Run summary

| Run | Verified outcome | Important limits |
|---|---|---|
| First fresh verified mix | Nine completed handovers; renderer span **59m29.194s**, approximately30.806s short of the requested hour; final decks paused/muted | Eighth overlap sampled phase error reached0.41064beats. Ninth maintained fractional lock but was approximately one beat displaced in the bar. Later playback reached about132BPM. |
| Calm progressive mix | Started16:39:31.362UTC /10:09:31PM IST; adapted to later faster/creative steering; ultimately stopped by user | Wrong source replayed during an action named for Trivial. First sampled unexpected EOF/silence at approximately22m35s; at least38.17s sampled silence before first sampled global cancellation. Final paused snapshot at23m29.6s is not all music-playing time. |

Both runs reported **zero audio callback deadline misses, invalid signal frames and master limited frames**, with6dB engine headroom. These are valuable engineering results, not a certification of rhythmic stability or musical quality. The calm set did not complete an hour and must not be reported as doing so.

Primary local evidence: [first-set review](../artifacts/luna-verified-observation-2026-10-18/first-set-review.md), [first final summary](../artifacts/luna-verified-observation-2026-10-18/final-performance-summary.json), [calm-set observation log](../artifacts/luna-calm-progressive-observation-2026-10-18/skill-observations.md), [calm stopped receipts](../artifacts/luna-calm-progressive-observation-2026-10-18/stopped-job.json), [calm final summary](../artifacts/luna-calm-progressive-observation-2026-10-18/final-performance-summary.json).

## 3. Prioritized actionable backlog

Priorities express recommended order, not authorization to implement. P0 blocks trustworthy unattended performance; P1 addresses major musical or execution quality; P2 improves efficiency and clarity.

| ID | Priority / owner | Finding | Proposed action | Acceptance evidence |
|---|---|---|---|---|
| A1 | P0 · queue/engine + control skill | A distant both-deck timer can reserve resources needed by track loading; Play can still use the old source | Reproduce ownership starvation; make source readiness causal and bind Play to the intended source/preparation result | Multi-track simulation with an hour timer: released-deck loads proceed, wrong-source Play is rejected, no unattended EOF gap |
| A2 | P0 · control + observability | Registered/running jobs conceal failed or blocked actions; recovery continued under incorrect source names | Surface failed/blocked critical steps and actual source identity; recover the active pair before adding distant plans | A failed load/launch preserves healthy audio and yields an explicit revised plan or actionable stop, not false success |
| A3 | P1 · art + library + retrieval | Same BPM/key/heuristic energy is treated as the same vibe | Represent passage character and intended next relationship; evaluate example-based audio/text retrieval | Human-rated hard negatives at the same BPM/key are separated; deliberate contrast remains possible |
| A4 | P1 · art + control + timing implementation | Phase excursions and bar errors occur despite checker/Sync success | Preserve canonical events, verify resolved anchors and sustained phase/bar relationship; investigate local-grid/Sync behavior | Near-event precision regression and real music fixtures pass; anomalies remain visible rather than averaged away |
| A5 | P1 · art + control | Repeated fader/bass templates dominate | Add a concise technique chooser with concrete supported alternatives and listening criteria | Realistic cases elicit appropriate different relationships; no compulsory technique rotation |
| A6 | P1 · control + session lifecycle | End timers use inconsistent origins, approximate re-arming and conflicting ownership | Keep one performance-clock origin and a source-independent ending intent; distinguish wall time from track position | Verified first-audible/Play origin, robust ending through source changes and honest final duration |
| A7 | P1 · art + control | Timing-valid recovery may use an arbitrary entry despite ample runway | Choose a credible future musical window when healthy audio remains; budget preparation and fallback time | Missed boundary with minutes remaining leads to a coherent revised handover, not needless immediate recovery |
| A8 | P2 · control + engine semantics | Master/Sync ownership and tempo intent drift across handovers | Compare intended tempo, local/actual/target BPM and master state; make deliberate reference ownership explicit | No unexplained cumulative rate movement; tested handoff/takeover behavior |
| A9 | P2 · library + control | Artist-name search, source variants and preparation overhead need clearer handling | Use bounded style/character evidence and source/version identity; expose pending preparation honestly | Missing literal artist names do not end discovery; loaded identity matches the selected recording |
| A10 | P2 · performance/product | Cold search remains above target in realistic startup stress | Profile foreground contention, suggestion backfill and query/index work without weakening durability | Cold and warm p90/p95/p99 meet declared budgets under dual-deck use, with complete end-to-end costs reported |
| A11 | P2 · product, optional | Advanced scratch articulation and some FX semantics are absent or unvalidated | Rehearse existing primitives first; add only demonstrated missing controls if separately requested | Listening and timing fixtures prove the claimed technique; unsupported effects remain explicitly unsupported |

## 4. Final failure: source identity and queue ownership

### What the receipts establish

In the calm set, the action named **play-trivial-break** executed at frame262874102 with A load revision9. Native samples show A still held **Be The One**, at approximately126s, while B held Noise. The intended Trivial load had not completed. The paired fade and outgoing pause subsequently executed, so the plan successfully handed control to the wrong source. Be The One later reached its end at285.048s with Noise paused.

Another proposed launch requested A source80s when A was already at126.04s and failed immediately as stale. Several later recovery/load actions were still pending when the user stopped the run. Their user-requested cancellations must not all be described as independent runtime failures.

### Source-backed mechanism; controlled reproduction still required

The source shows a credible causal chain:

1. Future channel-fader actions capture deck guards in [the native compiler](../crates/deck-engine/src/scheduled.rs).
2. Its active_ownership function includes pending guarded actions even when their renderer deadline is far away.
3. [The command queue](../src-tauri/src/control_command_queue.rs) defers business operations such as deck.load when they conflict with that ownership.
4. The source-dependent admission barrier checks a matching load in **running** state; a blocked **registered** load does not trigger that barrier.
5. A Play without a completed preparation dependency or intended-source requirement can therefore operate the old source while the proposed load waits.

The receipts and source support this explanation strongly, but no isolated regression reproduction was run during observation. Do not blame this entirely on the model or claim the engine cause is experimentally proven.

### Recommended correction

- Bind load → ready source/entry/tempo → launch → contribution → clearance as real dependencies. Action names are not source guards.
- Require the selected source identity and observed readiness/revision before admitting Play. A merely registered preparation is insufficient.
- Make long-lived set-ending intent compatible with released-deck source changes. Consider late binding to the final carrier or appropriately scoped reservations; preserve actual user takeover and relevant target/source protections.
- Surface why an action is blocked, its dependency and the resource owner. A healthy top-level running job is insufficient status.
- Reject or explain impossible dependency cycles and stale clock anchors at admission, with current clock/runway in the error.

Do not fix starvation by globally removing guards or silently resuming after manual intervention. Proposed regression: keep a one-hour end intent while repeatedly loading alternate decks, inject a failed load and manual takeover, and verify that no wrong-source launch or unexpected EOF gap can occur.

## 5. Musical timing, tempo and closing clocks

### Exact event precision and bar alignment

In the first set, the checker returned Human launch305.760009765625s at effective beat640. The submitted value was rounded to305.76001s. Native Play resolved306.2477111816406s, one beat later. The compiler's near-integer tolerance is1e-9beat before forward rounding; the approximately0.234microsecond timestamp change exceeded it. Subsequent samples had fractional beat error below0.025beats while modulo4 displacement stayed near one beat.

**Proposal:** carry canonical event identifiers/timestamps into the queued plan, compare resolved receipts with the accepted musical coordinates and reject or explicitly acknowledge a changed boundary. Evaluate a precision-tolerant or explicit-event product fix with boundary tests. Never equate fractional Sync lock with downbeat, bar or phrase alignment. Meter and bar origin must be credible before interpreting modulo-bar differences.

### Sustained phase and local grid

The first set's eighth blend reached0.41064beats sampled error. The calm set included approximately0.36756 and0.14017beat maxima, alongside successful approximately0.013–0.040beat examples. Incoming reported instantaneous BPM also varied sharply in one overlap. These values are renderer/grid-derived measurements, not independently heard tempo or proof of the cause.

**Proposal:** inspect local event spacing, source/grid quality, correction behavior and the full overlap. Use successful and failed fixtures to distinguish an imperfect source grid, a Sync implementation issue, an unsuitable rhythm pair and a bad plan. Do not hide a bad region by reducing analysis quality or loosening reporting. A checker tests coordinates; a sustained rhythmic relationship needs its own validation.

### Relative timing and recovery

Several bass moves used admission-relative after_seconds without a launch dependency while Play waited for quantized beats. Their position within the overlap shifted with the launch wait; one finished roughly11.96s after launch rather than a nominal16s. Later actions correctly used an explicit after-launch dependency.

**Proposal:** name the intended musical/renderer anchor for each stage. Use exact source events for arrangement, and launch-relative delays only when that is the desired relationship. When a boundary expires but healthy audio has substantial runway, select a new credible window rather than sacrificing bar alignment through an unnecessary immediate recovery.

### Tempo and master intent

The first set moved from approximately125 to132BPM; the calm set also showed downward target movement despite an earlier approximately120BPM preparation. Some of this may relate to local-grid interpretation and inherited rates; intentional musical tempo changes were not established for every case. The calm agent explicitly restored one carrier to120BPM, a useful corrective example.

Paused outgoing sources sometimes remained master until later loading; one interval lasted approximately134s. Playback continued. This is a semantics/intent question, not proof of a stall.

**Proposal:** distinguish base/source BPM, local BPM, target playback BPM, actual rate and musical set intent. Verify the intended playing master/follower state after clearance. Treat reference ownership as deliberate, not something inferred from a successful pause or a named promote action.

### Performance duration

The first set's ending used an inconsistent effective origin and ended approximately31s short. The calm set initially queued an exact first-Play-relative renderer ending, but it was interrupted during source preparation. A replacement admission-relative estimate would have ended approximately40s early had it survived unchanged. The user stopped the set before either ending could complete.

**Proposal:** retain one first executed Play/audible-onset origin, including the small initial fader delay when precision matters. Derive all revised deadlines from it; do not restart the hour from a new session or use the last track's source clock. Report actual duration and any silence separately. Ending should also fit the final musical statement; a correct timer alone cannot certify a coherent closing phrase.

## 6. Musical character and “more songs like this”

### The core requirement

The user reports a driving120BPM passage followed by a vocal/emotional120BPM song that felt wrong for the intended direction. No exact pair/time was supplied for that complaint; do not attach guessed identities to it. Vocals are not inherently low-energy, and emotional music can be driving. The missing judgment is whether the next passage preserves or intentionally changes the requested character.

Current [suggestion scoring](../src-tauri/src/suggest.rs) weights key/tempo/energy0.45/0.35/0.20. Its waveform energy proxy combines loudness and brightness0.65/0.35. This is useful technical evidence, but it is not measured emotion, vocal prominence or semantic vibe. Similar scores cannot justify claims of emotional continuity.

### Proposed passage evidence

| Dimension | Useful distinction | Evidence to retain |
|---|---|---|
| Intensity/arousal | Gentle, restrained, driving, forceful; independent of BPM | Listener ratings, calibrated predictions and contributing acoustic features |
| Emotional character | Warm, tense, euphoric, wistful, dark, playful, mixed or unknown | Contextual human descriptions/model hypotheses, with disagreement |
| Vocal role | No known voice, background fragment, lead phrase; whether a lyric must finish | Heard annotation or evaluated passage tagging; voice detection alone is insufficient |
| Groove | Swing/straight feel, syncopation, kick/backbeat relationship, density | Local timing plus listening, not just average BPM |
| Timbre/production | Dry/wet, bright/dark, rough/smooth, dense/open, bass weight | Heard descriptors, acoustic evidence and evaluated representations |
| Arrangement/tension | Building, holding, resolving, releasing; what follows the entry | Source spans/events, provenance and uncertainty |
| Desired next relationship | Continue, intensify, ease, answer or deliberately contrast | User/set intent, not a fixed nearest-neighbor preference |

Keep measured, human-heard, user preference, model-predicted, inferred and unknown evidence distinct. Whole-track summaries help coarse discovery; passage evidence is needed for a breakdown inside a driving song. The [DEAM authors' manual](https://cvml.unige.ch/databases/DEAM/manual.pdf) illustrates temporal annotation and uncertainty; its task is expressed emotion, not a universal measure of what each listener feels.

### EmbeddingGemma 2: a credible candidate, not a selected solution

The user's [official Google launch page](https://blog.google/innovation-and-ai/technology/developers-tools/embeddinggemma-2/) confirms that EmbeddingGemma2 supports audio alongside text and visual inputs. The [official model card](https://ai.google.dev/gemma/docs/embeddinggemma/model_card_2?hl=en) documents a shared768-dimensional space, modular encoders, mono16kHz audio and an approximately327s audio-only context limit. Its general retrieval benchmarks do not establish DJ-vibe matching. The original EmbeddingGemma version is text-only; do not apply that limitation to version2. No local speed, memory or quality claim has been measured here.

| Evaluation candidate | Potential role | Main uncertainty |
|---|---|---|
| Reliable descriptions + text embeddings | Semantic search over human notes, tags and supported descriptions | A title/BPM record does not supply unheard acoustic character; generated captions can be wrong |
| EmbeddingGemma2 audio/text | Local example-based and natural-language retrieval | General audio representations may emphasize attributes other than the desired musical facet |
| Music-oriented CLAP checkpoint | Joint text/audio retrieval; [authors' repository](https://github.com/LAION-AI/CLAP) | Checkpoint-specific preprocessing, domain bias and actual sequence relevance |
| MERT or similar music representation | Audio features or evaluated task heads; [author implementation](https://github.com/yizhilll/MERT) | An audio backbone is not automatically a text retriever or interpretable emotion score |
| Explicit mood/voice task models | Complementary interpretable predictions; [Essentia catalog](https://essentia.upf.edu/models.html) | Domain generalization, label meaning and deployment licensing require evaluation |

[MuLan](https://research.google/pubs/mulan-a-joint-embedding-of-music-audio-and-natural-language/) supplies research precedent for music-language alignment; this is not a claim that its trained model is an available drop-in dependency. Compare a small candidate set first, rather than shipping a platform of multiple model backends.

### Proposed retrieval flow

1. Seed from a selected track or useful passage; optionally refine same driving groove, similar emotional tone, similar texture or less lead vocal.
2. Retrieve a bounded shortlist from cached source-bound embeddings and reliable descriptions within the requested collection/history/exclusions.
3. Re-rank for the intended direction, source availability and explicitly supported character evidence. Allow deliberate contrast when requested.
4. Check beat/grid/key/tempo/meter, source identity, readiness and actual entry/arrival separately. Similar music is not automatically a viable next transition.
5. Return the matched facets, passage, meaningful uncertainty and tradeoffs. Cosine similarity is not a percentage probability of vibe compatibility; do not invent explanations from an opaque vector.
6. Use feedback to refine the current relationship or an authorized saved preference. Do not turn a rejection of one vocal contrast into a ban on vocals.

Cache derived features by source fingerprint, model/preprocessing version and passage range. Preserve existing audio analysis, tags, edits and cues. Model indexing belongs outside the audio callback and live critical preparation; imports should remain fast, incomplete semantic coverage should be explicit, and no whole-library inference should silently begin near a handover deadline. Verify actual Spotify audio/metadata access and readiness before promising equivalent cross-source embeddings; only local playback was established in these observations.

### Model decision gate

Use a held-out set of real passages with the user's and musically informed raters' judgments. Include same-BPM/key hard negatives, different-BPM character positives, driving vocals, sparse intros hiding driving main sections and intentional emotional releases. Group remix/artist duplicates appropriately to avoid leakage. Compare current scoring, reliable text notes, audio vectors and combined evidence on the same spans and exclusions.

Measure shortlist relevance/nDCG or pairwise preference, per-style error, rater disagreement and actual next-passage suitability separately. Then listen to blind before/during/after mix excerpts. Compare whole-track and passage pooling. A model wins only if it improves the user's task while meeting resource/performance constraints; a general benchmark ranking is insufficient.

## 7. A broader repertoire with honest control mappings

These are proposed technique cards, not instructions executed in either live set. Musical examples are hypothetical, not transcriptions of named DJs. Every choice still needs a purpose, source evidence, protected statement, desired arrival, complete overlap, reset and recovery.

| Family | When it can serve the music | Example relationship | PulseDeck mapping / boundary |
|---|---|---|---|
| Developed percussion/texture blend | Complementary sparse layers can evolve gradually | B's percussion supports A's completed motif, then B becomes foreground | Native Play and fader/EQ/filter ramps; a conspicuous bass swap is optional if arrangement already makes room |
| Short section exchange / cut | Dense ideas need separate space or quicker pace is requested | Finish A's line, carry a known B pickup and exchange at its arrival | Exact launch and channel exchange; protect pickups/lyrics and avoid accidental silence |
| Drop/payoff replacement | B can satisfy the buildup's promised impact | A's buildup resolves into B's prepared full-weight arrival | Actual arrival arithmetic, readiness and levels; a quiet intro is not a substitute payoff |
| Drop-to-breakdown / atmospheric bridge | Deliberate release or mood/tempo turn is desired | A resolves into B's sparse texture with a known rhythm return | Fader/EQ/filter/supported FX; explicitly plan the release rather than call it continuous drive |
| Filter reveal / withdrawal | A useful texture can emerge from darkness or recede in depth | Open B's low-pass texture as A's foreground resolves | Bipolar filter ramps; negative LPF, positive HPF, neutral0; no exposed resonance or arbitrary cutoffHz |
| EQ / foreground call-and-response | Actual motifs have gaps and complementary spectral roles | A's sentence resolves; B's stab answers while A's top rhythm supports | Independent mid/high/low ramps and channel control; three bands are not stems or vocal isolation |
| Loop-supported development | A complete clean unit can support an evolving B | Repeat a percussion unit, then release/clear on a selected repetition | Beat/manual loops and explicit exit; native timed launch rejects active loops on either deck |
| Loop reduction / tension | Sparse material tolerates a brief rhythmic intensification | Tighten a clean drum unit once, then release into B | Changing set_loop chooses a new region from a floored beat; no assumed Slip/roll or same-start halving |
| Tease / motif answer / double | Recognizable compatible material can create a brief deliberate relationship | A short B motif answers A, withdraws and later returns fully | Known cue/seek while isolated, launch and role shaping; two decks limit simultaneous independent layers |
| Echo/delay accent | A clean final hit/word has space for repeats | Let a restrained repeat answer over B's dry rhythm | Echo amount and beat length; FX precede channel/crossfader, so closing the channel closes the tail |
| Reverb / atmospheric accent | A sparse accent benefits from depth or a release | A recedes while B establishes dry proximity | Simple fixed-character reverb amount; no room/decay/predelay or independent wet return |
| Rhythmic/tempo contrast | A complete boundary supports a different pulse or deliberate ramp | Resolve one groove into a clear pickup or beatless bridge | Tempo/BPM/Sync/keylock controls; no dedicated half/double Sync ratio and no automatic compatibility |
| Scratch phrase | A known sound can answer in a real rhythmic gap | One short articulated response over a healthy carrier | Genuine signed jog/scrub exists; complex articulation requires separate rehearsal and listening |

Primary instruction behind these families includes [Sara Simms on transitions](https://blog.native-instruments.com/dj-transitions/) and [creative EQ](https://blog.native-instruments.com/dj-tips/), named practitioners' accounts of [spectral roles, motifs and exits](https://blog.pioneerdj.com/djtips/here-are-8-pro-level-dj-tips/), and [Steve Angello on loops/arrangement](https://blog.pioneerdj.com/djtips/steve-angello-tells-us-about-his-favorite-dj-gear-and-techniques/). Their hardware setups and genre habits are not automatically PulseDeck features or universal rules.

### Two practical design examples to evaluate later

**Filter and foreground reveal:** A has a complete lead phrase and open supporting rhythm. Introduce a darkened B texture, open its useful upper rhythm, attenuate the competing A midrange only after its sentence, then let B become foreground before clearing A. Keep one convincing foundation throughout. Listen for an unintentionally muffled passage, snare/body loss, two competing leads and filter-related level changes. If it becomes thin, restore the verified role rather than boost everything.

**Creative drop-to-breakdown contrast:** Identify A's actual resolved drop statement and B's real sparse entry, its duration and the following pulse return. Decide whether the listener should feel release, tension or continuity. If uninterrupted drive is wanted, retain appropriate support until B has useful rhythm/body; if release is intended, let the space be deliberate. Do not infer the first drop or a vocal-free gap from a waveform spike alone. Faster pacing does not require chopping a protected lyric or using every track's first drop identically.

For the earlier lively brief,8–16bars was a useful starting range. It is not a universal duration, and silent pre-roll is not audible development. Fader-ramp length, audible overlap, bass exchange and final clearance can have different musical spans. Actual meter, pickups and irregular phrases determine the intervals.

## 8. Scratching and FX: supported primitives versus proven performance

The [offline capability audit](../artifacts/dj-technique-research-2026-10-18/capabilities.md) inspected actual DSP and public commands. Signed scratch motion is implemented: jog with scratch enabled, jog_touch, scrub_to and scrub_end traverse source audio rather than merely seeking. This corrects the potential misconception that scratching is wholly unavailable. It does not prove that fast routines sound right.

Recommended progression for later isolated evaluation:

1. **Foundation:** baby forward/back motion and clean release/return to a known sound; preserve an independent healthy carrier.
2. **Articulation:** forward/cut/stab, then chirp/transformer/tear shapes with separately timed movement, gates and rests.
3. **Advanced:** flares, crabs, boomerang and combinations only after motion, gating, carrier level and cleanup are audibly verified.

[DJ Angelo's own instructional series](https://www.reloop.com/news/new-videos-learn-how-to-scratch-with-dj-angelo-473-article), [DJ Shortee's tear lesson](https://www.djshortee.com/learn-the-reverse-triplet-tear-3-top-tips-for-tearrific-tears-scratch-dj-tutorial/), [Emma Short-E's gate explanations](https://studioscratches.com/how-to-learn-from-a-hamster-reverse-crossfader-style-scratch-dj-if-you-scratch-regular/) and [Qbert's foundational prerequisites for boomerang](https://blog.artistworks.com/qbert-skratch-university-boomerang-scratch/) support learning movements/articulations progressively. The exact rehearsal ordering above is a proposal, not a claim of one canonical school curriculum.

Important product boundaries:

- Scrub trajectories need native sequencing on a stable independent clock. Model/tool calls must not time individual strokes. A scratched source clock is not monotonic.
- Faders/EQ/filter/FX are smoothed, approximately8ms time constant. The crossfader has a fixed constant-power law, not an adjustable battle cut curve, and affects both decks. No exposed cut-in/assignment/bypass or scratch-envelope macro is established.
- Scrub completion and a musical gate are different events. A gate chained only after motion completion cannot articulate the moving stroke. Release touch and restore intended state explicitly.
- PulseDeck FX precede channel/crossfader attenuation. An independent post-fader echo/reverb tail is unavailable. There is no exposed independent feedback/freeze/send control or native FX-amount ramp target.
- Timed native launches reject an active loop or held jog on either participating deck. Ordinary loops are not Slip/roll; repeated seeks are not scratching.

Basic supported filter/EQ/foreground techniques should not wait for advanced scratch work. Optional future features—scratch cut curves/trajectories, independent FX returns or loop-clock launch support—need their own demonstrated need and regression burden. Do not add them just to enlarge a feature list.

## 9. Proposed changes to the three skills—still proposals

Existing references already warn about harmony uncertainty, unknown vocals and musical success versus telemetry. Duplicating those warnings is unlikely to fix repeated behavior. Improve decision-changing examples, source-bound outputs and realistic validation instead.

| Skill | Narrow proposed improvement | Keep out of the entrypoint |
|---|---|---|
| djing-art | Separate tempo/harmony/groove/character judgments; state the intended next relationship; provide a concise purpose-to-technique chooser and contrasting successful/failed cases | Long theory catalogs, mandatory technique quotas, universal8–16bar recipes or claims that key-wheel movement guarantees energy |
| pulsedeck-library | Return bounded passage evidence, source/version identity, character provenance, actual motif/loop/scratch candidates and unknowns; distinguish scoped suggestions from whole-library evidence | Invented vocal/section labels, unrequested durable cue/tag edits, inferred audio character from titles alone |
| pulsedeck-control | Bind preparation to source identity, preserve exact event coordinates, verify resolved bar/phase relationships, make recovery runway and performance clocks explicit, map techniques to real primitives | One wrapper per technique, duplicated schemas, model-timed fader loops or instructions that paper over queue defects |

Use optional technique/scratch references through progressive disclosure. Retain all working controls and existing sources. A harmony naming error occurred even though the reference already correctly distinguishes relative from parallel keys; this needs a behavioral test and better evidence use, not another identical paragraph.

## 10. Performance and quality constraints

These are proposed gates for later authorized work, not achieved results:

- No loss of existing import/deduplication, metadata/My Tags, filters, editing/XML, cues, analysis, Spotify, MIDI, playback or takeover features. Preserve source/version identity and user edits.
- Audio, beat/key analysis and persistent caches keep their quality. Semantic enrichment supplements them; it does not overwrite measured timing or rerun audio analysis unnecessarily.
- Keep search and browsing bounded/paged/virtualized. A semantic shortlist must not hydrate full waveforms or the entire library for scrolling.
- For a representative100k-track workload, retain ambitious warmed retrieval budgets such as p90≤25ms, p95≤50ms, p99<100ms, and require a separately reported cold end-to-end target below100ms. Report model/indexing latency, app readiness and process-launch overhead honestly; do not exclude user-visible startup work merely to claim a pass.
- Prior [cold startup stress](../artifacts/cold-search-2026-10-18/cold-query-summary.json) still showed approximatelyp90=126ms, p95=136ms and p99/max=166ms across40 queries. This was a fresh-process103k fixture with startup work, not this live set's search latency. Pure-query benchmarks being faster do not close that gap.
- Run model extraction/index maintenance outside the audio callback, prioritize current-deck preparation and pause/cancel background work when needed. Measure actual CPU, memory, disk and source-change invalidation on the target Mac.
- Verify dual-deck playback plus browsing, tag editing/filtering, inference/index work and waveform rendering together. Zero overruns and no feature/analysis-quality regression are required gates. A single unfocused UI snapshot or fast paint metric does not certify120fps perception on the actual display.

Before choosing embedding dimensions, quantization or approximate search, compare retrieval quality with the full candidate representation. Compression is a measured tradeoff, not an automatic quality-free optimization.

## 11. Validation plan and recommended order

| Case | Evidence required |
|---|---|
| Future hour timer plus repeated alternate-deck loads | No starvation, no wrong-source Play; source identities/revisions and final clock verified |
| Load failure or registered-but-blocked preparation | Play cannot fall through to the previously loaded song; healthy carrier remains available |
| Near-exact neural event rounded above/below a boundary | Compiler/checker agree or explicitly report changed resolution; bar alignment retained |
| Actual phase-anomaly passages versus successful controls | Local-grid/Sync cause isolated; sustained phase/bar traces and audio checked |
| Missed boundary with ample versus exhausted runway | Credible revised musical plan in the first case; honest bounded recovery in the second |
| Source load/manual takeover during final timer | Relevant ownership preserved, timer state explicit, real human intervention never silently undone |
| Same BPM/key but different character | Human-rated character and next-passage relevance improve; driving vocals and deliberate contrast remain valid |
| Repetitive blend history with a useful texture/motif gap | Appropriate alternative filter/EQ/foreground relationship, not arbitrary FX rotation |
| Dense vocals, sparse destination or irregular pickup | Complete semantic statement and real arrival relationship, with unknowns retained |
| Loop, FX and scratch requests | Actual supported signal path, legal launch prerequisites, intended reset and audibly clean result |
|100k library under dual-deck use | Cold/warm query percentiles, UI responsiveness, no overruns and preserved metadata/analysis |
| Human stop and final reporting | Immediate stop, no restart, no active jobs, exact outcome and uncertainty rather than a generic success claim |

Recommended sequence:

1. Reproduce and correct the P0 queue/readiness/source-identity path before another unattended set. Build a small deterministic regression from these receipts.
2. Investigate event precision, local-grid/Sync anomalies and performance-clock ownership with fixtures. Preserve all protections and quality.
3. In parallel research/design work, define the user's character judgments and compare a small retrieval candidate set. No production model selection until human evaluation and performance data exist.
4. Propose the smallest three-skill changes using the repertoire and concrete failures; validate them with independent realistic scenarios rather than tests matching instruction wording.
5. Rehearse supported techniques in an isolated, intentionally routed environment if separately authorized; then listen to controlled live transitions and entire set excerpts. Advanced scratching and optional product controls have separate gates.

No step above has been implemented as part of this document.

## 12. Source quality and remaining unknowns

Core craft literature: Brewster/Broughton's [How to DJ Right, Grove publisher/excerpt](https://groveatlantic.com/book/how-to-dj-right/) and [How to DJ (Properly), Penguin publisher edition](https://www.penguin.co.uk/books/333746/how-to-dj-properly-by-frank-broughton-and-bill-brewster/9780593058114). They provide established craft context; publisher summaries do not authorize attributing unread chapters. Practitioner instruction above is used for specific technique principles, not fame-based guarantees. Manufacturer tutorials establish examples; source inspection determines PulseDeck's different capabilities.

Research support: [annotated bibliography](../artifacts/dj-technique-research-2026-10-18/sources.md), [technique research](../artifacts/dj-technique-research-2026-10-18/research.md), [capability audit](../artifacts/dj-technique-research-2026-10-18/capabilities.md), [musical-character research](../artifacts/dj-technique-research-2026-10-18/musical-character-and-retrieval.md). These are supporting evidence for this single review, not separate implementation requests.

Open questions: exact causal reproduction of ownership starvation and phase excursions; physical audibility of the sampled timing errors; the specific pair behind the user's vibe complaint; trustworthy passage labels; current Spotify-specific readiness/feature coverage; realistic scratch articulation; model quality and local inference budgets. None should be filled with invented certainty.

**Deliverable status:** compiled research and actionable proposals only. The performing agent remains stopped; the observer remains paused. Skills, application code, models, metadata and analysis were not changed by this compilation.

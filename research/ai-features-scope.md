# AI analysis features: scope

Proposed 14 October 2026. Nothing below is implemented. This scopes three features: beat and downbeat tracking, song sections with automatic cues, and next-track suggestions. Effort figures are rough estimates for one developer and are not commitments.

## Decisions

| Question | Decision (14 October 2026) |
| --- | --- |
| Sources | Local files **and** Spotify tracks receive model analysis. Spotify's developer policy prohibits ingesting Spotify content into ML/AI models; the owner accepted that account risk. A Settings switch turns model analysis off for Spotify sources without affecting local files. |
| Automatic cues | Both: memory cues mark every section start; empty hot-cue pads are filled with the main sections. |
| Genres | House, hip-hop, EDM and other electronic music. Test sets and section labels cover all four. |
| App size | The sound-similarity model is an optional download. Feature 1 adds about 25 MB (runtime about 17 MB, model 8 MB); the owner accepted this so beat analysis works offline from first launch (updated 14 October 2026 from the original 10 to 20 MB). |

## Shared foundation

**One entry point.** Local imports (`analyze_file`) and Spotify page or deck analysis (`analyze_decoded_audio`, [lib.rs:935](../crates/deck-engine/src/lib.rs)) both call `decode::analyze_pcm` ([decode.rs:257](../crates/deck-engine/src/decode.rs)). Models added there cover both sources.

**Model runner.** Add inference to `crates/audio-analysis`. The [spike](beat-model-spike.md) chose tract, which is pure Rust: about 0.7 s per 30 s chunk on 4 threads and 325 MB peak with the grouped-attention export. ONNX Runtime's CPU backend was about 1.6 times faster but needed more memory and a large native library; its CoreML backend was about 6 times slower and used more than 3.5 GB. Inference never runs in the render callback; it stays in the existing import and analysis workers, and deck loads keep priority over batch work.

**Version 4.** `ANALYSIS_VERSION` moves from 3 to 4 ([lib.rs:13](../crates/audio-analysis/src/lib.rs)). New `TrackAnalysis` fields use `#[serde(default)]` and persist through `tracks.analysis_json` and `spotify_analysis.metadata` without a table migration.

**Compatibility.** The Spotify cache accepts only versions 3 and 2 ([library.rs:116](../src-tauri/src/library.rs), [library.rs:126](../src-tauri/src/library.rs)). Change this to accept versions 2 to 4 for display and treat anything below 4 as upgradeable; otherwise the bump discards every cached Spotify analysis. Upgrade paths:

- Local files: a cancellable "Update library analysis" job. Import alone does not do this, because it only analyzes paths the user imports ([main.rs:335](../src-tauri/src/main.rs)).
- Spotify: upgrade when a track loads onto a deck (its PCM is already in RAM), or when **Analyze page** runs, which re-streams stale entries under its existing 50-track, cancellable limit.

## 1. Beat and downbeat tracking

**Model.** [Beat This!](https://github.com/CPJKU/beat_this) (JKU, 2024) predicts beats and downbeats. Its code and published weights are MIT licensed. The small checkpoint is about 8 MB and can ship inside the app. It is not distributed as ONNX; export is part of the spike.

### Spike: done, go (14 October 2026)

Results are in [beat-model-spike.md](beat-model-spike.md). On six tracks, Rust reproduces the Python reference: beat F-measure 0.96 to 1.00 at ±70 ms and downbeat F-measure 0.95 to 0.97 at ±20 ms. The model's grid lines up with kick onsets on every track. Analysis takes about 1.8 s per minute of audio on 4 threads, with about 250 MB of transient memory. The app grows by about 25 MB, which the owner accepted. Spike code: [tools/beat-model-spike](../tools/beat-model-spike/README.md).

### Build

- Store `beats_seconds` and `downbeats_seconds` with the analysis. Sections use them, and a future variable-tempo grid can too.
- Fit the existing constant grid: BPM by least squares over the model's beats, offset placed on a downbeat. The waveform's four-beat lines and bar counter derive from the offset ([Waveform.tsx:342](../frontend/src/Waveform.tsx), [waveform-data.ts:178](../frontend/src/waveform-data.ts)), so they become true bar lines. The engine is unchanged: loading still sends `SetBpm` and `SetBeatgrid`.
- Provenance: `bpm_source` and `beatgrid_source` become `"model"`. Manual BPM and grid edits keep precedence through the existing `upsert` rules ([library.rs:187](../src-tauri/src/library.rs)).
- Keep the DSP tempo estimate as a cross-check. When the model and DSP differ by a factor of two, keep both as candidates and set `bpm_ambiguous`. Half-time hip-hop and trap are the expected cases.
- Grid editor: a "beat 1" shift that moves the offset by one beat in either direction.
- Analysis popover: replace the "not a verified musical downbeat" wording ([AnalysisInfo.tsx:85](../frontend/src/AnalysisInfo.tsx)) when the source is the model.
- Settings: the Spotify model-analysis switch.

### Build status (14 October 2026)

Implemented, with tests passing (workspace Rust tests, strict Clippy, 41 frontend tests):

- `audio-analysis::track_beats`: bundled model (8 MB), log-mel front end checked against torchaudio values, upstream chunking and peak picking, constant-grid fit, four-thread utility-priority pool, one inference at a time, cancellation between chunks.
- A separate `beat_model` stage on each analysis, versioned by `BEAT_MODEL_VERSION`. This replaces the planned `ANALYSIS_VERSION` bump: version 3 analyses stay valid, so the Spotify cache needed no compatibility change and nothing is re-analysed needlessly.
- Merge rules (`deck_engine::apply_beat_model`): a usable model grid replaces the DSP or tag estimate, the previous BPM stays as a candidate, half/double or tag disagreement beyond 2% sets `bpm_ambiguous`, and manual BPM and grid edits stand. A manual BPM keeps its grid unless the model's pulse matches it or its half/double.
- Import and **Settings → Library → Update library analysis** run the model, with progress and cancel. Local deck loads now reuse the stored analysis instead of re-running DSP on every load.
- A track loaded without the stage is analysed in the background after loading. Its grid reaches the deck immediately unless that deck is playing while any deck has sync on; then it applies on the next load. User-initiated Reanalyze applies at once.
- Spotify: deck loads and **Analyze page** run the model (page analysis upgrades cached entries in place, keeping manual edits), behind a persisted switch in Settings → Library (`analysis.json`).
- Grid editor: "BEAT 1" offset with buttons that move beat 1 a beat earlier or later. The analysis popover explains model grids and flags "check beat 1" when bar agreement is below 50%.
- Evaluation: `tools/export_beat_annotations.sh` and the `beat_eval` example, local files only.

Desktop check (14 October 2026, debug bundle):
- Settings → Library shows the Beat analysis and Spotify cards, and the Spotify switch writes `analysis.json` both ways.
- Loading the melodic drill beat showed the DSP result first; the background model then set the deck to 142.02 BPM with a "bpm ½ / 2×?" flag, beat 1 at 0.131 s, and the stage saved to the library.
- The popover shows the model wording, rhythm support 85/100 and candidates 142.02 / 70.99.
- The beat-1 buttons moved the offset 0.131 → 0.554 → 0.132 → 1.399, wrapping within the bar.
- **Update library analysis** showed "Cancel (0/1)" and moved the second drill beat from 75.01 to 149.99 BPM (99% of beats on grid).

Remaining before feature 1 counts as done: the 32-track annotated evaluation (the ship gate) and a library-table refresh after background analysis (the table shows the old BPM until the next refresh).

**Out of scope.** Variable-tempo grids (beats are stored for later), bar-aligned sync (sync remains beat-phase only, [dsp.rs:908](../crates/deck-engine/src/dsp.rs)) and meters other than 4/4.

### Acceptance

- A 32-track test set: 8 house, 8 hip-hop, 8 EDM and 8 other electronic, including Spotify tracks and hard cases: long beatless intros, breakbeats, swung hip-hop, live drums and half-time trap.
- Per track, hand-annotate the BPM and one clear downbeat using the grid editor. Annotations reference private paths and stay with local evidence, not in the repository.
- Report per genre group: BPM within 2% (half/double errors counted separately), downbeat correct, and median grid-to-beat deviation.
- Ship only if the model matches or beats the v3 DSP on downbeat accuracy in every genre group and is no worse on BPM overall. The analysis-time target is set from spike measurements.

**Effort:** about 3 weeks including the spike.

## 2. Song sections and automatic cues

Depends on feature 1. Version 1 uses rules on top of downbeats, not a new model: open structure models are mostly pop-oriented, and some require stem separation first.

### Detection

1. Per-bar features from the downbeats: low/mid/high energy and a pitch-class (chroma) profile for repetition. This needs PCM, so it runs inside `analyze_pcm`; its cost is small next to the model.
2. Boundaries where energy or repetition changes, snapped to 4, 8, 16 or 32-bar lengths.
3. Labels from one of two vocabularies:
   - **Dance** (house, EDM, electronic): intro, build-up, drop, breakdown, outro. Used when a clear build-up into a drop exists.
   - **Hip-hop**: intro, verse, hook, outro. The most-repeated section is the hook.
   - Unclear sections stay unnamed. A per-track override in the analysis popover switches vocabulary.

### Automatic cues

- **Memory cues:** every section start, up to ten, in empty slots only.
- **Hot cues:** empty pads only, up to eight, chosen by priority (first downbeat, drops or hooks, breakdowns or verses, build-ups, outro) and then ordered by time.
- Record which slots were auto-filled, with their times, in the analysis metadata. Reanalysis replaces only auto slots that are still unchanged. A cue the DJ moved, deleted or set becomes manual and is never touched.
- Settings: memory and hot auto-cues toggle separately (both on by default).
- Local tracks: write auto cues into the library record when analysis completes, so `load_into_deck` restores them ([main.rs:438](../src-tauri/src/main.rs)).
- Spotify tracks: cues are not persisted today ([spotify_decks.rs](../src-tauri/src/spotify_decks.rs) restores none), so auto cues come from the cached sections at each load. Persisting manual Spotify cues is a separate gap.

### Interface

- Coloured section bands with labels on the overview and detailed waveforms. The colours are tokens in `frontend/src/styles/tokens.css` and must stay distinct from the blue/orange/white bands and the cue colours.
- A "bars to next section" countdown near the playhead.

### Build status (14 October 2026)

Implemented, with workspace Rust tests (130), strict Clippy and 42 frontend tests passing:

- `audio-analysis::detect_sections`: reuses the beat model's log-mel spectrogram, so it adds well under a second per track. It works from per-bar timbre, level, bass and bass-onset features. Phrase-aware dynamic-programming segmentation joins edge fragments under four bars into their neighbours. Labels: dance (intro, build-up, drop, breakdown, outro) or hip-hop (intro, verse, hook, outro), with "groove" from bass level and bass prominence so noise risers do not read as drops. Sections are cut only from model or manual grids. `BEAT_MODEL_VERSION` is now 2 (beats plus sections), so tracks analysed before sections existed are updated on load or by **Update library analysis**.
- `plan_auto_cues` and app wiring:
  - Fills empty hot cue pads in priority order and adds memory cues at every merged section start.
  - Records what it placed, so untouched auto cues move with new sections, cues the DJ set or moved are kept, and cleared auto cues stay gone.
  - Applied at import and library update for stored tracks, and on the deck after load, background analysis, reanalysis and manual grid edits (sections are re-cut on the new grid within a quarter second).
  - Spotify tracks get fresh auto cues at each load.
  - Settings → Library → **Automatic cues** has separate hot and memory switches.
- Interface: a colour strip of sections on both waveforms, section names on the detail view beside any hot cue flag, a "DROP in 8" countdown beside the bar counter, and a section list with a Dance / Hip-hop label switch (`set_section_vocabulary`) in the analysis popover. Neighbouring sections with the same label display and cue as one.

Real tracks (no hand annotation yet): Detroit Swindle cut into 11 sections with a long intro, build-ups, drops, a short breakdown and an outro. FILV: intro, drop, breakdown, drop, outro. Kendrick Lamar "Prayer" alternates verse and hook. A melodic drill loop: intro, a 64-bar main section, outro. In the running dev app, a Spotify extended mix showed seven auto hot cues A to G on section starts, the section strip, labels and countdown.

Not yet exercised in a running app: the vocabulary switch and the section refresh after a manual grid edit (both use unit-tested parts). Remaining before feature 2 counts as done: the 24-track section evaluation.

**Out of scope.** Vocal detection (needs stems), mood or energy labels, manual section editing (a likely v1.1) and vocabularies for other genres.

### Acceptance

- 24 tracks (6 per genre group) with hand-marked section starts. Boundaries within one bar; label agreement reported per vocabulary.
- In a practice review, the DJ keeps at least 80% of the automatic cues.

**Effort:** about 3 weeks after feature 1. This is up from 2 because of the hip-hop vocabulary, repetition analysis and two cue types.

## 3. Next-track suggestions

### Version 1: scoring without a model

- **Key:** Camelot compatibility (same key, one step, A/B switch), weighted by key confidence.
- **Tempo:** percentage BPM distance with half/double matching, limited by the deck's tempo range. Distance naturally separates hip-hop (about 85 to 100) from house (about 120 to 128).
- **Energy:** a new per-track score from the stored waveform bands; computed lazily for version 3 records. A control chooses keep, raise or lower energy.
- **Exclusions:** tracks played this session (in memory for now) and the track on the other deck.
- **Reasons:** a one-line explanation built from the scores, for example "8A→9A · +1.5% · energy up". No language model.
- Suggests for the audible deck and offers loading into the other deck. The pool is local tracks plus Spotify tracks with cached analysis. Spotify's own recommendation endpoints are unavailable to new apps.
- Implementation: a new `src-tauri/src/suggest.rs` with `suggest_next(deck, energy_direction)` returning the top 20. A **Next** view in the library drawer, reachable from the browse knob ([controller-browser.ts:15](../frontend/src/controller-browser.ts)).

### Version 1 build status (14 October 2026)

Implemented, with workspace Rust tests (145), strict Clippy and 48 frontend tests passing:

- [suggest.rs](../src-tauri/src/suggest.rs): `suggest_next(deck, energy_direction)` returns the top 20 for the audible deck, or for a given deck, to load into the other deck. The audible deck is the louder playing deck after its channel fader and the constant-power crossfader curve, the master when both are about as loud, and otherwise the loaded deck with its faders up.
- Scoring, weighted 45% key, 35% tempo and 20% energy:
  - **Key:** same key 1.0, one step around the wheel 0.9, A/B switch 0.85; anything else is a clash at 0. When either key confidence is below 0.6 the fit moves towards neutral in proportion. Unknown keys score neutral and read "key ?".
  - **Tempo:** the change the other deck needs, as a percentage of the heard BPM (the deck's BPM times its tempo). Half and double time are tried too; they cost 5% when either track is flagged `bpm_ambiguous` and 30% otherwise. Tracks the other deck's tempo range (±6, 10, 16 or 100%) cannot reach are left out and counted.
  - **Energy:** a new per-track score from the waveform bands: 65% the loudness of the louder half of the track (its main sections, so long intros and breakdowns do not lower it) and 35% brightness (the high band's share). It shows as a level from 1 to 10. Keep, raise and lower prefer no change, about one level up, or about one level down. On the owner's library the house tracks scored 7 to 8, the drill beats 6 and the quieter tracks 5 to 6. It is a heuristic until the practice review.
  - Feature 2's section `energy` is relative within each track, so it cannot compare tracks and is not used.
- **Exclusions:** both decks' tracks and tracks played this session. A track counts as played after 30 seconds of audible playback (fader and crossfader open); previewing it in headphones does not count. The list lives in memory, and **Forget played** clears it.
- **Reasons** come from the scores, for example "8A→9A · +1.5% · energy up", "2A→2A · ±0.0% half-time · same energy" or "8A→3B clash · +0.3%". Every clash is named in the reason and marked on its key tag, and clashes rank below compatible keys of similar tempo and energy.
- **Speed:** features live in a compact `suggest_features` table that every library and Spotify-cache write keeps current, with a copy in memory, so a request never parses `analysis_json`. Older records get features from a background pass started by the first request, 16 at a time and parsed outside the library lock; the list says how many are still being prepared. In a debug build, 10,000 synthetic tracks took 9.6 ms per request after a one-time 28 ms load.
- **Interface:** a **next** key in the library tray opens a Next view in the drawer. The left pane chooses keep, raise or lower energy. The right pane lists title, reason, BPM, key, energy level and a load key for the other deck; rows also drag onto either deck. The FLX4 browse knob reaches it like the other sources: turn on the tray to pick next, press for the energy pane, press again for suggestions, and LOAD A or B loads the selected one (still refused on a playing deck). Below 1100 px window width the BPM column hides, as the local table hides its fourth column.
- **Tests:** Camelot rules, confidence weighting, half/double time, tempo range, energy direction, the energy score, exclusions, reason text, the audible deck, the played history and clash ordering; feature upkeep on every library write, the background pass for older records, and two 10,000-track benchmarks (ranking alone and through the stored library). Frontend helpers: the refetch key, energy stepping, summaries, empty-state messages and drag data.

Checks so far (14 October 2026):
- **Real library.** An opt-in test (`PULSEDECK_LIBRARY=<copy of library.sqlite3> cargo test -p pulsedeck -- --ignored --nocapture real_library`) ranked every track of a copy of the owner's library: 2 local drill beats and 14 cached Spotify tracks. The lists were sensible. "How Long Have You Been Away? (Extended Mix)" (123 BPM, 10A) got "It's Over Now (Extended Vocal Mix)" first ("10A→10A · +0.8% · energy up"), and the two drill beats suggested each other ("2A→1A · -5.3%"). Within ±6% the hip-hop tracks at 82 to 97 BPM never reached the house tracks at 118 to 127; "Do Not Disturb" (85.5 BPM) and "Drift" (82 BPM) suggested each other. Every clash in the lists was labelled.
- **Web layout.** In the browser, against the running Vite server, the **next** key, the drawer, the energy pane and the empty state render in the house style.
- **Desktop app** (debug bundle, master output muted, 14 October 2026):
  - With a drill beat on deck A, the Next view showed "following A · 142.0 bpm · 2A · energy 6 → loads into B (±6%)" and the other drill beat with "2A→1A · -5.3% · same energy". The footer read "1 in reach · 13 out of tempo range · 0.7 ms".
  - The suggestion's load key loaded deck B, and the list immediately changed to the out-of-range message; that request took 1.0 ms. With deck B at ±16%, six cached Spotify tracks appeared, with three clashes marked in red.
  - **raise energy** moved "Choir To The Wild" (energy up) to the top.
  - The keyboard path the browse knob uses (arrows and Enter) moved between the energy pane and the suggestions. No FLX4 was connected, so its LOAD button was not pressed.
  - Requests took 0.7 to 1.1 ms in the debug build, and the list followed each deck load without a visible delay (the screen polls the engine every 33 ms).

Remaining before version 1 counts as done: the practice-set review ("the DJ loads from the list regularly") and calibrating the energy score against the DJ's own sense of energy. Not modelled: the key shift from a tempo change without key lock (about half a semitone at ±3%).

### Version 2: sound similarity

- An audio embedding so tracks with the same key and tempo but a different sound rank apart. Candidate: LAION's [larger_clap_music](https://huggingface.co/laion/larger_clap_music) (Apache-2.0 weights). It is much larger than the beat model, so it is an optional download that runs in the background.
- Local and Spotify tracks are embedded whenever their PCM is analyzed or loaded. One small vector per track (a few KB) keeps even a 20,000-track library small, and brute-force scoring takes milliseconds without a vector database.
- CLAP also embeds text, which could later support offline "describe what you want" search.

### Version 2 plan (proposed 14 October 2026, not started)

Nothing here starts, and no weights are downloaded, without the owner's go-ahead.

1. **Spike, about 3 days.** Export the audio encoder of `larger_clap_music` to ONNX and run it with the existing tract runner. Compare its embeddings with the PyTorch reference on the six spike tracks (cosine similarity above 0.999), then measure size, speed and memory. The published checkpoint is 776 MB with both the audio and text encoders in float32; the audio encoder alone, stored at float16, should be much smaller, and the spike measures it. If tract cannot run the encoder's operators, the fallbacks are ONNX Runtime's CPU backend (a larger native library) or a smaller encoder with a permissive licence.
2. **Optional download.** Settings → Library gets a **Sound similarity** card showing the size and the Apache-2.0 licence, with Download and Remove buttons. The file comes over HTTPS from a pinned URL, is checked against a recorded SHA-256 and is stored in the app data folder. Nothing downloads until the DJ presses Download.
3. **Background embedding job.** It runs after the beat stage on import and **Update library analysis**, and whenever a Spotify track's audio is already in memory (deck load, **Analyze page**), behind the existing Spotify model-analysis switch. It uses the beat model's pattern: utility priority, one track at a time, cancellable, deck loads first. A few 10-second windows from the track's main sections (from feature 2) are averaged into one 512-value vector, stored at float16 (about 1 KB per track) next to the version 1 features with a model version.
4. **Scoring.** Sound similarity (cosine) becomes a fourth fit, at proposed weights of key 40%, tempo 30%, sound 15% and energy 15%, with "similar sound" in the reason when it is high. Tracks without an embedding keep their version 1 score. Brute force over 10,000 vectors of 512 values adds well under a millisecond.
5. **Acceptance.** On hand-picked pairs, same-style tracks rank closer than cross-style tracks; the 50 ms target still holds; the practice review repeats.

About 2 weeks after a successful spike. Decisions needed from the owner: approval to download the weights for the spike, and whether the measured download size is acceptable.

**Out of scope.** Learning from play history (needs a history table first), automix and cloud recommendations.

### Acceptance

- The list updates within about 50 ms of a track change on a 10,000-track library.
- The top ten never contains an unflagged key clash.
- In practice sets, the DJ loads from the list regularly.

**Effort:** version 1 about 1 to 1.5 weeks with no dependency on feature 1. Version 2 about 2 weeks and reuses the model runner.

## 5. Transition guide

Added 14 October 2026 at the owner's request; it depends on feature 2. The app suggests when to start the waiting deck so the two tracks' sections line up, and draws a guide on the waveforms. The DJ still does the mix: nothing starts, moves or fades on its own.

### How it works

- **Decks.** The playing deck the crowd hears goes out (channel fader and crossfader, as in feature 3), and the other deck comes in. Both tracks need sections, so both need a model or manual grid.
- **Bars, not seconds.** Each track is counted in bars on its own grid, so a plan holds once the decks are beatmatched, whatever their source tempos.
- **Candidates.** Every 8-bar phrase start of the playing track from now on (section starts, and every 8 bars inside a section) is paired with start points in the new track. Those are the sections before its first drop or hook, plus the point exactly as many bars before that drop as remain to the playing track's end or its last drop.
- **Score.**
  - 35%: where the new drop or hook lands. Best is as the playing track ends, or as its last drop ends; worst is on top of a drop.
  - 25%: avoiding two full sections at once, such as a drop over a drop.
  - 20%: how long both play together; dance mixes prefer 16 to 32 bars, hip-hop 8 to 16.
  - 10%: starting late in the playing track.
  - 5% each: starting on a section start, and starting the new track on one of its phrases.

  At most three alternatives are shown, one per start moment, and they are recomputed once per beat.
- **Once both play,** the guide stops planning and checks the mix: whether the new track's next section falls on the playing track's phrase ("B is 2 bars behind A's phrase"), and where its drop will land.

### Interface

- A **mix guide** key in the waveform toolbar turns the guide on or off; the choice is remembered on this computer. Beside it is the suggestion, for example "start B at A bar 129 · B's intro over A's outro · 16 bars together · B's drop lands as A ends", with arrows to step through the alternatives. **Cue B** moves B's cue point, and its playhead when paused, to the suggested start, only when pressed.
- On the playing deck's waveform: a dashed **START B** line, the stretch both play together tinted in B's colour, and a dotted **B DROP** line where B's drop lands. On the waiting deck: a dashed **START** line. The overview strips show the same lines.
- Under the bar counter beside the playing deck's playhead: **START B in 8** (bars), then **in 3 beats**, then **NOW**.

### Build status (14 October 2026)

Implemented in the frontend only: [transition.ts](../frontend/src/transition.ts), [TransitionGuide.tsx](../frontend/src/TransitionGuide.tsx), guide drawing in [Waveform.tsx](../frontend/src/Waveform.tsx) and the toolbar line in `App.tsx`. Nothing changed in the engine or the analysis. 14 new tests, 62 frontend tests in all, cover intro-over-outro and drop-swap plans, phrase alignment, mixed tempos, start points part-way into an intro, a long outro, offering only future moments, the countdown, the guides, the live check (including inside a long section), bar numbering and deck roles.

Checks so far:
- **Real library.** On a copy of the owner's library, the first choice between house tracks put the new drop on the old track's end. For example, "How Long Have You Been Away?" into "It's Over Now" started at A bar 129: intro over outro, 16 bars together. Hip-hop and drill pairs got short intro-over-outro mixes. Irregular analysed lengths, such as 10-bar intros and 17-bar outros, are why start points part-way into an intro and a 2-bar tolerance at the end were added.
- **Drawing.** A temporary preview page drew the guides with the real waveform component on two synthetic tracks: the start line, the tinted span, the landing line, the waiting deck's start line and the countdown.
- **Desktop app** (debug bundle, master output muted, 14 October 2026), with the two local drill beats:
  - With A playing, the toolbar showed "start b at a bar 73 · b's intro over a's outro · 8 bars together", with alternative 2 of 3 "start b at a bar 65 · b's intro over a's verse · 16 bars together".
  - On A's waveform, "START B in 64" counted down to "in 2" beside a dashed START B line and the tinted span at A's outro; B's waveform showed its START line. **cue b** moved B's cue and playhead to its start. Once the moment had passed, the toolbar said "no phrase left in a to start b on".
  - With B started on A's outro (using A's hot cue C), the live check read "b is on a's phrase", then "b is 0.25 bars ahead of a's phrase" two seconds later. That is correct, because the decks were not tempo-matched (142 against 150 BPM).
  - The click-through found that with B inside a 64-bar section the live line was empty. It now checks against B's next 8-bar phrase.
  - **Unexplained:** twice, deck A stopped when B was started, away from a track end. Two later attempts did not reproduce it, and no path in the interface or engine that pauses the other deck was found. Left open to watch.
- **Not yet done:** a practice review of whether the suggested moments are the ones the DJ would choose.

**Out of scope:** automatic mixing (starting, fading or EQ), meters other than 4/4, and FLX4 controls for the alternatives (the toolbar works with the mouse or keyboard).

## Order

| Step | Depends on | Rough effort |
| --- | --- | --- |
| Feature 1 spike | nothing | done 14 October 2026: go |
| Feature 1 build | spike passes | about 2.5 weeks |
| Feature 3, version 1 | nothing; can run in parallel | built 14 October 2026; practice review pending |
| Feature 2 | feature 1 | first version built 14 October 2026; evaluation pending |
| Feature 3, version 2 | model runner | about 2 weeks; plan proposed, waiting for the owner |
| Feature 5, transition guide | feature 2 | built and clicked through 14 October 2026; practice review pending |

## Risks

- **Spotify policy.** Accepted by the owner. The Settings switch allows turning Spotify model analysis off quickly.
- **Model conversion.** ONNX export or runner support for the transformer may need work.
- **Preprocessing mismatch.** A front end that differs from training silently lowers accuracy; the spike's reference comparison guards against it.
- **Hip-hop structure.** Sections are harder to label than in dance music, so more hip-hop sections may stay unnamed.
- **Re-analysis time.** Large local libraries take time to upgrade, and Spotify upgrades require re-streaming.

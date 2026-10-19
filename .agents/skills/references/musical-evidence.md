# PulseDeck analysis semantics

Use this reference to interpret observations. `$djing-art` owns selection, phrase/arrangement relationships and musical judgment. `$pulsedeck-library` owns saved metadata edits; `$pulsedeck-control` owns playback and timing. Analysis reads do not authorize reanalysis or cue/grid edits.

## Waveforms and named regions

`track.inspect` returns bounded bins from existing data. Peak aggregation is a maximum; low/mid/high values aggregate stored RMS on a shared full-band scale. Compare neighboring ranges within the source. A brief peak is not sustained intensity, and a quiet bin may be a gap rather than a section. Null bands are missing evidence. Normalization, mastering, window size and bins straddling range boundaries limit inference. More points cannot exceed stored resolution.

Band changes support amplitude/texture/bass-presence hypotheses, not vocals, instrumentation, emotional mood, genre or a verified chorus/drop. JSON-only inspection is not listening. Use available human-authored cue spans with per-point provenance and inspect the surrounding passages. PulseDeck has no automatic cue/section generation. Look before and after a candidate change.

The track-level suggestion score is a waveform loudness/brightness heuristic, not a passage-specific tension measure. Scores shortlist candidates; inspect the intended passages before the art decision. Missing key or vocal evidence stays unknown. See [named anchors and analysis](named-cues-and-suggestions.md).

## Effective beat clock and provenance

Default full Beat This! `final0` inference uses native DBN/HMM decoding, comparing one whole-track meter of three, four, five or seven beats per bar over 55–300 BPM. It retains varying local timestamps and imposes bar continuity; it does not detect arbitrary meter changes or musical phrases. Model/activation scores are uncalibrated and do not prove accuracy or rekordbox parity. Failure/warning requires passage/grid validation before automated beatmixing; retained estimates are not a substitute for successful analysis.

Effective loaded flags/events take precedence over original neural provenance:

| Evidence | Interpretation |
|---|---|
| `beatgrid_valid` | Usable effective grid versus an unknown numeric-zero fallback |
| `beatgrid_downbeat` | Predicted bar identity or manual origin; neither establishes a phrase |
| `beatgrid_manual` | Explicit origin/edit provenance |
| `beatgrid_uniform`, `beat_events.uniform` | Constant-tempo override rather than the original neural sequence |
| Native `beat_position` | Current anchor-relative phase on the effective clock |

An explicit source BPM/half/double correction switches to a whole-track uniform grid. Offset-only realignment preserves active neural intervals and does not switch a uniform override back to neural mode. Manual origin numbering retains detected meter, with four as unknown-meter fallback. Original neural counts/BPM remain provenance and can differ from effective events. Unloaded manual-BPM records may infer uniform mode from `bpm_source:manual`.

For a selected interval use actual `beat_events` positions/timestamps, not `(time-offset)*mean_BPM/60`. Events are capped at `points`; inspect `total_in_range` and `truncated`, narrowing ranges to cover the complete lead when counting. Model frame resolution is 0.02 seconds. A beat-position difference counts intervals, not the number of returned events including both endpoints. A proposed grouping of 8/16/32 bars is a candidate until arrangement evidence supports it.

## Ambiguity and reconstructed pulses

`bpm_ambiguous:true` requires passage review even if local BPM appears stable. Inspect `neural_rhythm.bar_evidence`, `half_phase_corrected` and `pulse_repairs`. Midpoint events used in a guarded half-pulse repair are reconstructions, not newly detected acoustic beats. Repair records include source ranges/counts, recurrence and flanking support. Bar recurrence scores describe the independent clock before reconstruction, not rescored confidence for the repaired publication.

For bounded inspection, `beat_events.local_tempo` includes source BPM/p05/p95, `inferred_events_in_range` and `pulse_review_required`. A broad spread, half/double interpretation or repaired passage needs checking against its actual pulse. Regular model bar gaps are imposed partly by the decoder and do not independently verify meter.

## Playing tempo and phase

`local_bpm` is the current four-beat fit. `playback_bpm` multiplies it by nominal rate; `actual_playback_bpm` includes rendered local adaptation/phase/bend. `target_playback_bpm` remembers accepted intent. Absolute playback BPM setting uses the local fit at execution and is an entry setting, not a continuing BPM lock. Use valid master/follower Sync for ongoing local adaptation during a variable-tempo blend; it preserves transport position. See the [CLI field details](cli.md#playing-tempo-and-source-bpm).

For beat phase, read both native `beat_position` values from the same snapshot and wrap B minus A into `[-0.5,0.5)`. With credible same-meter downbeat origins, also inspect modulo the bar length. A two-beat offset in 4/4 can pass fractional alignment and fail bar alignment. Compare actual arrival countdowns for phrase progression. Convert phase to approximate milliseconds with the local effective beat interval, without treating one numeric tolerance as an audible guarantee.

A missing grid/phase is unknown, not aligned. Sync and quantize do not identify phrase starts or vocals. Use a supported simpler cut/fade when the intended sustained beatmix lacks credible timing, and report that distinction.

## Source-specific timelines

Spotify may relink a selected URI to a provider-listed playable release. `status.source_jobs[deck].resolved_uri` records the actual release after provider preparation, including the requested URI when unchanged; it is not a deck-readiness signal. selected-source guards retain the requested URI. Check actual loaded title/artist/duration and refreshed timing; source-time cues for another edit/version cannot be assumed correct. Preserve explicit manual metadata and respect unrelated-source/availability rejections.

Duration calculations, harmonic overlap and energy programming belong to the relevant art/library references. Their conclusions must retain these observation limits rather than turning a section label or scalar score into certainty.

## Measured sound limits and loudness

`audio_measurements` stores full-PCM audible start/end frames at −60 dBFS (end exclusive), gated integrated LUFS, true-peak dBTP and a suggested programme correction. These sound bounds are independent of musical cues. Current planner runway uses these bounds; older analysis falls back conservatively to waveform evidence until source preparation upgrades it. Silent material has no audible interval.

Normalization is enabled per deck on load, targets −18 LUFS with bounded boost/true-peak headroom, and stays separate from manual trim. `set_normalization` toggles it smoothly. Read `normalization_gain_db` for applied correction. Integrated programme matching does not prove that a breakdown and a drop have equal local loudness; passage/role review still matters. Unknown/error measurements retain unity gain rather than inventing a correction.

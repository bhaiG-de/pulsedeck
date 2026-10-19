# Choosing entries and exits from arrangement

## Map functions rather than just names

A phrase is a musically coherent unit. A section has a larger arrangement role and may contain several phrases. A 32-bar annotated region may contain useful changes at 8 and 16 bars, or an irregular extension. Find local evidence before applying arithmetic.

Useful clues include repeated patterns, fills, cymbal accents, bass changes, a lyric completing, a new answer or melody, deliberate subtraction, rising tension, and a sustained arrival. Distinguish a brief fill from a new section. When using waveform/feature evidence, describe only what it supports: amplitude, band contribution, texture or repetition. It cannot establish that a singer has finished.

| Role | What to establish | Possible mixing use |
|---|---|---|
| Intro | What enters and when; when the track can carry the foreground | Supporting entrance, or a later complete start |
| Groove or verse | Which idea leads and where its units resolve | Develop the current song; introduce complementary support |
| Hook or chorus | Defining recognizable material and its ending | Protect it, then exchange into B's next idea |
| Breakdown | What withdraws and whether a motif remains | Give space or make a deliberate atmospheric bridge |
| Buildup | What arrival is being promised and its exact timing | Preserve the payoff or prepare an intentional replacement |
| Drop or payoff | Context before it and sustained material after it | Let the payoff land; choose an appropriate later exit |
| Outro | What remains and for how long | Blend while replacement elements arrive |

These are functional interpretations, not claims that all tracks contain every role in that order. A section labeled `outro` may be too sparse for the desired handover. A passage inside the core may provide a better exit.

## Choose the outgoing moment before its file end chooses for you

Ask what the track contributes to this set. Protect that contribution long enough to be meaningful for the brief. A full verse, a short vocal refrain and a developing instrumental need different amounts of time. For a song based on a buildup and drop, distinguish delivering the buildup from delivering its resolution.

Look for a musically complete unit after that contribution, with space for the intended change. Do not automatically wait for the final outro. Compare a later option: does it introduce worthwhile development, repeat material already heard, or take the set into an unwanted loss of drive? Waiting is useful when the passage is still developing.

Select the handover jointly with B. The same A exit can work with B's developed groove and fail with C's drum-free intro. Avoid storing a single universally correct mix-out point for a track independent of its follower and purpose.

## Work backward from the incoming arrival

Choose what should lead the listener at handover: a main groove is as valid as a drop. Then choose an entry that leads coherently into it. Inspect the material between them, not just the endpoints.

With credible beat positions on the same pulse interpretation:

```text
incoming_lead_beats = B_arrival_beat - B_entry_beat
A_launch_beat = A_handover_beat - incoming_lead_beats
```

The beat numbers are positions, not event counts including both endpoints. A 64-beat difference spans 64 intervals, although a response including both boundaries might contain 65 events.

At a steady playing tempo:

```text
lead_wall_seconds = incoming_lead_beats * 60 / playing_BPM
```

At constant playback rate, a source interval takes `(arrival_source_seconds - entry_source_seconds) / playback_rate` wall seconds. When rate or local tempo varies, this conversion is not constant: follow actual events and the intended clock behavior with the control workflow.

If the subtraction lands off a credible phrase boundary, investigate the mismatch. It may be a pickup, an extra bar, an unsuitable entry, or wrong meter/grid. Do not round to the nearest convenient 8/16/32 bars. Select a later complete B entry, a different A handover, a prepared loop with a deliberate release, or a different technique.

## Preserve pickups and genuine irregularities

An incoming lyric can start before its first downbeat. If the full lyric matters, include that lead and launch at the corresponding point in A's bar, while allowing A's preceding phrase room to finish. If it clashes, choose another complete entry. Do not chop the pickup to make a bar formula work.

Extra beats and bars change the countdown. Repeatedly matching fractional beat phase is insufficient when bar or phrase phase is wrong. Half/double-time interpretations also change bar counts; confirm the musical pulse instead of treating 87 and 174 BPM as interchangeable phrases.

For variable tempo, use actual beat/downbeat events and source endpoints, with enough samples to cover the interval. A truncated inspection is not a complete event sequence. A clean grid can coexist with incompatible swing or busy patterns. Shorten or change the blend when the grooves do not sit together.

## Hand the phrase plan to the blend design

Retain the important internal changes as well as entry and arrival. B may add a vocal eight bars before its apparent drop; A may return with a lead halfway through an assumed outro. These events constrain which boundaries can work together. Use [mixing and blending](mixing-and-blending.md) to assign foreground, bass and supporting roles across that timeline. It owns layer transfer and balance; this reference owns the complete musical units and their relationship.

## Use PulseDeck observations without inventing hearing

When available, use named canonical cue spans with start/end times and per-point provenance. Check grid freshness, meter, actual bar times, local beat events and neighboring bands. PulseDeck does not generate structural labels; use human annotations when present. A manually confirmed cue is useful source knowledge, but confirmation alone does not specify how it should be paired with another song.

Compare neighboring stored waveform bands to locate candidate changes; they do not identify a chorus or first drop. A whole-track suggestion energy score does not describe this intro’s drive. Without cues, keep an ephemeral passage map supported by these observations and actual beat events; do not write inferred boundaries into the manual cue store automatically.

If exact first-drop or vocal placement matters and evidence cannot locate it, keep the uncertainty visible and seek a usable cue or audition through available capabilities while preparing alternatives. Under a live deadline, preserve current playback and choose a handover supported by what is known. Do not spend the remaining song repeatedly requesting the same coarse waveform.

## Rank evidence for a phrase candidate

Combine independent clues rather than promoting a convenient multiple of bars into a fact. A known cue for this edit, a repeated pattern resolving, a fill leading into sustained new content, and a change in bass/body together can support a boundary. A single isolated peak or a cue name without context is weaker. Inspect preceding context, the boundary itself and enough following material to identify what actually arrives.

Use the meter and local bar origin from this source. Search common 4/8/16/32-bar relationships as candidate scales, then count the selected effective events. Intro, verse, buildup and outro are arrangement functions; a usable phrase can sit inside one of them. An annotation can also split a continuous idea or suit a different mixing purpose. If data only establishes a texture change, name it as such and retain the uncertainty about lyrics and resolution.

The candidate’s job matters. A good first-audibility point can be too early for foreground handover. A good foreground arrival can be a poor playback entry if it omits its pickup or explanation. A good exit for one successor can fail for a sparse successor. Do not store one independent scalar “correct mix time” and apply it to all pairs.

[Crossfader’s listening lesson](https://wearecrossfader.co.uk/blog/learning-to-listen-like-a-dj/) teaches recognition of structure and what enters or withdraws. For an agent, this translates into an evidence-backed map rather than a claim of hearing from numeric features. The evidence ranking and boundary roles above are synthesized reasoning guidance.

## Choosing between two feasible windows

Consider a current groove that ends with a fill, followed by an eight-bar buildup and a strong payoff. A successor offers a drum-led approach and a later developed groove. If the brief wants the current payoff, the earlier mathematically compatible handover is unsuitable: let the payoff develop, then inspect a later complete exit. If the brief calls for a purposeful replacement, B must supply the intended resolution rather than restart anticipation with an unrelated sparse intro.

Compare the cost of waiting: useful musical development, an unwanted long breakdown, a repeated section, or insufficient later preparation runway. Choose among feasible complete windows. Do not optimize the target duration by cutting a lyric halfway through or moving an endpoint to the nearest arbitrary beat.

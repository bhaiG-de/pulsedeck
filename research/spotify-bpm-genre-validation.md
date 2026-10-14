> Historical analysis-version-5 results. See [version-11 validation](spotify-rhythm-v11-validation.md) for the current Take Five correction and genre/BPM checks.

# Broader Spotify BPM / genre validation — 14 October 2026

All twelve preselected Spotify tracks now load and produce model analysis, spanning **76.98–210.99 detected BPM** across twelve contrasting style cases. Seven initially rejected provider-linked releases succeed after the loader fix. This is an exploratory engineering sample, not an accuracy percentage or evidence of rekordbox parity. Full authenticated sources were decoded in RAM with output muted; no audio was exported.

| Track | Style case | Detected BPM | Beats/bar | Assessment |
|---|---|---:|---:|---|
| Someone Like You — Adele | ballad / piano | 135.02 | 4 | Processing / tempo checks pass; musical alignment unverified |
| Teardrop — Massive Attack | trip-hop | 76.98 | 4 | Tempo/pulse changes: review |
| Three Little Birds — Bob Marley | reggae | 148.38 | 4 | Processing / tempo checks pass; musical alignment unverified |
| Lose Yourself - Instrumental — Eminem | hip-hop | 85.71 | 4 | Processing / tempo checks pass; musical alignment unverified |
| Despacito — Luis Fonsi | reggaeton / Latin pop | 88.97 | 4 | Processing / tempo checks pass; musical alignment unverified |
| Smells Like Teen Spirit — Nirvana | grunge / rock | 117.65 | 4 | Processing / tempo checks pass; musical alignment unverified |
| Sandstorm — Darude | trance | 135.98 | 4 | Processing / tempo checks pass; musical alignment unverified |
| Blinding Lights — The Weeknd | synth-pop | 171.12 | 4 | Processing / tempo checks pass; musical alignment unverified |
| Tarantula — Pendulum | drum and bass | 173.91 | 4 | Tempo/pulse changes: review |
| Master of Puppets — Metallica | thrash metal | 210.99 | 4 | Tempo/pulse changes: review |
| The Second Waltz — André Rieu | classical waltz | 185.33 | 3 | Three-beat meter matches composition; timing unverified |
| Take Five — Dave Brubeck | jazz / odd meter | 168.42 | 4 | **Known meter failure: expected 5** |

All twelve returned finite, sorted, unique, in-source beat timestamps with downbeats contained in the beat sequence, analysis version 5, no rhythm error and no manual/uniform override. All sources retain recording_allowed:false. The model is full Beat This! final0 FP32 with the native madmom-style DBN, whole-track meter candidates 3/4 and tempo search 55–300 BPM. Activation scores are uncalibrated and event resolution is 20 ms. A median summary may describe a different pulse interpretation from the musical BPM.

Full inference took 6.55–17.12 seconds for sources 180–515 seconds long. Load times include provider/network work and are recorded separately.

## Musical failures and uncertainty

**Take Five is a confirmed meter limitation:** the decoder returned four-beat bars although the composition is in 5/4 ([Library of Congress](https://blogs.loc.gov/nls-music-notes/2020/05/song-stories-time-out-for-take-five/)). Candidate restrictions prevent correct five-beat bars. Whole-track meter changes and other odd meters remain unsupported; regularity imposed by DBN cannot validate its own bar placement. The Second Waltz returned three-beat spacing throughout, matching its composition-level meter, without establishing exact acoustic downbeat placement.

The runtime tempo-spread warning is active for the following tracks. Local ranges use the 2nd/98th percentiles of sliding 32-beat windows, not minimum/maximum instantaneous intervals:

| Track | Local BPM p02–p98 | Runtime warning |
|---|---:|---|
| Teardrop | 76.92–154.09 | true |
| Tarantula | 85.64–174.23 | true |
| Master of Puppets | 99.48–219.18 | true |
| Take Five | 83.26–182.86 | true |

These spreads can represent intentional tempo changes or half/double-pulse interpretation switches; this test does not distinguish them. Lose Yourself also ranges 85.64–130.97 BPM, below the warning ratio threshold of 1.6. An absent warning does not prove a stable/correct musical pulse. Agent instructions require review of the intended passage and distinguish beats, bars and phrases. High activation scores cannot remove that requirement.

## Provider-resolution fix

The adapter selects only available alternatives explicitly listed by Spotify metadata, loads the exact selected URI, and rejects unexpected playback identity. It retains requested-source identity and exposes resolved_uri through source_jobs. Seven formerly blocked sources—Adele, Bob Marley, Eminem, Luis Fonsi, Nirvana, Metallica and André Rieu—now load with matching title/artist and a recorded provider-linked URI. Relinked releases refresh automatic timing/envelopes while preserving explicit manual metadata. Rust workspace tests, strict Clippy, 48 frontend tests and the rebuilt desktop bundle passed.

## Native launch and sync checks

Additional muted playback checks use observed model downbeats, source revision guards and native sample-clock launch scheduling. B is tempo-matched before launch, then continuous sync is enabled. Each overlap is sampled for eight seconds. These are transport/grid tests; they do not validate the model against sound or demonstrate a musically good blend.

| Pair | Clock / incoming entry (s) | Initial phase error (beats) | Maximum sustained error (beats) |
|---|---:|---:|---:|
| Lose Yourself - Instrumental → Despacito | 63.50 / 60.98 | 0.001020 | 0.000177 |
| Smells Like Teen Spirit → Calmy - Extended Mix | 65.12 / 60.62 | 0.004443 | 0.000550 |
| Sandstorm → Someone Like You | 64.76 / 61.22 | 0.000852 | 0.000651 |
| Blinding Lights → Tarantula - Original Mix | 94.10 / 91.22 | 0.003202 | 0.016497 |

All four checks stayed below the 0.05-beat test limit; the largest observed error was 0.016497 beats on the fastest pair. There were 29 snapshots per overlap, positive native deck peaks and zero stream errors. These errors are wrapped differences between native beat_position values, so they cannot measure acoustic model error. The original Into You / Calmy decks, positions, tempo/sync settings and output levels were restored, with both decks paused and no pending jobs or launches.

Earlier house-track checks and the continuous-sync bug/fix are in [spotify-grid-validation.md](spotify-grid-validation.md). Source URLs, requested/resolved identities, event counts, tempo distributions, warning flags and sampled transport evidence are in [spotify-bpm-genre-validation.json](spotify-bpm-genre-validation.json). Genre labels are curated selection cases, not Spotify genre annotations. A labeled human beat/downbeat benchmark and listening are still needed before claiming high-precision acoustic accuracy or rekordbox parity.

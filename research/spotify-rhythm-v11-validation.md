# Spotify rhythm validation — analysis version 11

All twelve preselected sources load and return valid native model timelines. They span 77.00–211.00 detected BPM. These are runtime integrity and transport checks, not independent acoustic accuracy or Rekordbox parity.

| Track | Style case | Detected BPM | Beats/bar | Review |
|---|---|---:|---:|---|
| Teardrop | trip-hop | 77.00 | 4 | Pulse/tempo spread |
| Lose Yourself - Instrumental | hip-hop | 85.72 | 4 | Musical alignment unverified |
| Despacito | reggaeton / Latin pop | 88.99 | 4 | Musical alignment unverified |
| Smells Like Teen Spirit | grunge / rock | 117.71 | 4 | Musical alignment unverified |
| Someone Like You | ballad / piano | 134.98 | 4 | Musical alignment unverified |
| Sandstorm | trance | 136.04 | 4 | Musical alignment unverified |
| Three Little Birds | reggae | 148.36 | 4 | Musical alignment unverified |
| Blinding Lights | synth-pop | 171.02 | 4 | Musical alignment unverified |
| Take Five | jazz / odd meter | 171.14 | 5 | 142 inferred pulses |
| Tarantula - Original Mix | drum and bass | 173.98 | 4 | Pulse/tempo spread |
| The Second Waltz, Op. 99a | classical waltz | 185.28 | 3 | Musical alignment unverified |
| Master Of Puppets | thrash metal | 211.00 | 4 | Pulse/tempo spread |

## Take Five correction

The final build selects five-beat bars and reconstructs 142 pulses across approximately 158.48–263.85 seconds. Median local tempo is 171.14 BPM. The 5th–95th percentile of 32-beat local tempo changes from 86.95–176.25 BPM in version 10 to **166.33–176.46 BPM** in version 11. Beats/downbeats change from 774/154 to 916/183. The correction uses the dominant pulse and matching flanks, preserves already normal intervals inside boundary blocks, and retains both source and affected-window review warnings. Inserted events are explicit midpoint reconstructions, not fresh neural detections. The [composition-level 5/4 reference](https://blogs.loc.gov/nls-music-notes/2020/05/song-stories-time-out-for-take-five/) does not establish exact downbeat timestamps for this release.

The Second Waltz selects three-beat bars, matching its composition-level stress case. Teardrop, Tarantula and Master of Puppets retain tempo/pulse-spread warnings. Other tracks can still have a musically incorrect pulse despite a narrower spread; for example a consistent half/double interpretation need not trigger the warning. No absence of warning is treated as accuracy.

## Evidence and limits

Every source has analysis version 11, no rhythm error, sorted/unique/in-source timestamps, downbeats contained in beats, no manual/uniform grid override and recording_allowed:false. All twelve sources were analyzed with master and cue outputs muted. Requested/resolved URIs, event digests, meter hypotheses, local tempo summaries, confidence scores and five inspection windows per track are retained in [the JSON evidence](spotify-rhythm-v11-validation.json). Genre labels are curated test cases, not provider genre annotations.

Independent human-label measurements, strict timing improvements, regressions and the remaining offbeat failures are in [the labeled evaluation](labeled-rhythm-validation.md). The [earlier version-5 sweep](spotify-bpm-genre-validation.md) is historical; these are the final-build source checks.

## Native queued launches and sync

Five muted overlaps test paused incoming sync, cue preservation, native source-clock launches, bar/beat alignment and eight seconds of continuous sync. All pass the 0.05-beat transport tolerance. All initial bar errors are below 0.003 beats; software source peaks are positive throughout, and no stream/device errors are reported. Those counters do not measure hardware underruns. These comparisons are against generated grids; acoustic beat correctness and listening remain separate requirements.

| Pair / passage | Meter | Initial grid error (beats) | Maximum sustained grid error (beats) |
|---|---:|---:|---:|

| Lose Yourself - Instrumental → Despacito (hip-hop / Latin pop) | 4 | 0.000028 | 0.000915 |
| Smells Like Teen Spirit → Calmy - Extended Mix (rock / house) | 4 | 0.000664 | 0.002995 |
| Sandstorm → Someone Like You (trance / piano ballad) | 4 | 0.000234 | 0.002539 |
| Blinding Lights → Tarantula - Original Mix (synth-pop / drum and bass) | 4 | 0.000972 | 0.006010 |
| Take Five → Take Five (jazz / reconstructed solo) | 5 | 0.002529 | 0.027523 |

The fifth test starts the clock in Take Five around 140 seconds and the incoming copy in its reconstructed solo around 190 seconds. It therefore exercises different local beat spacing despite identical track-summary BPM. It is a transport test, not a performed artistic mix.

Original Into You / Calmy sources, playhead positions, tempo and sync states are restored. Both decks are paused, master/cue gains are 0.8, recording is inactive, and no source jobs, preparation or launches remain pending. The final state and individual transport samples are retained in the JSON evidence.

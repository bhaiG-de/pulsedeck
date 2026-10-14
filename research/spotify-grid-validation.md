# Spotify grid validation — 14 October 2026

The final packaged app analyzed the same four Spotify tracks used in the session, using full Beat This! final0 plus the default 55–300 BPM DBN. Native authenticated streaming decoded each full source into RAM; no Spotify PCM, recording or exported audio was created. All source recording capabilities remained disabled. The library/streaming credentials were already connected; no authorization or scopes changed.

| Track | Detected BPM | Beats / downbeats | First predicted downbeat | Analysis wall time |
|---|---:|---:|---:|---:|
| Brave — Texture | 122.92 | 307 / 77 | 0.22 s | 5.78 s |
| Falling — Keanler | 124.03 | 312 / 78 | 1.72 s | 5.76 s |
| Into You — April Girl | 125.00 | 403 / 101 | 0.94 s | 6.78 s |
| Calmy - Extended Mix — Chris Luno | 123.08 | 675 / 169 | 0.12 s | 11.58 s |

All four sources produced finite, sorted in-source events, analysis version 5, no rhythm error and consistent four-beat intervals between predicted downbeats. Their prior caches were version 3; three had no grid origin. Successful loads upgraded and persisted their analysis. These are predictions, not human-verified musical bar starts. The model has 20 ms frames; activation scores are uncalibrated.

## Native launch and continuous sync

Master and headphone gains were muted. Each incoming deck was cued on an actual predicted downbeat in a stable preparation region, effective BPM matched, then launched on a future outgoing downbeat with both source revision guards. First snapshots verified beat and bar phase. Sync was then enabled and sampled every ~0.25 seconds for eight seconds. Both decks rendered nonzero signal, source positions advanced and stream error counters stayed at zero.

| Pair | Outgoing launch | Incoming entry | Initial phase error (beats) | Maximum sustained error after fix (beats) |
|---|---:|---:|---:|---:|
| Brave → Falling | 35.36 s | 17.44 s | 0.002274 | 0.000547 |
| Falling → Into You | 34.86 s | 16.32 s | 0.002710 | 0.000666 |
| Into You → Calmy - Extended Mix | 35.52 s | 17.68 s | 0.001137 | 0.000348 |

The initial average-BPM controller drifted to 0.057786 beat on Falling → Into You as local beat intervals changed. The final controller feeds forward both grids' local source-time slopes, adds phase correction and smooths/bounds its varispeed adjustment without seeking live audio. The same three overlaps passed after the fix; maximum sustained phase error was 0.000666 beat (~0.32 ms at 124 BPM). This compares native transport with its detected grids; it does not measure learned detection accuracy against actual transients or listening quality. A synthetic nonuniform-grid regression and the callback allocation audit also pass.

The selected timestamps test control behavior. Band waveforms were inspected in bounded intro/body/tail windows, but numerical bands cannot establish vocals or exact phrase boundaries. No full musical phrase claim or human listening assessment is made. Phrase-based entry still requires arrangement evidence and appropriate lead-in counting; detecting a downbeat does not label a phrase.

Both decks ended paused with Into You / Calmy loaded, tempo and sync test overrides cleared, and original master/headphone gains restored. Detailed event counts, confidence, band summaries and before/after phase samples are in [spotify-grid-validation.json](spotify-grid-validation.json). Wider detector/decoder comparisons and limitations are in [beat-detector-validation.md](beat-detector-validation.md).

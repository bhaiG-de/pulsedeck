# Performance waveform UI evidence and reconstruction

2026-10-13. Target: installed rekordbox 7.2.19.0342; matching binary identity is recorded in [the static analysis report](rekordbox-analysis-waveform.md). REA's staged workflow was used to separate runtime UI questions from waveform/tempo/phase implementation. REA MCP/native decompiler tools remain unavailable in this session. The observations here are **CUA native accessibility/pixels and user-supplied screenshots**, not invented REA Evidence IDs or decompiled runtime behavior.

The user supplied screenshots at 19:25:44/19:26:25 and 19:27:49/19:28:16 showing Performance / 2Deck Horizontal. CUA subsequently inspected the live `com.pioneerdj.rekordboxdj` Performance window with `Brave` (123.00 BPM, Gm) and `Hello (The Sun Is Up Where Are You)` (124.00 BPM, Fm), playback times 1:26.2 and 1:30.0. No account settings, protected transport, music export, firmware installation or analysis preferences were changed. Initial preferences/setup dialogs were dismissed to reach the existing deck window.

Observed waveform behavior:

- Two enlarged rows share a central playback reference. At a new song's position zero, the left side is blank and the audio begins to the right. During playback, the row scrolls beneath the center; it does not clamp the head to the file's left/right edge.
- Beat ticks span each row, with stronger red top/bottom triangles for four-beat group boundaries. The blue bar/beat readout sits next to the center line. In the supplied playback screenshot these readouts differ (45.1 and 47.2), reflecting each track's own position.
- The full-track preview is separate and appears in its corresponding deck card. Its position marker moves through the whole song. Hot cue flags and loop spans appear in both detailed and full previews in the supplied examples.
- One zoom/reset control group governs the enlarged display. Visual comparison needs the same beat count/scale in each row, rather than independent whole-file/seconds spans.
- Native accessibility describes Beat Sync as synchronizing both BPM and beat position to the chosen master, with x1/x2/x1/2 metrical choices. It separately describes Quantize for cue/loop snapping and Slip for background timeline continuation. Those descriptions establish intended controls, not measured DSP accuracy or complete reconstruction of every mode.

Independent PulseDeck changes:

- Fixed detail head at exactly 50%, with a virtual time window extending before/after the file and real silent padding. Newly loaded audio enters from the right. The overview retains the whole-file timebase and moving head.
- Linked visible beat span, reset zoom, explicit beat/downbeat-style ticks drawn above frequency envelopes, bar/beat labels, color-matched A–H flags, memory/main cue markers, loop shading and overview viewport shading. Source-time BPM and grid origin determine geometry; playback tempo changes the native clock rate.
- Full previews live inside deck cards. The shared stack contains two detailed rows plus linked zoom, a beat-phase gauge and optional snapped waveform seeks. Waveform dragging uses the native jog path. These are independent design choices informed by observed behavior; the phase gauge is not asserted to duplicate a vendor indicator.
- Version 3 adds a data-derived periodic grid origin from causal energy-rise envelopes across bounded excerpts. Phase tests cover offset pulses, strong offbeat hats and absolute excerpt offsets. Ambiguous/nonperiodic signals can retain no phase. This is **not verified musical downbeat detection, phrase segmentation, dynamic tempo mapping or automatic identification of the first bar**. Strong markers group four beats relative to the chosen grid origin; users can align it in the grid editor.
- Version-2 BPM/key/waveform cache entries remain usable. Loading such a source computes only the missing phase from normally loaded RAM audio and upgrades the metadata; it does not discard or redownload an entire cache for UI changes. Manual BPM/grid corrections persist.

Checks: 80 Rust tests, 15 frontend tests and strict workspace Clippy passed before final packaged UI verification. Geometry tests cover fixed center at both boundaries, genuinely blank padding, clamped seeks, equal beat spacing at different BPMs and no invented 120 BPM grid. A real catalog reference yielded 122.02 BPM/B minor (published 122/B minor); `Brave` yielded 122.97/G minor versus 123.00/Gm observed in the supplied/live rekordbox view. These two references do not establish broad accuracy or performance parity.

Remaining scope: calibrated visual latency/jitter, vendor pixel parity, musical downbeat/phrase inference, a dynamic beatgrid, full quantize/slip/key-sync workflow equivalence, the vendor's complete FX/stems system, and physical DDJ-FLX4 routing/timing remain unverified or unimplemented. This pass directly addresses the centered playhead, source-relative beatgrid markers and useful two-deck waveform layout.

# Successor queue and audio improvements

Implemented 19 October 2026, following the [Mixxx architecture review](mixxx-architecture-and-improvements-2026-10-19.md).

## User and agent workflow

Suggest contains the shared next-move list and an explicit successor queue. A person or agent authorizes an ordered list or a scoped collection, technique/energy/tempo policy and optional duration. The app prepares each successor as soon as a deck is released, then submits the same guarded move executor used by individual suggestions. Source failures have a bounded skip limit. Queue revision checks protect pending edits; stop/takeover cancels owned preparation/schedules; restarting preserves an interrupted summary without replaying playback. The control skill documents the exact start/follow/status/edit/stop commands.

An unanalyzed explicitly queued source is loaded/analyzed on the isolated deck with a generation owner and carrier-runway deadline, then replanned. A stopped or superseded load cannot commit to a newer source generation. Complete collection indexing is required for collection selection; a cached latest Spotify page does not stand in for all Liked Songs. The queue still requires a healthy audible carrier at start; unavailable source audio and unusable beatgrids are reported rather than fabricated.

## Precise audible boundaries

Full decoded PCM supplies first/last-above-threshold frame indices at −60 dBFS, stereo maximum. The end is exclusive. Silence has no interval. These limits are independent of user musical cues and carry their own algorithm version, source sample rate and decoded frame count; source fingerprint validation protects reuse. Old cached sources upgrade during PCM preparation without discarding compatible rhythm/key results. Runway uses current PCM bounds; old metadata retains conservative waveform fallback until upgraded. Native overlap plus verification must finish within the outgoing audible interval.

## Programme loudness

The pinned maintained Rust [ebur128 library](https://docs.rs/ebur128/0.1.10/ebur128/struct.EbuR128.html) supplies gated integrated LUFS and oversampled true peak. Work runs before playback on source workers; the audio callback applies only a smooth static gain. The default target is −18 LUFS, with maximum +12 dB boost, −60 dB attenuation bound and a measured source peak cap of −1 dBTP. Silence/unknown/error results apply unity. Gain stays separate from manual trim, can be bypassed with each deck’s Level control or `set_normalization`, and is visible in snapshots and analysis details. Its dependency is optimized in development builds so analysis remains practical.

Integrated programme matching does not equalize every musical passage. EQ/effects/overlaps and time-stretch transients can alter final peaks; the existing output headroom/limiter remains active. No output true-peak guarantee or repair of already clipped source audio is claimed.

## Independent effects tails

Echo/reverb now use fixed-capacity independent send/return buses. Main sends are after channel and crossfader attenuation; headphone sends are pre-fader, with separate effect history. Dry pause, fader closure, source replacement and unloading do not erase an existing main return. After sends stop, feedback decays for a maximum eight seconds; sample-rate changes clear old-rate history. Master gain, routing and protection still apply. Snapshot tail activity/peaks expose residual audio. A service-source return retains its recording restriction even after the source is replaced.

Two eight-bar native recipes are available: `echo_out` and `reverb_exit`. Their effect sends and dry clearance are guarded native ramps with incoming advancement/phase checks, bounded endpoints and recovery. The five available recipes share one planner/executor and successor-runway policy. Native effect targets append to existing ramp indices; old snapshot arrays migrate on read. Existing effects algorithms are reused; no new plugin host or alternate stretching engine is introduced.

## Verification

Results and logs are retained under `artifacts/queue-audio-2026-10-19`:

The [validation manifest](../artifacts/queue-audio-2026-10-19/validation.json) records the rebuilt executable hash, checks and final stopped transport state.

- Workspace regression run: 468 passed, 11 opt-in tests ignored; one existing 100,000-row SQL timing test excluded. Additional focused checks cover the final cache migration, queue event projection and high-level normalization correction.
- Frontend: 73 tests and production build passed. The browser interaction fixture passed queue add/reorder/start/edit/stop and event feedback; a later browser rerun failed during Chrome initialization on this host. The final rebuilt native app was then inspected directly: Suggest opens the successor queue, its controls render legibly in the aluminium theme, and both decks remain stopped with no pending launches, ramps or queue.
- Two unchanged local songs measured approximately −7.92 and −8.74 LUFS before correction and −18.002 and −18.000 LUFS after correction. Exact first/last frame threshold assertions pass. Calibration signals separately cover silence and attenuation beyond 12 dB.
- Seven actual-song native render cases cover all five recipes, incoming-stall recovery and the short fallback. Every case has zero silent 100 ms windows; all normal moves reach their endpoints and the injected stall recovers the carrier.
- The isolated desktop runtime completed Return to Oz → Born Slippy (`short_exchange`) → Explore Your Future (`filter_blend`), stopped safely during unknown-source preparation and restored an interrupted queue without playback replay after process restart. See [runtime evidence](../artifacts/queue-audio-2026-10-19/successor-runtime.json).

These tests discard rendered audio and keep live library/provider state isolated. The queue runtime accelerates its test clock only during native batches; it does not establish wall-clock performance or listening quality. Hardware latency, physical cue routing and a listening comparison remain unmeasured. Luna remains stopped; opening the rebuilt app does not restart the performance.

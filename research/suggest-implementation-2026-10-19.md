# Suggest implementation · 19 October 2026

The top navigation **Suggest** button opens a live next-move panel shared with the `mix.options` control API. It evaluates exact cached releases, incoming passages, future outgoing windows and three native recipes: sixteen-bar EQ/layer blends, eight-bar filter blends and four-bar phrase exchanges. Each result carries source-clock launch, audible entrance, handover and clearance, incoming entry, actual playing tempo, correction, preparation budget and expiry, component scores, evidence and review requirements.

The planner uses multiscale low/mid/high band-energy contrasts plus explicit eight-bar grid priors and confirmed human cues. Priors remain unconfirmed; no verse, vocal ending, drop or automatic cue is invented. Effective edited beat-event mappings supply timing. Fixed-rate overlap admission checks all beat intervals for accumulated drift (at most 0.20 beat), local tempo spread, audible content, continuation, meter/bar evidence and bounded natural-speed correction (default ±2%). The transparent ranking weights encode an initial preference, not a scientific probability of a good mix. The list includes technique diversity and abstains when no feasible option exists.

Collection membership and analysis readiness are separate. Explicit `mix.index_scope` indexes every Spotify page without decoding audio, and coverage retains older tracks when the first browser page is reopened. Unanalysed tracks appear separately with source metadata. Suggestion reads do not start analysis or change playback.

`mix.prepare` protects a user-prepared or playing target, isolates an empty channel, loads the exact source, sets its planned entry, tempo, pitch/key lock and literal mixer controls, and validates the same recipe used by `mix.arm`. Native callback receipts reject intervening manual edits. `mix.preview` auditions only the isolated incoming channel on headphone cue; stopping restores and revalidates its entry. Arming requires explicit passage review. Every audible transfer requires a ready, playing, advancing incoming carrier with bounded phase error. Recovery preserves the healthy outgoing carrier. `mix.status` exposes receipts and invalidation; `mix.cancel` cancels owned pending work without stopping ordinary playback.

An optional `mix.session.start` with a prepared pair and duration owns an application monotonic finish time across deck reloads. The native endpoint atomically yields to intervening manual input; audio/device/route failures interrupt it. It is assisted operation: the performer still selects/prepares/arms successors. It does not claim autonomous hour-long curation or the ability to continue from an unavailable source.

Continuity safeguards cover an **armed supported transition**. A prepared but unarmed option and the session finish clock do not launch a successor automatically. An application supervisor that prepares replacements and chooses fallbacks across repeated handovers remains future continuous-mode work; the assisted feature cannot promise that an unattended, unarmed deck will never reach EOF.

The native engine also exposes explicit Sync rate/slew limits for recipe-specific policies, preserving normal variable-tempo Sync outside those policies. The current recipes deliberately use explicit fixed tempo and reject cumulative drift; they do not chase uncertain local grids.

## Verification

Focused planner, coverage, execution, session and native render regressions cover absent evidence, silence, wide tempos, expired windows, source/control races, rejected launches, carrier recovery, raw provider pagination, independent session endpoints and manual takeover. Browser fixtures exercise real UI rendering, three techniques, preparation, isolated preview, reviewed arming, cancellation, indexing, filtering and the live countdown. These are operational checks. Real listening quality, hardware headphone isolation, live provider indexing and comparative expert preference tests remain unmeasured.

## Explicit repertoire limits

The panel lists fourteen technique families with concrete capability reasons. Independent echo/reverb returns and tail lifetime, loop-aware source clocks, beatless/tempo-curve contracts, validated vocal/structure/tonal models, stems and deterministic performance gestures remain unavailable. The implementation does not substitute another recipe under those names. Derived boundary proposals are a bounded in-memory cache; persistent proposal promotion/editing and learned preference feedback are future work. The full research roadmap remains in `next-track-and-transition-planner-2026-10-19.md`.

## Actual local-library validation

A read-only SQLite backup of the user's **3,403-track local library** supplied isolated tests. Original song files were read directly; original cues, tags and play history were not edited. Both physical output gains stayed muted and both transports stayed paused during API/UI checks. No Spotify requests were needed.

An unrestricted `mix.options {provider:"local"}` request showed **3,403 indexed, 125 analysed, 3,278 needing analysis**. It inspected a bounded shortlist of 24 tracks and returned **12 options from 12 distinct sources across all three techniques**. Examples included Anyma's *Explore Your Future*, *Midnight*, *Apollo 11*, Andrew Meller's *Born Slippy (Reincarnation Mix)*, *Roped In* and *Otherside*. Cold service time was **202.92 ms**; eleven warm requests had a **132.46 ms median / 150.12 ms maximum** on this busy host. Moving the reference to 180s and 350s recomputed the options. *Pantheon → The Rapture Pt.III* returned no option after nine outgoing pulse/energy window rejections; this is a feasibility abstention, not a claim about musical incompatibility.

Exact compiled recipes for **Monolink — Return to Oz (ARTBAT Remix) → Andrew Meller — Born Slippy (Reincarnation Mix)** were rendered offline from the actual MP3s through the native engine/protection pipeline:

| Recipe | Rendered span | Executed actions | Peak | Minimum 100ms RMS | Maximum incoming speed correction |
|---|---:|---:|---:|---:|---:|
| EQ/layer blend | 38.9167s | 6 | 0.56887 | 0.01100 | 0.5720% |
| Filter blend | 23.4267s | 6 | 0.57978 | 0.02093 | 0.6089% |
| Short exchange | 15.6855s | 3 | 0.68816 | 0.01843 | 0.6089% |

All three maintained a contributing carrier on every rendered frame, produced finite samples, had zero silent 100ms windows, executed every scheduled action and ended with the incoming deck playing at full fader and the outgoing deck paused/closed. These prove execution/continuity of these windows; perceived phrasing, vocal compatibility and sound quality still require listening review. Native UI inspection verified the actual Suggest panel populated from the full local library, in addition to browser interaction tests.

Final functional gates: **443 Rust unit/integration tests** and **70 frontend tests** passed; strict workspace Clippy, frontend production build, control-catalog coverage and debug desktop/CLI builds passed. The explicit real-library compiler export and native three-recipe render tests also passed. Model/corpus-dependent tests remain opt-in. One existing 100,000-row SQL timing benchmark exceeded its 100ms budget under heavy unrelated host load (115.8ms combined query; 198ms cold query on repeat); it was retained unchanged and excluded from the functional rerun. No claim is made that this performance budget passed.

Reproducible metadata/recipe manifests and summaries are in `artifacts/suggest-real-library-2026-10-19/`; they contain no rendered/exported song audio.

A supervised cleanup check against the copied full library returned normal status in 5,745 bytes (compact: 5,131 bytes), with the isolated process remaining healthy. This exposed two issues now fixed: planner/status reads were missing from the explicit read classification, so refreshes journaled their results, and `app.stop` started its exit timer before journal persistence finished. Planner/status reads now accept field projections and do not write the journal; shutdown starts after persistence and acknowledgement construction.

The final isolated regression made twelve unrestricted local-library requests and a normal CLI `mix.options --fields /options` request, each returning twelve options. The 2,443,003-byte journal retained its exact size, modification time and SHA-256. Warm request wall time had a 163.55 ms median (planner time: 143.64 ms) on this run. `app.stop` returned its successful acknowledgement and the process exited with code 0. Details are recorded in `read-only-regression.json`. The protocol's generic incomplete-frame message also mentions its 4MiB cap; the earlier cleanup failure was not an oversized status response or evidence of a library-size crash.

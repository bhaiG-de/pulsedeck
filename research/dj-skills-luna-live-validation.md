# Three-skill live Luna evaluation — 15 October 2026

Follow-up: the native abort was subsequently fixed and a fresh blind set completed without observed gaps. See [native launch and continuity retest](native-launch-and-gapless-set-validation.md). This report retains the earlier failure as historical evidence.

The blind live run failed the continuity and phrasing checks. It played three liked songs, but its first blend was two beats out at the bar level and its second launch failed. Both decks then stopped for approximately 70–73 seconds before the prepared successor resumed. Changes below address the observed skill failures; a further live evaluation is needed to establish improved behavior. The underlying native launch abort has not been root-caused or fixed in application code.

## Protocol and evidence

Each independent `gpt-6-luna` agent received only:

> Play a 15-minute DJ set using my Spotify liked songs.

No parent history, coaching, follow-up prompts or timing hints were supplied. The parent observed public command actions, native feedback, compact snapshots roughly every three seconds, and source metadata. The parent did not operate playback during the trial. At the user's request, the app was reopened before the second attempt; the user subsequently completed Spotify sign-in. No audio recording was made. Musical conclusions below distinguish grid/arrangement evidence from listening; user feedback independently reported the audible gap.

[Evidence directory](../artifacts/skill-trials/luna-three-skills-20261014T223959Z/) contains `manifest.json`, frozen `skills-before`, `skills-round-3` and `skills-after`, raw status/events, loaded metadata, scoped liked-song results, phase/band evidence, final native jobs and `assessment.json`. The evaluated performance used the round-three snapshot, before the final revisions described here. Full agent sessions remain in the local Codex session archive; `agent-round-3-path.txt` identifies the performance transcript. No private reasoning is reproduced.

## Attempts

1. `spotify_liked_set` inferred sign-out from `account:null`, `dj_streaming_available:false` and `playable_in_deck:false`, without attempting a normal isolated load. It also interpreted compact `track:null` as unloaded. These conclusions were unsupported by this build's API semantics.
2. `spotify_liked_set_restarted` repeated the access diagnosis and initiated sign-in. The user completed it. Before attempt three, the library/control instructions were corrected: inspect native connection/permission/errors and the actual guarded load result; read compact source identity from `metadata.tracks`. Those corrections enabled actual playback in the next attempt, although the intervening user sign-in means this is not a controlled attribution of causality.
3. `spotify_liked_set_signed_in` played the set, first wrestling with queue ownership/scheduling, then completing one blend and failing the next. It eventually restarted the final track and deliberately closed the bounded set.

The observed selected sources were all present in the liked-song response:

| Track | Requested Spotify ID | Musical metadata |
|---|---|---|
| Waterstone, Amanda Wilson — It's Over Now - Extended Vocal Mix | `76sTVWN1GTofI77lz4nlxk` | ~121.99 BPM, B minor / 10A |
| Lane 8, Rae Morris — No Fun | `5ImCVtO1gvcD1ttdG5SrQT` | ~122 BPM, G major / 9B |
| The Shapeshifters, Tripolism — Lola's Theme - Tripolism Extended Remix | `2RQ1LEpUNpbzEX3x2ApMpD` | ~124.01 BPM, 9B |

## First blend: transport worked, phrasing claim failed

Luna selected A's outro at **346.267s**, effective beat **704**, and B entry near **14.75s**, snapped to **14.758s**, beat **30**. Its stated B arrival was **46.223789s**, beat **96**. It called the approach 16 bars / 64 beats.

The actual difference is **96 − 30 = 66 beats**. With the inspected 4/4 origins, A704 is a downbeat while B30 is beat three of its bar. Simultaneous native samples showed mostly small fractional beat error but approximately **two beats of bar offset**. One local irregularity also produced a larger transient beat error. Sync did not establish correct bar or phrase progression.

Native job `job-38884-40` establishes what happened:

| Completion | A source seconds | B source seconds |
|---|---:|---:|
| `launch-b-time` | 346.267042 | 14.758069 |
| `open-b-time` after 31.47s ramp | 377.737042 | 45.248145 |
| `transfer-b-time` | 377.768526, EOF | 46.767598 |

Thus the ramp/transfer straddled B's nominated arrival, and A reached EOF. Continued B playback was insufficient evidence for Luna's claim that the first main groove had landed on a verified 16-bar handover. The two songs also have different key labels, despite Luna describing them as nearly the same tempo and key. A harmonic clash cannot be established from labels alone.

The overlap inspection exposed a separate musical issue: A's low-band RMS dropped from roughly **0.401** to **0.0078** around **362.002s**, midway through its outro. B contained low-band energy around **0.31**, but its low EQ remained zero until the final transfer. This predicts a loss of bass foundation during the last portion of the blend. It is feature/control evidence, not a measured listening-quality score.

## Second launch: failed action, missed recovery budget

Lola's Theme was loaded on isolated A at revision 2 and prepared near **240.0786s**. The planned launch against No Fun at **375.579s** used inner `play_quantized` scheduling combined with an outer absolute condition. That timestamp was about **0.160s before** the nearby inspected downbeat at **375.739105s**. The plan's later incoming “groove” did not identify and substantiate a specific arrival as clearly as the first plan.

The native action failed with **`launch_aborted`** and reported **35.5686 seconds** of outgoing source headroom. Dependent opening/transfer actions failed; the overall session could still appear running. This establishes the failure and available runway, but does not prove the root cause of the native abort. Earlier attempts also mixed persistent sessions and standalone launches and encountered ownership/schema errors.

Luna spent that remaining runway inspecting commands and stopping/replacing automation. No Fun reached EOF while A was still paused, muted and bass-cut. The observer first saw both stopped at **22:54:57 UTC**. The fallback eventually restored A's EQ/fader/master state and plain-played it from **271.0504s** around **22:56:07 UTC**. Sampling bounds the actual transport gap to **69.985–73.050 seconds**; Luna's own report rounded it to 69 seconds.

The fallback then deliberately faded Lola's Theme and paused A at **434.994572s**. B remained at EOF. No fourth track was loaded because Luna had chosen its closing passage for the requested bounded set; that final close is distinct from the earlier unintended stall. Renderer frames measure **975.038s (16:15.038)** from first play to final pause. Wall-clock receipts give **975.239s**. Summing source-content durations is not elapsed performance time and ignores rate changes. Both decks were paused at completion.

## Revisions made from the failures

- **DJing — the art:** now starts from a changing source-specific musical view. A new data-to-decisions reference maps beat events, bar origin, section provenance, local tempo/rate, key, waveform bands and execution feedback to musical choices. The core requires actual interval subtraction and bar checks, a justified arrival, layer ownership through internal changes, musical deadlines, and explicit revision after failed execution. It distinguishes continuity recovery from a phrase-aligned mix.
- **Controlling PulseDeck and workflows:** now includes a minimal plain-play plus outer-timing example, one queue owner, full event precision, failed-action inspection even when the job remains running, prompt recovery with known controls, inherited mute/EQ checks, and preparation of each released deck. It separates a deliberate bounded close from abandonment of a continuous set and tracks wall elapsed time.
- **Manage Library:** now specifies bounded evidence packets with complete beat-event intervals, exact timestamps, sections/cues and neighboring bands. It warns against guessed JSON paths, truncated beat data and average-BPM phrase counts. Native Spotify placeholder semantics remain documented.

No application code was changed for these revisions. They reduce avoidable planning/operation mistakes; they do not establish that the unresolved runtime abort is cured.

## Validation and limits

All three skill frontmatters validate. The package validator resolves all local references and parses 26 examples. All seven packaging/relocation tests pass, including copying/rebasing the new cross-skill reference. Whitespace checks pass.

These checks establish packaging and example syntax, not improved live musical behavior. The final revisions have **not** been tested in another live set. The previous seven-skill validation JSON remains historical evidence and is not treated as validation of this rewrite.

## 18 October follow-up: stopped before first handover

The requested hour-long local 120–128 BPM set was stopped by the user after A (`Beautiful Now`, duration 252.578s) ended with B (`Summer (Extended Mix)`) still paused and isolated. The agent's postmortem confirms that no B launch or blend ramps were ever registered. B load, analysis and preparation succeeded; this was a failure to arm a transition. The engine reported zero callback deadline overruns.

Proposed A launches at 136.544s, 166.559s and 204.061s were already passed when checked (by approximately 0.280s, 2.656s and 17.817s). One estimated timestamp also failed exact-event validation. The final B entry seek to 57.322s executed only after A reached EOF; the final check reported an ended outgoing clock. The agent consumed its runway inspecting hundreds of beat events/waveform bins and retrying nearby launch windows. It initially fetched 1,000 library rows despite having fast indexed search available.

On stop, the supervisor interrupted the agent, wrote its stop flag, terminated its actual CLI process (PID 2655), stopped automation and explicitly paused both decks. The writable tool handle 76101 was not an OS PID. Resume status showed no active jobs. Playback was not restarted.

Focused corrections: expose `--filters` on the friendly `library tracks` command, use native text/BPM/classification filtering with small candidate pages, retain raw timing evidence while returning compact summaries, budget model/tool time before selecting launches, and distinguish prepared/armed/launched/audible/cleared states. The session reference now requires a preparation margin and a fallback deadline rather than chasing successive missed boundaries. CLI tests and a read-only filtered query verify the search binding; packaging checks verify the skill references. These changes have not been validated by another live mix.

Validation: 26 CLI tests and seven skill packaging tests passed. A read-only `Summer` + BPM 120–128 + rating ≥3 query returned the expected matching record. The first measured CLI process-to-result time was 238.09ms; five repeats were 18.59, 11.33, 12.45, 10.63 and 10.37ms. This does not certify cold search below 100ms; startup, IPC, app scheduling and SQL were not separately profiled in this follow-up.

## 18 October fresh-agent observation and concrete interface fixes

A fresh Luna received only “Play a mix with good beats for an hour while I work.”
It started FISHER’s “You Little Beauty” at native frame 6,692,920. The first setup
commands were not applied until A reached 260.078s. Earlier atomic plans were
rejected for a missing EQ `mid` field, an unknown dependency, and grouping a seek
with the absolute BPM setter (which reads that position). Disjoint fader/EQ ramps
on one deck are supported; the agent's broader claim that all same-deck grouping
was prohibited was incorrect.

The accepted launch for A=222.656s failed at A=260.238s. Its replacement targeted
A=303.959s and failed at A=345.339s. Both were already in the past at submission.
Sync was never enabled. A reached EOF at 372.715s; B's immediate fallback began
afterward with its channel closed, then faded up. One-second passive transport
samples place the stopped-both-decks interval at about 10 seconds. No positive
two-deck overlap occurred and audio overruns remained zero. The user stopped it;
the supervisor cancelled automation, paused both decks and terminated its session.
Raw observations and complete job receipts are in
`artifacts/luna-fresh-observation-2026-10-18/`.

The load responses had flooded the agent with dense analysis output (its tool
reported approximately 171,715 original tokens for the pair). The direct load
path now reuses the existing compact control-result projection by default;
`detail:true` preserves full responses and all underlying analysis remains intact.
Targeted engine-action discovery and retained concise error examples reduce
catalog/schema hunting. The CLI can validate the exact atomic session append
before Play using the existing backend validator. The checker’s explicit paused
preflight retains musical-coordinate/grid/Sync requirements and reports
`execution_ready:false`. The skills now prepare and validate the complete first
pair while paused, then submit opener Play and its whole handover together.

A muted isolated rehearsal used the real cached “The Rapture Pt.III” and
“Temperament of the Beat” analyses, an atomic initial plan, follower Sync,
gradual incoming faders, a paired bass exchange and outgoing clearance. Both
sources contributed over a sampled 38.911-second span; the outgoing deck paused
only after clearance and the incoming continued playing. Audio overruns were
zero. Compact ready responses were 3,067/3,070 bytes; an explicit full response
was 390,495 bytes and retained 4,096 waveform bins and the complete beat events.
Evidence: `artifacts/mix-preflight-2026-10-18/`. This validates execution and signal
contribution, not physical speaker audibility or subjective musical quality.

A further fresh Luna was dispatched with the same uncoached one-hour prompt on
the updated normal app. Its live result is still pending; the muted rehearsal is
not a substitute for that live behavioral validation.

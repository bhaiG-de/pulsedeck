# Prepare an incoming deck

Read the [CLI](../../references/cli.md) for endpoint binding and control stages. Obtain the musical entry, playing tempo/key strategy and intended later arrival from `$djing-art`. Preparation-only work leaves the deck paused and queues no launch. An audition request can start its transport only after appropriate isolation and monitoring are verified.

## Inspect availability and deadlines

Use a compact snapshot to establish both sources/load revisions, playing/position and position revisions, rate/Sync/master, loops, faders/EQ/filter/FX, cue selection, crossfader/global levels, routing and active ramps/source jobs. Compare remaining live audio with observed preparation latency and a fallback boundary.

Respect the explicit destination. A paused deck may contain the user's next track, audition or pending load. Two contributing decks are not available to replace. Never use `--replace-playing` for ordinary preparation; source guards do not protect against every concurrent manual control change.

## Isolate, load and prepare

1. Close the incoming channel only, using its observed source guard. Native control smoothing settles across rendered frames; an `applied` target is not instantaneous isolation. Keep the live channel, crossfader, master gain, routing and role intact.
2. Load the selected real source. A synchronous result reaches `ready` with a new `load_revision`; asynchronous acceptance requires observing its job/readiness. Capture the new revision before subsequent guarded controls. In a persistent queue, dependency completion captures actual guards; do not guess a future revision. Use the [loaded-source readiness check](#loaded-source-readiness) before treating preparation as failed.
3. Disable stale incoming loops and apply the intended clean starting filter/EQ/FX, key lock, tempo/Sync and final entry under that source guard. Preserve permanent cues unless their edit was requested. An ordinary seek preserves play state; a hotcue trigger starts transport.
4. Check loaded-source readiness (`preparation_pending:false`), intended paused position, rate/Sync/key lock, loop state and closed fader. A current bounded native processor does not require a whole-track pitch-worker job; do not keep waiting for a nonexistent worker.
5. Enable the requested cue selection, verify actual discrete route and suitable cue mix/gain. Do not silently alter global cue controls or device routing. Start headphone preview only if requested and isolated. On a two-channel device with no distinct cue route, report cue-enabled status separately from headphone readiness.

If Smart CFX couples filter and FX, inspect resulting incoming FX after a filter reset. Moving the crossfader with Smart Fader may alter both decks' tempo/effects; do not use it as an isolation shortcut.

## Loaded-source readiness

After load completion, read the current deck in `status`: `source_state:ready`, `track.path` matching the selected source and `engine.load_revisions[deck]` identify ready material. `metadata.tracks[deck]` supplies the same compact identity. `preparing` means the source is loaded but processor preparation remains pending; `metadata_pending` means the transport and metadata epochs have not converged yet. Re-read those transient states instead of declaring a provider failure. `empty` with `track:null` means no current source.

Then `pd deck B wait --expected-load-revision "$B_REV" --timeout 30s` verifies preparation and returns the current `source` and `load_revision`; an empty deck fails. Verify that returned source against the intended URI/path. A source job describes the latest provider request, which can differ from a previous source still on the deck. Its `preparing:false` alone is not proof of readiness. Its `resolved_uri` is the actual provider release, including the requested URI when no relinking happened; it is not an audio-health flag. Respect an actual load error or source mismatch.

Once the intended loaded source passes readiness and audio rendering is running, an authorized set can proceed with its prepared opener or incoming launch. A paused deck has zero meters until played; Connect device discovery is unrelated to whether native PulseDeck audio is loaded. Do not abandon a ready source based on account placeholders or optional provider fields. Check advancing playhead and output meters after the authorized Play to establish software playback.

## Playback tempo and Sync

Use `deck B tempo set --bpm TARGET` for a chosen absolute entry tempo. It derives rate from the current local four-beat fit at execution, preserves source analysis/grid and takes manual ownership by disabling follower Sync. `--percent` sets absolute source-relative rate percent, not an increment. Check `target_playback_bpm` and `actual_playback_bpm` separately; source `bpm` is metadata. One entry setting does not lock a variable-tempo track's future BPM.

`deck B bpm set` is instead a persistent grid correction; do not use it for matching. Inspect local pulse, ambiguity, key-lock/pitch state and relevant actual events. Choose ordinary small stretch from a suitable successor before compensating for a poor pair. Suggestion defaults near 3%, or a deliberate wider choice, are preparation heuristics rather than guarantees of sound quality.

For ongoing local synchronization, establish a loaded master and a distinct loaded follower on credible grids. Both can be paused during first-pair preparation: `pd deck B sync on` / `{"type":"set_sync","deck":"B","enabled":true}` arms the paused follower. A playing follower requires its master to be playing with jog released. Seek the final entry before setting its entry tempo; tempo setting disables Sync, so enable Sync last and verify roles before launch. Use `pd commands describe engine.command --action set_sync` for the exact action schema. Sync preserves position and adapts smoothed local tempo and beat phase. It does not establish bar or phrase identity. Keep the chosen strategy stable through launch rather than repeatedly toggling Sync or chasing local BPM readings.

Preserve the current live master unless the performance plan calls for handoff. Loading/unloading the master can promote a healthy playing other deck; pausing it alone does not. Inspect actual roles after a source change. Sync-off/promotion folds the current effective rate into nominal control for continuity; re-read the resulting target rather than assuming the old number.

## Bounded standalone example

Only after B is available, A is protected, `B_OLD_REV` is observed, and `TRACK_SOURCE` (an observed `local:ID`), `ENTRY_TIME` and `TARGET_BPM` are selected. Provider loads instead use the discovered URI form; this example is for a local record:

```sh
set -e
task_prep_dir=$(mktemp -d)
pd deck B fader set 0 --expected-load-revision "$B_OLD_REV"
pd deck B load --track "$TRACK_SOURCE" --expected-load-revision "$B_OLD_REV" > "$task_prep_dir/load.json"
B_REV=$(python3 - "$task_prep_dir/load.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
assert r['ok'] and r['stage'] == 'ready', r
print(r['result']['load_revision'])
PY
)
pd deck B seek --at "$ENTRY_TIME" --expected-load-revision "$B_REV"
pd deck B tempo set --bpm "$TARGET_BPM" --expected-load-revision "$B_REV"
pd deck B keylock on --expected-load-revision "$B_REV"
pd deck B loop off --expected-load-revision "$B_REV"
pd deck B wait --expected-load-revision "$B_REV" --timeout 30s
pd status --view compact
```

Check every response and stop on failure. This example uses manual entry tempo, not follower Sync, and leaves playback paused. In a running set, queue these known actions through the existing persistent process with load/readiness dependencies; do not invoke a new CLI process for each timed gesture.

## Verify and recover

Compare source/revision, entry, paused/play state, readiness, loop, tempo/Sync/key lock, isolation and cue state to the plan. Verify A's source and position revision are preserved, position advances normally and protected controls/global routing remain unchanged. Cue selection alone cannot verify headphone sound.

On failed load, keep B isolated, preserve A, inspect whether the previous B source survived and use a supported alternate before the live deadline. On timeout inspect jobs/state before retrying. Job cancellation may not stop an active provider load; use `deck B load cancel` when required and verify actual source-job completion. Do not restore cached controls over an unexplained manual change.

Queue preparation as dependent steps: entry seek → tempo setter → Sync enable. Seek and the absolute BPM setter conflict in a simultaneous group because BPM reads the current source position. Disjoint fader and EQ ramps may share a group, including on the same deck. Generic `set_eq` needs all three `low`, `mid`, `high` values; the friendly `deck B eq set --low` command provides separate partial-band semantics.

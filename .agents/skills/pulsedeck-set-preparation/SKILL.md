---
name: pulsedeck-set-preparation
description: Select, compare, and order PulseDeck tracks for a requested set, propose compatible followers, and save an ordered local playlist when requested. Use for set planning and playlist preparation; use transition/session skills for live mixing.
---

# Prepare a PulseDeck set

Turn the user's intent into an explainable ordered selection. PulseDeck supplies estimates and executes playlist edits; you make the musical decisions. Planning does not authorize playback, cue edits, or playlist changes. A request to build/save/edit a playlist authorizes those playlist operations without approval at each step.

Read [CLI operation and response semantics](../references/cli.md) for endpoint setup, JSON errors, stages, guards, and jobs. Read [musical evidence](../references/musical-evidence.md) when judging analysis or transition regions. Examples below use the shared `pd` shell function: it invokes the selected `pulsedeckctl` instance with `--json`.

## Observe before selecting

Discover the implemented methods, then inspect the session and available material:

```sh
pd commands list
pd commands describe library.tracks
pd commands describe library.track
pd commands describe playlist.edit
pd status
pd playlist list
pd library tracks --sort bpm --limit 100 --offset 0
```

Follow each non-null `result.next_offset` until enough eligible material has been inspected. Search matches title, artist, BPM text, and key text; it is not a semantic mood search. `--playlist` accepts an observed playlist ID or `local:ID` and restricts membership. **Its result is still sorted by title/BPM/duration, not playlist order.** Reconstruct a source playlist's order from `playlist.list` → `track_ids` and join those IDs to track records.

Gather title, artist, ID/path, duration, BPM, BPM confidence/ambiguity/candidates, key/Camelot/confidence/alternatives, and playlist membership. Default `library.tracks` omits waveforms while retaining analysis fields. Get a selected track's detail with `library track get local:ID`, where `ID` comes from the observed record, or use the track-inspection skill for timestamped waveform evidence. Get durable cue metadata through `cues.get` rather than assuming the list contains cue labels. When choosing a follower for deck A/B, use `status` for its source, position, effective tempo, and load revision, then `engine.metadata` for confidence/detail if needed. The library's original BPM is not necessarily the live deck's effective tempo.

Do not load a track merely to organize a playlist; set preparation can remain inaudible and leave both decks untouched. If duration/confidence/key is unavailable, show the missing evidence and choose a conservative plan rather than manufacturing it. Empty artist strings and null keys are unknown, not evidence of instrumental or atonal material. A confident BPM cannot fill that gap.

## Make musical choices

Respect explicit order, exclusions, preferred tracks, style, duration, and transition preferences. Rank candidates by the user's intent, not one universal score:

- For a gradual tempo progression, compare adjacent original BPMs and the proposed playing BPM. Required adjustment is `100 × (target BPM / original BPM − 1)` percent. Prefer changes manageable for the material and explain larger changes; the supported numeric range does not establish musical acceptability. Flag likely half/double-time ambiguity before using a BPM as a sequencing fact.
- When useful, compare known tonal keys or Camelot codes: same key, neighboring wheel number with the same A/B letter (including 12↔1), or same number across A/B are candidate compatible moves. They are guides, not guarantees; key confidence, modal/atonal material, arrangement, and overlapping notes can make a nominal match clash. Do not infer key from a title or assert compatibility when missing.
- Use energy/mood only when supplied by the user, stored meaningfully, or supported by observations. Relative amplitude and bass/band patterns can suggest arrangement density or contrast; normalized waveform levels do not establish absolute loudness, perceived energy, genre, vocals, or emotional mood. Say what was observed and what was inferred. Do not claim to have listened.
- Consider workable entry/exit regions, cue availability, duration, bass overlap, and confidence. A moderate BPM difference can still make a hard blend; uncertain grids or sparse evidence favor a shorter/simple transition. Mark adjacent pairs that need detailed inspection or audible checking.
- A beatgrid origin does not prove a downbeat or phrase start. If using bar counts, declare the meter assumption; 8/16/32-bar structures are candidates rather than guaranteed sections.

For a duration target, distinguish the sum of full track durations from an estimated played set. With per-track played windows `[entry_i, exit_i]` and consecutive overlaps `overlap_i`, estimate:

`set seconds = Σ(exit_i − entry_i) − Σ(overlap_i)`

Use nonnegative in-range windows and overlaps supported by both tracks. At constant playing BPM, overlap seconds = `beats × 60 / BPM`; with assumed 4/4, bars = beats/4. If tracks are tempo adjusted, scale original-time played windows by `original BPM / playing BPM`. State assumed cue cuts, overlaps, meter, and tempo changes, including temporary beatmatching changes omitted from arithmetic; do not call this an actual performance duration. Report a range only when its endpoints follow from stated window/overlap alternatives, not a fabricated confidence interval. If the library cannot support the target, report the achievable length and shortfall instead of silently repeating tracks: local playlists cannot contain duplicates.

## Expected plan

Return the ordered IDs/titles/artists with relevant BPM/key confidence and a short reason for each choice. For every neighboring pair, explain likely transition difficulty, suitable overlap/candidate regions, and uncertain evidence. Include total source duration, estimated played duration/range with assumptions, exclusions/shortfall, and any proposed playlist mutation. For “find five tracks that could follow A,” return ranked options without changing the active set.

## Save only the requested playlist change

Use `playlist create --name NAME`, `playlist add ID --track local:TRACK_ID`, or `playlist reorder ID --tracks '[INTEGER_IDS]'`. Reorder maps to `playlist.edit action=replace`: it replaces the **entire membership and order**, not just positions. Preserve unmentioned existing members unless the user requested a reduced selection. IDs in `--tracks` must be unique integers of existing local tracks; `local:42` strings are not accepted in that array. Spotify membership cannot be written through these local playlist operations.

Read the target playlist immediately before editing. There is no playlist revision/CAS guard: if someone changes membership during preparation, re-read, reconcile with the request, and avoid overwriting the manual change. Duplicate tracks currently return `operation_failed` rather than an argument-specific code; read its explanatory message and correct the order. On uncertain create/mutation transport results, inspect state and use the same request ID for the identical retry within the receipt window; blindly repeating create can produce another playlist. An edit's `applied` stage is followed by re-querying `playlist.list` to verify exact `track_ids`, and querying membership to verify every intended track exists. Do not use sorted `library.tracks` to prove order.

### Planning example: propose a rising-tempo local practice set

These reads save real response envelopes in a temporary directory. They make no playlist or deck changes. Choose an order after evaluating the results; a BPM sort is evidence, not an automatic musical verdict.

```sh
set_work_dir=$(mktemp -d)
pd status > "$set_work_dir/status.json"
pd playlist list > "$set_work_dir/playlists.json"
pd library tracks --sort bpm --limit 100 --offset 0 > "$set_work_dir/tracks.json"
python3 - "$set_work_dir/tracks.json" <<'PY'
import json, sys
response = json.load(open(sys.argv[1]))
assert response['ok'], response
page = response['result']
for t in page['tracks']:
    print(t['id'], t['title'], t['artist'], t['bpm'], t['bpm_confidence'],
          t['key'], t['key_confidence'], t['duration_seconds'])
print('next_offset:', page['next_offset'])
PY
```

### Execution example: save the chosen order without starting playback

After the user requests saving a set, write `order.json` under the same temporary directory as a JSON array of the observed integer track IDs in your musically chosen order. The following code validates that file against the inspected page and derives the created playlist ID; it does not invent IDs. If selection spans several pages, validate against their combined records instead.

```sh
set_order=$(python3 - "$set_work_dir/order.json" "$set_work_dir/tracks.json" <<'PY'
import json, sys
order = json.load(open(sys.argv[1]))
page = json.load(open(sys.argv[2]))
assert page['ok'], page
available = {t['id'] for t in page['result']['tracks']}
assert isinstance(order, list) and order and all(type(i) is int for i in order)
assert len(set(order)) == len(order) and set(order) <= available
print(json.dumps(order))
PY
)
pd playlist create --name 'Prepared practice set' > "$set_work_dir/created.json"
set_playlist=$(python3 - "$set_work_dir/created.json" <<'PY'
import json, sys
response = json.load(open(sys.argv[1]))
assert response['ok'], response
print('local:' + str(response['result']['id']))
PY
)
pd playlist reorder "$set_playlist" --tracks "$set_order"
pd playlist list > "$set_work_dir/verified.json"
python3 - "$set_work_dir/verified.json" "$set_playlist" "$set_work_dir/order.json" <<'PY'
import json, sys
response = json.load(open(sys.argv[1]))
assert response['ok'], response
playlist_id = int(sys.argv[2].split(':')[-1])
actual = next(p for p in response['result'] if p['id'] == playlist_id)
assert actual['track_ids'] == json.load(open(sys.argv[3])), actual
print(actual)
PY
```

## Failure handling and manual takeover

Inspect `ok`, error code/details/hint, and any job before retrying. Bad IDs, duplicate order entries, or invalid options need a corrected plan/request, not repeated retries. Replacement is transactional: on failure, re-read and verify the previous playlist survived; a prior successful create/add is a separate mutation and is not rolled back. For missing local files or weak analysis, exclude/flag the track or choose another supported option rather than delay an active session with analysis work.

Do not pause playback or reset mixer settings to fix a playlist problem. On “stop”/manual takeover, stop issuing mutations and report the last verified playlist plus any uncertain action. Set preparation usually owns no active audio operation; `jobs cancel` prevents future job steps but is not proof that an already-running import/load ended. If preparation launched such a job, inspect its current status and use the operation-specific cancel documented in the shared CLI reference where appropriate. Use `automation stop` only when stopping session automation is requested; verify jobs/ramps afterward rather than promising rollback.

End with the saved playlist ID, verified exact order, estimated duration assumptions, and what remains to check by listening. Trial-generated fixtures validate metadata/order/runtime accounting and leave transport unchanged; they do not validate musical quality or harmonic compatibility of real tracks.

# Manual cues, cached suggestions and analysis

Use [CLI](cli.md) for the running endpoint and guards, and [timed execution](timed-execution.md) for scheduled performance. Discover only unfamiliar commands.

## Human-authored source knowledge

PulseDeck does not generate cues or classify song sections. BPM, beat/downbeat events, key and waveform bands remain available. A track with no saved cues has no annotated musical boundaries; do not diagnose that as failed analysis or request reanalysis to obtain drop/outro labels.

`cues.get` returns a revision and nested `cues` object with durable `points` and `bindings`. A point contains stable `id`, `start_seconds`, optional `end_seconds`, `name`, optional human-assigned `role`, `origin` (`manual` or `imported`), `modified` and `confirmed`. Main, hot and memory slots bind to those points. A saved cue records source knowledge, not a universally correct entry or exit for every successor.

```sh
pd call cues.get --args '{"deck":"A"}'
pd call track.inspect --args '{"deck":"A","start_seconds":8,"end_seconds":32,"points":128}'
```

Replace ranges from observation. Cue endpoints are source times; changing playback rate does not rewrite them. Inspect neighboring waveform changes and effective events, and listen when available. Human labels do not prove vocal absence or the best musical relationship for this pair.

Use `cues.set` only for an authorized edit. `point` creates/edits a named point (include `id` to edit), `delete_id` deletes it, and `bind:{kind,index,id}` binds an existing point. Use one form per request, current `expected_revision`, and a loaded target's `expected_load_revision` when applicable. Preserve unrelated points and bindings. Permanent cue creation is not an automatic side effect of planning or performing a set.

## Scoped candidates

```sh
pd library suggest --deck A --energy keep
pd call library.suggest_next --args '{"deck":"A","playlist":"spotify:liked","max_stretch_percent":3,"allow_half_double":false,"limit":20}'
```

Directions are `keep`, `raise`, `lower`; friendly `up`/`down` normalize to those values. Honor collection membership and requested order. Missing scoped cache does not authorize searching the whole collection. Browse early and bound the shortlist before a live deadline.

Suggestions use cached tempo/key/waveform features. They do not start analysis, detect phrases or choose the transition. Inspect actual playing tempo, source identity, stretch, key confidence, meter, availability and exclusion reasons. Unknown data stays unknown. Half/double relationships require musical pulse and phrase review. Do not clear played history merely to obtain more candidates.

## Reuse and refresh

```sh
pd library analysis update --no-wait
pd library analysis cancel
```

Updates require the user's analysis intent. Passive reads/loads reuse compatible persisted BPM/rhythm, neural timing, key and waveform stages. Explicit reanalysis can retry a failed stage; it preserves manual grids and cues. Retain job identity and inspect terminal completion/errors. Cancellation does not undo completed writes. Provider preparation may require its own `deck.load.cancel`.

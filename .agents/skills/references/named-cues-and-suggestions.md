# Source anchors and scoped candidates

Use the [connected MCP tools](mcp.md) and actual returned fields. PulseDeck supplies tempo, beat/downbeat events, key and waveform features; it does not automatically create cues or classify song sections. An unannotated track is not a failed analysis.

## Human-authored knowledge

`cues_get` returns a revision and nested `cues` object containing canonical `points` and `bindings`. A point has stable `id`, source start/end, name/role, manual/imported origin, modification and confirmation. Main, hot and memory slots bind to those points. A saved cue is evidence about a source, not a universally correct mix entry.

Use `cues_get` arguments `{"deck":"A"}` or `{"source":"OBSERVED_SOURCE"}`, exactly one target. Compare surrounding `track_inspect` observations and provenance. Cue times are source seconds; playback tempo does not rewrite them. Labels do not establish vocal absence or the best relationship for this pair.

Arbitrary named-point creation/editing/deletion and binding changes require a metadata-write tool that is not exposed by the current server. Report that missing capability. Explicit loaded-deck slot setters such as `set_hot_cue_point` are available through `deck_control` when the request covers that edit. Use current `expected_load_revision`, preserve unrelated slots, and verify the canonical result with `cues_get`. A cue trigger starts playback; it is not a metadata edit. JSON hot indices are 0–7 and memory indices 0–9; memory projections sort chronologically.

## Candidates remain choices

Use `library_suggest_next` with the requested scope and observed deck state. `mix_openers`/`mix_options` can supply alternative source/passage proposals. Cached suggestions do not analyse new audio, identify phrases or select the transition. Inspect actual playing rate, key confidence, pulse/meter, availability and exclusions. A half/double relationship needs musical review rather than just a ratio.

Preserve scope and played history. Missing collection cache calls for `mix_index_scope` or provider collection reads; it does not authorize widening repertoire. Passive reads and compatible loads reuse saved analysis. Bulk refresh/cancellation is not exposed through this MCP connection; a provider load has its own `deck_load_cancel` and actual source-job result.

---
name: pulsedeck-library
description: Find PulseDeck tracks and Spotify collections and inspect counted waveform structure and saved cue/grid evidence through MCP. Use for source knowledge and collection requests; pair with djing-art for musical decisions and pulsedeck-control for playback.
---

# Source knowledge for DJing

Use the connected tools in [MCP access and capabilities](../references/mcp.md). Return reliable source identity, collection membership and musical evidence without disturbing playback. Discover only unfamiliar schemas and use observed IDs/revisions. If a required tool is absent, report that missing capability rather than invoking a different transport.

| Request | Reference |
|---|---|
| Browse scoped repertoire, collections or a shortlist | [Collections](references/collections.md) |
| Decode a source's measured structure or inspect a passage | [Track knowledge](references/track-knowledge.md), [analysis semantics](../references/musical-evidence.md) |
| Read named source anchors or make an authorized exposed slot/grid edit | [Named anchors](../references/named-cues-and-suggestions.md), [track knowledge](references/track-knowledge.md) |
| Explain stored analysis and readiness | [Analysis and persistence](references/analysis-and-persistence.md) |

[DJing — the art](../djing-art/SKILL.md) interprets evidence and chooses the combined result; [PulseDeck control](../pulsedeck-control/SKILL.md) prepares and performs it. Complete the available authorized parts of compound requests, and identify any missing write/import/export capability precisely.

## Find repertoire for the musical idea

Preserve the requested collection, exclusions and played history. Match provider URI or canonical ID/path, artist/title, duration and version; a remix or relink can have different structure. Reuse complete indexed scope. `mix_index_scope` obtains missing membership; indexing is separate from audio analysis. An unanalysed song remains eligible and can be prepared on demand on an available isolated target. Do not wait for collection-wide analysis before the next deck can load.

Choose discovery for the question: `library_tracks` and provider collection reads for direction, `library_suggest_next` for a scoped shortlist, and `track_inspect` for actual passage relationships. `mix_openers`/`mix_options` are proposals, not required first steps or the limit of the agent's repertoire. Their rank, selected entry and technique remain choices for the DJ to assess. Keep the requested collection; an incomplete cache is not permission to widen the repertoire.

Maintain working source maps and used source IDs. This supports recognition, variety and deliberate callbacks without clearing played history or creating permanent tags/cues.

## Return a temporal shape

For relevant sources supply ordered counted windows: exact effective beat positions/timestamps, meter and grid provenance, low/mid/high and peak behavior, changes before and after the join, and the next sustained return/withdrawal. Preserve event coverage and source/analysis revisions. Missing named cues do not prevent provisional functional inference from recurrence, withdrawal, increasing activity and return.

Capture `track_inspect` tool JSON. The art skill's [offline passage condenser](../djing-art/scripts/passage_map.py) combines those saved packets into a compact counted trajectory, retaining exact anchors and provenance. It never calls the app. Complete truncated ranges with additional MCP inspections before relying on interval counts.

Separate measured local amplitude from rhythmic drive/tension and normalized played level. Raw band averages across different masters are not an energy ranking; a band is not a vocal/instrument label. Let the art skill judge the combined musical course and retain the evidence supporting each inference.

The control validator's transition `forecast` reuses cached loaded-source bands to project the scored exposure on one timeline. Use its source provenance, coverage and omitted ranges to target further inspection. It complements the source map: an unexposed return, a later development outside the sampled forecast or an uncertain loop/filter projection still needs musical interpretation from the relevant bounded source evidence.

## Preserve saved knowledge

Read-only inspection needs no deck load. A performance request allows source reads and necessary isolated preparation, including missing per-source analysis. It does not imply permanent metadata changes, playlist replacement or collection-wide reanalysis.

`cues_get` reads canonical points/bindings. Exposed native slot/grid setters use `deck_control` under current source guards; verify through canonical reads. Arbitrary named-point editing, playlist/tag writes, imports, bulk analysis and XML exchange are not exposed by this connection. Report that limit when requested, preserving existing metadata and playback. Do not replace arrays, trigger a cue or reload a contributing source as a workaround.

Inspect actual persistence/job results; acceptance is not completion. Reconcile a changed source or revision before retrying. Saved evidence, executed controls and listening quality establish different things.

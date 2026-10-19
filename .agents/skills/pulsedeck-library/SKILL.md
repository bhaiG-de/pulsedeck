---
name: pulsedeck-library
description: Find and organize PulseDeck local tracks and Spotify collections; manage playlists, tags, stored analysis, canonical cues/beatgrids and rekordbox XML. Use for source evidence and saved library knowledge; musical choices use djing-art and deck performance uses pulsedeck-control.
---

# Manage Library

Keep the collection and saved source knowledge accurate without disturbing performance. Bind the requested running instance with [CLI setup](../references/cli.md). Discover only unfamiliar methods and use observed source IDs and revisions.

## Select the work

| Request | Reference and responsibility |
|---|---|
| Browse/import, inspect membership, tag, shortlist or save order | [Collections and playlists](references/collections.md): records, pagination, scoped suggestions and verified writes |
| Inspect passages and return evidence | [Track knowledge](references/track-knowledge.md): bounded reads, manual/imported cues and exact events |
| Save a cue or correct an authorized grid | [Track knowledge](references/track-knowledge.md): provenance, revisions and narrow edits |
| Explain or refresh stored analysis, refresh measured features | [Analysis and persistence](references/analysis-and-persistence.md), [analysis semantics](../references/musical-evidence.md) and [named anchors](../references/named-cues-and-suggestions.md) |
| Exchange saved metadata | [XML exchange](references/xml-exchange.md): preview, partial import and export verification |

Use [DJing — the art](../djing-art/SKILL.md) to judge successors, phrases and set order. Supply source-specific evidence, not a supposed automatically correct mix point. Use [Controlling PulseDeck and workflows](../pulsedeck-control/SKILL.md) for loading, audition, transport and recording. Complete the authorized parts of compound requests.

## Preserve identity and knowledge

Match canonical ID/path or provider URI, artist/title, duration and version. Preserve the requested collection, membership, order and exclusions. For a continuous set, use scoped `mix.openers` for one analysed opener, then the control skill's detached successor queue. Do not manually shortlist all successors. For one individual move use scoped `mix.options`; it searches the indexed analysed collection and returns passage evidence. Avoid separately paging the latest liked songs or loading candidates just to shortlist them. If collection coverage is incomplete, `mix.index_scope` indexes all pages; indexing and audio analysis are separate. Use targeted reads for missing evidence so the active move can be submitted promptly. Do not substitute the whole library for an unavailable scoped cache.

Read-only inspection does not require loading a deck. A performance request permits source reads and necessary preparation, including completing missing playback analysis on the isolated queued target. It does not imply permanent cue edits, playlist replacement or replacing a confirmed grid. Distinguish source BPM/grid correction from playback tempo matching.

Keep cue IDs, bindings, provenance, edits. Re-read before a write; use cue `expected_revision` and a loaded target’s `expected_load_revision` for their distinct guards. Reconcile conflicts. Playlist/tag replacements replace arrays: preserve unmentioned content and verify exact saved objects. Playlist writes have no revision guard. XML can partially succeed; do not claim rollback.

## Return usable evidence

Return selected spans with exact effective event timestamps/beat positions, meter, grid provenance, neighboring features, provenance and uncertainty. Check truncation. Cue names and band energy cannot establish vocals, emotional mood or a verified first drop. Cue generation and automatic section labeling are unavailable. Full source durations and conditional played-set estimates are different quantities.

For writes, verify persistence and unchanged transport; for jobs, inspect terminal status rather than treating `ok:true` as successful completion. On an uncertain timeout, inspect the receipt and state before retrying. Keep saved knowledge, musical judgment and audible quality distinct.

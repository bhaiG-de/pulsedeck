# Analysis and persistent source knowledge

Owns the lifecycle of saved analysis and when to refresh it. [Track knowledge](track-knowledge.md) owns bounded evidence reads and cue/grid writes; [analysis semantics](../../references/musical-evidence.md) defines the fields. Musical interpretations belong to DJing — the art.

## Know what is saved

Keep canonical local/provider identity and the exact edit distinct from a title. Different remixes or relinked content can require different timing and cues. Stored metadata includes source timing/analysis and canonical cue knowledge. A prepared deck additionally needs audio available for playback; these are separate layers.

After restarting, reading or loading the same source should reuse compatible persisted analysis. A load can still take time to obtain provider audio, fingerprint content and prepare the renderer. A progress spinner or source preparation delay alone does not prove beat/BPM inference ran again. Inspect actual stage diagnostics, cache decision and stored stamps before diagnosing a cache miss.

The current implementation persists analysis in the library database. Its float round-trip serialization must preserve exact timing values across restarts. That runtime fix does not permit the agent to promise permanent provider audio storage. Do not delete profiles, rebuild databases or request collection-wide reanalysis to resolve an ordinary slow load.

## Read before refreshing

Use cached summaries to shortlist, then inspect selected passages. Inspect `available`, grid freshness, source identity, per-point origin/confirmation and stage errors. No saved cue is an ordinary unannotated source, not an analysis failure. An unavailable field stays unknown until a supported observation establishes it.

Passive inspection is read-only. A set request does not itself request permanent grid changes or analysis refresh. Do not create a cue solely to satisfy a phrase validator or overwrite a manual marker with a new automatic guess.

## Apply an authorized update

For requested analysis refresh or grid correction, use [track knowledge](track-knowledge.md#refresh-analysis-deliberately). Retain job identity, inspect completion/errors and reread the canonical result. Cancellation does not roll back records already processed.

There are no automatic cue-binding or section-vocabulary settings. Analysis covers tempo, beat/downbeat timing, key and waveform features. Manual/imported cues are independent source knowledge and survive reanalysis.

## Verify persistence at the right layer

Verify saved objects by canonical reads; compare source identity, expected timing, provenance and freshness. When a restart is already part of an authorized development test, compare before/after analysis stamps and decisions for the same source. Do not restart an app during a live set merely to test persistence. Renderer readiness and provider permission must still be checked through the control workflow.

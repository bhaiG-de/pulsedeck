# Manual-only cue cleanup — 15 October 2026

Automatic structural segmentation and cue generation have been removed, including section labels/vocabulary, automatic hot/memory filling, reconciliation/tombstone logic, structural stage caches, settings, UI controls, catalog/CLI operations and the generator-only integration test. The source-owned store now accepts manual/imported cues with stable IDs, optional human roles and main/hot/memory bindings.

BPM/rhythm, Beat This! beat/downbeat timing, key, waveform bands, manual grid edits, Sync, native scheduling, XML exchange and analysis persistence remain. Analysis/model versions were preserved so removal does not force neural reanalysis. The three skills and timing helper use measured data and optional human cues; absence of cues no longer blocks a coordinate check or implies failed analysis.

## Migration and real-library verification

A one-time transaction removes untouched generated points and their bindings, promotes explicitly edited/confirmed generated points to manual, cleans old analysis fields and preserves other source metadata. Legacy slot-only migration uses recorded automatic slot/time provenance, retaining annotated positions. A failed migration rolls back. Reopening a migrated library is idempotent.

The original library was backed up before application at `/Users/yashwinghosh/Library/Application Support/app.pulsedeck.desktop/backups/library-before-manual-cues-20261015T023707Z.sqlite3`. Isolated-copy and normal-app verification preserved all 11 local and 105 Spotify analysis records, including checked timing, key, waveform and fingerprint values. The migration removed 148 canonical points explicitly marked untouched generated, retained all 81 points marked manual/imported or edited/confirmed, and recovered four further retained points from older metadata.

Some older auto-filled slots had incorrectly been saved as manual. The user authorized clearing generic markers matching old generated boundaries while keeping named/manual edits. A separate revision-guarded cleanup removed 46 such markers across three Spotify sources: only role-free `Hot cue N` or `Memory cue` records matching the old generated positions within 2 ms and unchanged since the backup qualified. Imported cues, named/role edits and changed positions were excluded. No qualifying marker had changed since the backup.

Final verification found 39 canonical points remaining: the 35 original records outside the approved cleanup plus the four recovered records. Their IDs, names, roles and source times were preserved. All cue bindings resolve to retained points. Cached analysis still contains the same 11 local and 105 Spotify sources, with timing, key, waveform, fingerprint and manual-grid values unchanged; generated analysis fields are absent. The full backup remains available.
The rebuilt app starts normally with both decks paused. Read-only inspection still returns effective beat events and band bins; generated section output and the four retired catalog commands are absent. The CLI was rebuilt too. No music was restarted.

## Checks

- Strict workspace Clippy, all targets, warnings denied: pass.
- Audio analysis: 47 tests pass, two model/integration tests remain ignored.
- Deck engine: 162 tests pass, one model integration test remains ignored.
- App: 105 tests pass, one existing ignored test.
- Control protocol: 9 tests pass; CLI: 18 unit and 6 integration tests pass.
- Frontend: 68 tests pass; TypeScript/Vite and debug desktop bundle builds pass.
- Three-skill link/example validation, seven installation/relocation tests and eight timing-helper regressions pass.

The interrupted Luna trial is preserved in artifacts/skill-trials/luna-reference-skills-20261015T020213Z/outcome.md. It was stopped at the user’s request before completing 15 minutes. It demonstrated actual overlaps and independent recovery, but does not establish correct musical phrasing or a completed set. The separate same-event dependency deadline failure is recorded there and was not changed as part of cue removal.

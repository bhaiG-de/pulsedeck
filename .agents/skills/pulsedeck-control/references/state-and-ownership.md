# State and ownership

Use `performance_observe` for sources, controls, current generations, exact future event handles, beat/bar telemetry and an active session endpoint. Use `status` or `engine_transport` when readiness, meters, routing, cue selection or source jobs are needed. Describe only an unfamiliar tool/action rather than reloading every schema.

Protect contributing decks and user-prepared sources. A paused deck is not necessarily free. A direct mutation's `expected_load_revision` guards its source; a score's full generation additionally protects position/grid/control state. Preserve master level, crossfader and output routing. Do not globally mute or restart audio as a preparation shortcut.

Use one owner per affected control. Read `performance_receipts` before replacing accepted work. `performance_cancel` with `stabilize:false` releases pending score controls without restoring remembered values; its authored stabilization is appropriate only while that musical recovery is still intended. Human takeover yields authority. Cancellation does not erase valid loading, cues or earlier applied work.

Software meters establish software signal, not physical speaker/headphone sound. Cue selection does not prove discrete output routing. Model scores do not certify a usable pulse or phrase. See [analysis semantics](../../references/musical-evidence.md) and the [loaded-source readiness check](deck-preparation.md#loaded-source-readiness).

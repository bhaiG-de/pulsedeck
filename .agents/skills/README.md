# PulseDeck agent skills

Three skills keep musical judgment, source knowledge and software execution distinct:

- [DJing — the art](djing-art/SKILL.md) chooses records, passages, pulse/harmony, role transfers and the combined musical course.
- [PulseDeck library](pulsedeck-library/SKILL.md) obtains scoped repertoire and counted source evidence.
- [PulseDeck control](pulsedeck-control/SKILL.md) prepares sources and performs artist-authored native scores through MCP.

Live work uses the connected [PulseDeck MCP tools](references/mcp.md). A missing connection or required capability is reported directly, without falling back to another transport. Source descriptions and native receipts do not certify audible quality. The musical basis is preserved in the art skill's [sources](djing-art/references/sources.md), style guide and worked cases.

Entrypoints link only the references needed for a decision. Shared [analysis semantics](references/musical-evidence.md) and [named anchors](references/named-cues-and-suggestions.md) preserve source provenance. Offline scripts consume captured tool JSON and do not contact the app.

## Package validation

```sh
python3 tools/validate-dj-skills.py
python3 tools/test-dj-skill-packaging.py
python3 tools/test-dj-phrase-check.py
python3 tools/test-dj-offline-evidence.py
```

These check package closure, schemas/examples and deterministic captured-evidence behavior. Live MCP tests and musical trials are separate. The existing isolated developer integration harness is maintained outside the live skill package.

For a separately authorized installation into a fresh client directory, `tools/install-dj-skills.py` stages the three skills, rebases shared links and refuses existing/new legacy package collisions. It does not configure MCP, build the app, modify accounts or start playback. A fresh chat may be needed to refresh tool/skill discovery.

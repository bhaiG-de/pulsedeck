# Check a selected linear timeline offline

[phrase_check.py](../scripts/phrase_check.py) reads captured MCP tool JSON only. It checks chosen clock coordinates, effective meter/bar relationships and prepared transport strategy; it never calls PulseDeck or certifies musical phrasing/audibility. Its scope remains `clock_coordinates_only` with `audible_overlap:not_evaluated`.

Choose the musical relationship first using [phrasing and arrangement](phrasing-and-arrangement.md). Capture complete bounded `track_inspect` packets for both passages and a `status` packet after inspection. An optional status capture before inspection lets the helper reject intervening source/grid/seek changes. Retain actual source/revision identity and exact event timestamps.

For a same-meter linear blend, prepare the final incoming entry and an outgoing-master/incoming-Sync-follower strategy or matched uniform clocks. The checker inspects that prepared strategy; native `performance_validate` still owns executable feasibility and `performance_commit` owns execution.

Run the offline helper on those saved responses:

```sh
python3 "$DJ_ART_SKILL/scripts/phrase_check.py" \
  --state status-after.json --before-state status-before.json \
  --outgoing-packet outgoing-window.json --incoming-packet incoming-window.json \
  --outgoing A --incoming B --launch "$A_LAUNCH" --handover "$A_HANDOVER" \
  --entry "$B_ENTRY" --arrival "$B_ARRIVAL" --technique blend --save phrase-check.json
```

The files may contain direct structured data, an MCP tool result with `structuredContent`, or a captured JSON-RPC response. No app connection is made by this script. Reverse deck roles when appropriate. Four timestamps are selected exact source seconds, not rounded display labels.

Exit 0 means the selected coordinate relation passes, 1 means it failed, and 2 means unusable input/evidence. Read the exact interval counts and bar evidence: a 66-beat approach cannot replace 64 beats by rounding. Missing manual cues alone do not fail a coordinate check.

With both decks prepared and paused, `--preflight` allows the outgoing source to be paused and returns `execution_ready:false`. While the outgoing source plays, use the normal check. `--technique cut` means a true joint arrival with zero leads; it is not validation of an unequal-tempo fade.

Loops, unclocked pickups, beatless bridges, different meters and other nonlinear arrangements need their own counted construction and supported native score. Do not alter a source grid or remove a pickup to satisfy this helper. A timing pass says nothing about vocals, tonal fit, first audibility, role ownership, control curves, subsequent development or listening quality.

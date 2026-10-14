#!/bin/bash
# Export hand-checked beat grids from the PulseDeck library for beat_eval.
#
# A track counts as annotated after you open GRID on it, set or confirm the BPM
# and beat 1, and press "Apply grid" (both become manual). Put test tracks in
# playlists named House, Hip-hop, EDM or Electronic to group results by genre.
# Only local files are exported; Spotify audio cannot be decoded offline.
#
# Usage: bash tools/export_beat_annotations.sh > annotations.json
set -euo pipefail
db="${PULSEDECK_LIBRARY:-$HOME/Library/Application Support/app.pulsedeck.desktop/library.sqlite3}"
sqlite3 -readonly "$db" <<'SQL'
SELECT json_group_array(json_object(
  'path', t.path,
  'title', t.title,
  'bpm', t.bpm,
  'downbeat_seconds', t.beatgrid_offset_seconds,
  'genre', (SELECT lower(p.name) FROM playlist_tracks pt JOIN playlists p ON p.id = pt.playlist_id
            WHERE pt.track_id = t.id
              AND lower(p.name) IN ('house', 'hip-hop', 'hiphop', 'hip hop', 'edm', 'electronic')
            LIMIT 1)
))
FROM tracks t
WHERE json_extract(t.analysis_json, '$.beatgrid_source') = 'manual'
  AND json_extract(t.analysis_json, '$.bpm_source') = 'manual'
  AND t.bpm > 0;
SQL

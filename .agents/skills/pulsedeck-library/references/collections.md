# Discover the requested repertoire

Use `library_tracks` for indexed local/scoped queries. It returns `tracks`, `total` and `next_offset`; follow the returned offset until enough eligible candidates or the requested scope is covered. Apply textual search, BPM, playlist and observed classification filters together. Default results omit large waveform arrays. Missing-BPM tracks are excluded by BPM bounds, so a small unbounded query can be useful for an unanalysed candidate.

Example arguments for `library_tracks`:

```json
{"search":"house","bpm_min":120,"bpm_max":128,"sort":"bpm","limit":20}
```

Use observed tag IDs, ratings and color values rather than invented classifications. Read a selected record with `library_track` and canonical markers with `cues_get`. The query's sort is not a saved DJ sequence. `playlist` filters known membership; exact local playlist enumeration/order and writes are not exposed here. Preserve the requested order from supplied evidence or report the missing capability; do not infer sequence from sorted results.

## Spotify scope and preparation

Use `spotify_liked`, `spotify_playlists`, `spotify_playlist_tracks` or `spotify_album_tracks` with returned IDs and pagination. `spotify_playlist_tracks` requires `playlist_id`; offsets are provider pages, not musical positions. `mix_index_scope` with `{"scope":"spotify:liked"}` obtains missing indexed membership for that collection. Collection indexing is not whole-collection audio analysis.

A collection read does not prove native deck eligibility. Inspect `spotify_native_status` and an actual authorized isolated `deck_load`. Null account profiles and false placeholder availability flags can coexist with native `connected:true`/`playback_permission:true`. A real source refusal/cooldown remains authoritative; stop bulk retries and preserve the carrier. Do not start login from those placeholder fields.

## YouTube Music collections

After the user configures and signs in through Settings → YouTube Music, use `ytmusic_status`, `ytmusic_playlists` and `ytmusic_tracks` with returned IDs and `next_page_token`. Use `playlist_id:"liked"` for Liked videos. `ytmusic_match` takes at most 50 `video_ids` and caches local/Spotify matches; inspect partial errors and cancellation before continuing.

Load only a returned match's `source` through `deck_load`. A YouTube video ID or URL is not playable audio. Matches identify candidate recordings, not analysed or phrase-ready decks. Unmatched rows cannot play, and YouTube collections are not Suggest or automatic queue scopes.

## A shortlist for the next musical role

`library_suggest_next` can use actual deck tempo/ownership and scope:

```json
{"deck":"A","playlist":"spotify:liked","energy_direction":"keep","max_stretch_percent":3,"allow_half_double":false,"limit":20}
```

Directions are `keep`, `raise`, `lower`. Inspect scope completeness, exclusions, stretch, key confidence and grid freshness. Cached reads do not start analysis. `scope_not_cached` calls for obtaining that scope, not broadening to all songs. Do not clear played history to get more candidates.

`mix_openers` and `mix_options` add proposal evidence when useful. Their selected entries, techniques and continuation budgets need the DJ's source/phrase review. A long continuation allowance does not prove useful support through the overlap. This MCP connection exposes those proposal reads, not their automatic recipe/queue execution methods; express the chosen arrangement with the typed performance tools.

## Preserve musical and saved intent

Build a working sequence from the selected sources and role maps. The current connection cannot import audio, write tags/playlists or replace collection order; state the missing capability for that requested task. Do not silently mutate unrelated metadata through a different route. A playlist write, when separately available, must preserve unmentioned members and exact order, and array replacements need a fresh read; working notes are not saved playlists.

Estimate performance duration from actual entry/exit windows and rates:

```text
set_seconds = sum((exit_source_i - entry_source_i) / rate_i)
              - sum(overlap_seconds_i)
              + additional_loop_seconds
```

Count added loops once. Use piecewise or bounded estimates for rate changes; declare meter for bar counts. Do not subtract trimmed passages twice. Full source duration, conditional arranged duration and actual elapsed set time are different observations. If the requested duration cannot be covered, identify the shortfall and musical continuation choices.

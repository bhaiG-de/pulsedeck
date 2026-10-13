import { test } from "node:test";
import assert from "node:assert/strict";
import { spotifyTrackId, spotifyWidgetUrl } from "./spotify-links.ts";
const id = "4uLU6hMCjMI75M1A2tKUQC";
test("shared Spotify links and track URIs select the same fixed-origin widget", () => {
  assert.equal(
    spotifyTrackId(`https://open.spotify.com/track/${id}?si=public-share`),
    id,
  );
  assert.equal(spotifyTrackId(`spotify:track:${id}`), id);
  assert.equal(
    spotifyWidgetUrl(id),
    `https://open.spotify.com/embed/track/${id}`,
  );
});
test("untrusted origins, credentials and script URLs never become widget sources", () => {
  for (const value of [
    `https://open.spotify.com.evil.example/track/${id}`,
    `https://example.com/track/${id}`,
    `https://name:password@open.spotify.com/track/${id}`,
    `javascript:alert(1)`,
    `https://open.spotify.com/playlist/${id}`,
    `https://open.spotify.com/track/too-short`,
  ])
    assert.equal(spotifyTrackId(value), null);
  assert.equal(spotifyWidgetUrl(`${id}/../../evil`), null);
});

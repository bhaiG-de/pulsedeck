import { test } from "node:test";
import assert from "node:assert/strict";
import {
  dragPayload,
  emptyMessage,
  listCounts,
  referenceSummary,
  stepDirection,
  suggestionRequestKey,
  type Suggestion,
  type SuggestionList,
} from "./suggestions.ts";
import type { EngineSnapshot, TrackRecord } from "./types";

const track = (path: string): TrackRecord => ({ id: path, path, title: path });
function engine(): EngineSnapshot {
  return {
    crossfader: 0,
    decks: [
      {
        track: track("/a.mp3"),
        playing: true,
        channel_fader: 1,
        tempo_percent: 0,
        bpm: 124,
        position_seconds: 10,
      },
      { track: track("/b.mp3"), playing: false, channel_fader: 1, bpm: 122 },
    ],
  };
}
function list(change: Partial<SuggestionList> = {}): SuggestionList {
  return {
    deck: 0,
    target_deck: 1,
    direction: "keep",
    reference: {
      source: "/a.mp3",
      title: "A",
      bpm: 124,
      bpm_ambiguous: false,
      camelot: "8A",
      key_confidence: 0.7,
      energy: 0.6,
      energy_level: 6,
    },
    tempo_range: 6,
    suggestions: [],
    considered: 0,
    played: 0,
    out_of_range: 0,
    without_tempo: 0,
    pending: 0,
    elapsed_ms: 1.234,
    ...change,
  };
}

test("energy choice steps with the browse knob and stops at either end", () => {
  assert.equal(stepDirection("keep", 1), "raise");
  assert.equal(stepDirection("raise", 1), "lower");
  assert.equal(stepDirection("lower", 1), "lower");
  assert.equal(stepDirection("raise", -5), "keep");
  assert.equal(stepDirection("keep", Number.NaN), "keep");
});

test("suggestions refetch on track, audibility, tempo and energy changes only", () => {
  const base = engine();
  const key = suggestionRequestKey(base, "keep");
  const moved = engine();
  moved.decks[0].position_seconds = 42;
  moved.decks[0].channel_fader = 0.98;
  assert.equal(suggestionRequestKey(moved, "keep"), key);
  assert.notEqual(suggestionRequestKey(base, "raise"), key);
  const loaded = engine();
  loaded.decks[1].track = track("/c.mp3");
  assert.notEqual(suggestionRequestKey(loaded, "keep"), key);
  const faded = engine();
  faded.crossfader = 1;
  assert.notEqual(suggestionRequestKey(faded, "keep"), key);
  const started = engine();
  started.decks[1].playing = true;
  assert.notEqual(suggestionRequestKey(started, "keep"), key);
  const faster = engine();
  faster.decks[0].tempo_percent = 1.5;
  assert.notEqual(suggestionRequestKey(faster, "keep"), key);
  assert.doesNotThrow(() =>
    suggestionRequestKey({ decks: [{}, {}] } as EngineSnapshot, "keep"),
  );
});

test("summary names the followed deck, its tempo, key and energy", () => {
  assert.equal(
    referenceSummary(list()),
    "following A · 124.0 bpm · 8A · energy 6 → loads into B (±6%)",
  );
  assert.equal(referenceSummary(list({ deck: null, reference: null })), "");
});

test("empty lists explain why", () => {
  const filled = list({
    suggestions: [{ source: "/c.mp3" } as Suggestion],
  });
  assert.equal(emptyMessage(filled), null);
  assert.match(emptyMessage(null)!, /Load a track/);
  assert.match(emptyMessage(list({ pending: 12 }))!, /Preparing 12 tracks/);
  assert.match(
    emptyMessage(list({ out_of_range: 1 }))!,
    /deck B's ±6% tempo range; 1 track is outside/,
  );
  assert.match(emptyMessage(list({ played: 3 }))!, /played this session/);
  assert.match(emptyMessage(list())!, /has a BPM yet/);
});

test("counts report exclusions and ranking time", () => {
  assert.equal(
    listCounts(list({ considered: 40, out_of_range: 3, played: 2 })),
    "40 in reach · 3 out of tempo range · 2 played · 1.2 ms",
  );
});

test("dragging a suggestion uses the payloads the decks accept", () => {
  const local = { source: "/c.mp3", spotify: false } as Suggestion;
  assert.deepEqual(dragPayload(local), [
    "application/pulsedeck-track",
    "/c.mp3",
  ]);
  const spotify = {
    source: "spotify:track:0000000000000000000000",
    spotify: true,
    title: "Song",
    artist: "Artist",
  } as Suggestion;
  const [type, data] = dragPayload(spotify);
  assert.equal(type, "application/pulsedeck-spotify-track");
  assert.deepEqual(JSON.parse(data), {
    uri: spotify.source,
    title: "Song",
    artists: ["Artist"],
  });
});

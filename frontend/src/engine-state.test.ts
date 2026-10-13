import { test } from "node:test";
import assert from "node:assert/strict";
import { withTrackMetadata } from "./engine-state.ts";
import type { EngineSnapshot, TrackRecord } from "./types";
test("transport ticks retain waveform identity and clear explicitly unloaded metadata", () => {
  const waveform = [0.1, 0.2];
  const track: TrackRecord = {
    id: "a",
    path: "a",
    title: "a",
    waveform,
    waveform_bands: { low: waveform, mid: waveform, high: waveform },
  };
  const tick = {
    decks: [
      { position_seconds: 1, playing: true, track: null },
      { position_seconds: 0, track: null },
    ],
  } as EngineSnapshot;
  const first = withTrackMetadata(tick, [track, null]);
  const next = withTrackMetadata(
    {
      ...tick,
      decks: [{ ...tick.decks[0], position_seconds: 1.1 }, tick.decks[1]],
    },
    [track, null],
  );
  assert.equal(first.decks[0].track, next.decks[0].track);
  assert.equal(next.decks[0].track?.waveform, waveform);
  assert.equal(next.decks[0].position_seconds, 1.1);
  assert.equal(withTrackMetadata(tick, [null, null]).decks[0].track, null);
});

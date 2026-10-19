import { test } from "node:test";
import assert from "node:assert/strict";
import {
  deckBeatGrid,
  gridBeatPosition,
  gridBeatTime,
  visibleGridBeats,
  snapToGridBeat,
  gridBarBeat,
  gridStatus,
} from "./beat-grid.ts";
import type { DeckSnapshot, TrackRecord } from "./types.ts";
const beats = [0.12, 0.6, 1.14, 1.72, 2.35, 3.03, 3.78, 4.6];
const track: TrackRecord = {
  id: "variable",
  path: "/variable",
  title: "Variable timing",
  bpm: 120,
  neural_rhythm: {
    beats_seconds: beats,
    downbeats_seconds: [0.12, 1.72, 4.6],
    bpm: 120,
    beat_confidence: 0.9,
    downbeat_confidence: 0.8,
    model: "Beat This! final0",
    analysis_seconds: 1,
  },
};
const deck: DeckSnapshot = {
  track,
  bpm: 120,
  beatgrid_valid: true,
  beatgrid_downbeat: true,
  beatgrid_manual: false,
  beatgrid_anchor_index: 0,
  beatgrid_offset_seconds: 0.12,
};
test("markers and seeking retain actual varying intervals and predicted downbeats", () => {
  const grid = deckBeatGrid(deck);
  assert.equal(grid.beats, beats);
  visibleGridBeats(grid, 0, 5, 5).forEach((marker, index) =>
    assert.ok(Math.abs(marker.time - beats[index]) < 1e-12),
  );
  assert.deepEqual(
    visibleGridBeats(grid, 0, 5, 5)
      .filter((m) => m.downbeat)
      .map((m) => m.index),
    [0, 3, 7],
  );
  assert.equal(snapToGridBeat(grid, 2.1, 5), 2.35);
  assert.equal(snapToGridBeat(grid, 5, 5), 4.6);
  assert.deepEqual(visibleGridBeats(grid, 4.7, 8, 8), []); // no extrapolated markers
  assert.equal(gridBarBeat(grid, 1.14), "1.3");
  assert.equal(gridBarBeat(grid, 1.72), "2.1");
  assert.equal(gridBarBeat(grid, 3.78), "2.4");
  assert.equal(gridBarBeat(grid, 4.6), "3.1");
});
test("manual source anchor, offset and BPM scaling mirror native event clock", () => {
  const grid = deckBeatGrid({
    ...deck,
    bpm: 100,
    beatgrid_manual: true,
    beatgrid_anchor_index: 2,
    beatgrid_offset_seconds: 1.2,
  });
  assert.equal(gridBeatTime(grid, 0), 1.2);
  assert.ok(Math.abs(gridBeatTime(grid, 1)! - 1.896) < 1e-9);
  assert.deepEqual(
    visibleGridBeats(grid, 0, 5, 5)
      .filter((m) => m.downbeat)
      .map((m) => m.index),
    [2, 6],
  );
  assert.equal(gridBarBeat(grid, 1.2), "1.1");
  assert.equal(gridBarBeat(grid, 4.368), "2.1");
  assert.ok(Math.abs(snapToGridBeat(grid, -1, 5) - 0.552) < 1e-9);
  assert.ok(Math.abs(snapToGridBeat(grid, 5, 5) - 4.368) < 1e-9);
  for (let beat = -3; beat <= 9; beat += 0.25)
    assert.ok(
      Math.abs(gridBeatPosition(grid, gridBeatTime(grid, beat)!)! - beat) <
        1e-9,
    );
});
test("invalid grids abstain from markers, snapping and bar labels", () => {
  const grid = deckBeatGrid({ ...deck, beatgrid_valid: false });
  assert.deepEqual(visibleGridBeats(grid, 0, 5, 5), []);
  assert.equal(snapToGridBeat(grid, 2.1, 5), 2.1);
  assert.equal(gridBarBeat(grid, 2), null);
});
test("model, manual and failed fallback provenance are distinguished", () => {
  assert.match(gridStatus(deck), /Model beats and downbeats/);
  assert.match(
    gridStatus({ ...deck, beatgrid_manual: true }),
    /Manual bar 1 · model timing retained/,
  );
  const failed = {
    ...track,
    neural_rhythm: null,
    rhythm_error: "model file missing",
  };
  assert.match(
    gridStatus({ ...deck, track: failed }),
    /failed · estimated beat fallback/,
  );
  assert.match(
    gridStatus({ ...deck, track: failed, beatgrid_valid: false }),
    /failed · grid unknown/,
  );
  assert.equal(
    gridBarBeat(
      deckBeatGrid({
        ...deck,
        track: {
          ...track,
          neural_rhythm: { ...track.neural_rhythm!, downbeats_seconds: [] },
        },
      }),
      1,
    ),
    null,
  );
});
test("manual tempo overrides use a complete uniform grid without retiming model events", () => {
  const overridden = {
    ...deck,
    bpm: 240,
    beatgrid_uniform: true,
    beatgrid_manual: true,
  };
  const grid = deckBeatGrid(overridden);
  assert.equal(grid.beats, null);
  assert.ok(Math.abs((gridBeatTime(grid, 80) ?? 0) - 20.12) < 1e-8);
  const markers = visibleGridBeats(grid, 20, 21, 32);
  assert.equal(markers.length, 4);
  assert.ok(markers.some((marker) => marker.downbeat));
  assert.equal(track.neural_rhythm?.beats_seconds, beats);
  assert.equal(gridStatus(overridden), "Manual bar 1 · uniform timing");
});

test("manual bar realignment retains a five-beat meter", () => {
  const odd = {...deck, beatgrid_manual:true, track:{...deck.track!, neural_rhythm:{...deck.track!.neural_rhythm!,beats_per_bar:5}}};
  const grid=deckBeatGrid(odd);
  assert.equal(grid.meter,5);
  assert.equal(gridBarBeat(grid,grid.offset),"1.1");
});

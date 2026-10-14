import { test } from "node:test";
import assert from "node:assert/strict";
import {
  beatPosition,
  beatPhaseDelta,
  shiftBeatOne,
  snapToBeat,
} from "./mixing-view.ts";
test("beat 1 shifts by whole beats and stays within one bar", () => {
  const close = (a: number, b: number) =>
    assert.ok(Math.abs(a - b) < 1e-9, `${a} vs ${b}`);
  close(shiftBeatOne(0.11, 120, 1), 0.61);
  close(shiftBeatOne(0.11, 120, -1), 1.61);
  close(shiftBeatOne(1.61, 120, 1), 0.11);
  close(shiftBeatOne(5.0, 120, 0), 1.0);
  assert.equal(shiftBeatOne(0.3, 0, 1), 0.3);
});
test("aligned source-time grids compare correctly across different BPMs and pickup offsets", () => {
  assert.equal(beatPosition(1.125, 120, 0.125), 2);
  assert.ok(Math.abs(beatPosition(1.8, 100, 0.6)! - 2) < 1e-9);
  assert.ok(
    Math.abs(
      beatPhaseDelta(
        beatPosition(1.125, 120, 0.125),
        beatPosition(1.8, 100, 0.6),
      )!,
    ) < 1e-9,
  );
  assert.ok(Math.abs(beatPhaseDelta(3.95, 4.05)! - 0.1) < 1e-9);
  assert.equal(beatPosition(3, 0), null);
});
test("waveform seek quantization uses the real grid offset and file bounds", () => {
  assert.equal(snapToBeat(0.38, 120, 0.125, 20), 0.625);
  assert.equal(snapToBeat(-5, 120, 0.125, 20), 0.125);
  assert.equal(snapToBeat(22, 120, 0.125, 20), 20);
  assert.equal(snapToBeat(2.38, 0, 0, 20), 2.38);
});

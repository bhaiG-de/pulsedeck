import { test } from "node:test";
import assert from "node:assert/strict";
import {
  hasWaveformBands,
  rebinEnvelope,
  smoothEnvelope,
  getWaveformWindow,
  timeToWaveformFraction,
  waveformSeekSeconds,
  beatSpacingFraction,
  waveformBarBeat,
  displaySections,
  nextSection,
} from "./waveform-data.ts";
test("sections merge repeated labels and count bars to the next one", () => {
  const stage = {
    version: 1,
    bpm: 120,
    downbeat_offset_seconds: 0.5,
    vocabulary: "dance" as const,
    sections: [
      { start_seconds: 0.5, bars: 16, label: "intro" as const },
      { start_seconds: 32.5, bars: 8, label: "drop" as const },
      { start_seconds: 48.5, bars: 8, label: "drop" as const },
      { start_seconds: 64.5, bars: 4, label: null },
      { start_seconds: 72.5, bars: 8, label: null },
      { start_seconds: 88.5, bars: 8, label: "outro" as const },
    ].map((s) => ({ ...s, energy: 0.5, groove: 0.5, rise: 0, group: 0 })),
  };
  const shown = displaySections(stage, 100);
  assert.deepEqual(
    shown.map((s) => [s.start, s.end, s.bars, s.label]),
    [
      [0.5, 32.5, 16, "intro"],
      [32.5, 64.5, 16, "drop"],
      [64.5, 72.5, 4, null],
      [72.5, 88.5, 8, null],
      [88.5, 100, 8, "outro"],
    ],
  );
  // Two seconds per bar at 120 BPM: 31 s into the intro, the drop is a bar away.
  assert.deepEqual(nextSection(shown, 31, 120), { label: "drop", bars: 1 });
  assert.deepEqual(nextSection(shown, 0.5, 120), { label: "drop", bars: 16 });
  assert.equal(nextSection(shown, 95, 120), null);
  assert.deepEqual(displaySections(null, 100), []);
});
test("compressed overview retains a narrow transient instead of sampling past it", () => {
  const peaks = Array(4096).fill(0);
  peaks[2047] = 0.9;
  const overview = rebinEnvelope(peaks, 0, 1, 128, "peak");
  assert.ok(Math.abs(Math.max(...overview) - 0.9) < 1e-6);
  assert.ok(overview.some((value) => value > 0));
});
test("band rebinning preserves shared RMS amplitude rather than boosting quieter bands", () => {
  const low = rebinEnvelope(Array(4096).fill(0.4), 0, 1, 200, "rms");
  const high = rebinEnvelope(Array(4096).fill(0.04), 0, 1, 200, "rms");
  assert.ok(Math.abs(low[100] / high[100] - 10) < 1e-5);
  assert.ok(Math.abs(smoothEnvelope(high)[100] - 0.04) < 1e-6);
});
test("zoomed envelopes interpolate existing points and silence stays silent", () => {
  const zoomed = rebinEnvelope([0, 1], 0, 1, 8);
  assert.equal(zoomed.length, 8);
  assert.ok(zoomed.some((value) => value > 0 && value < 1));
  assert.ok(rebinEnvelope([0, 0, 0], 0, 1, 100).every((value) => value === 0));
  assert.ok(
    rebinEnvelope([NaN, Infinity, -Infinity], 0, 1, 3).every(
      (value) => value === 0,
    ),
  );
});
test("partial legacy band metadata cannot be presented as three-band evidence", () => {
  assert.equal(
    hasWaveformBands({ low: [1, 1], mid: [1], high: [1, 1] }, 2),
    false,
  );
  assert.equal(hasWaveformBands(null, 512), false);
  assert.equal(
    hasWaveformBands({ low: [0, 0], mid: [0, 0], high: [0, 0] }, 2),
    true,
  );
});

test("detail playhead remains centered at both file boundaries with a full virtual span", () => {
  const begin = getWaveformWindow({ position: 0, duration: 10 });
  const end = getWaveformWindow({ position: 10, duration: 10 });
  assert.equal(begin.start, -20);
  assert.equal(begin.end, 20);
  assert.equal(begin.span, 40);
  assert.equal(end.start, -10);
  assert.equal(end.end, 30);
  assert.equal(end.span, 40);
  assert.equal(timeToWaveformFraction(0, begin), 0.5);
  assert.equal(timeToWaveformFraction(10, end), 0.5);
  assert.equal(begin.playheadFraction, 0.5);
  assert.equal(end.playheadFraction, 0.5);
});
test("overview keeps the whole-file timebase and a moving playhead", () => {
  for (const position of [0, 2.5, 10]) {
    const view = getWaveformWindow({
      position,
      duration: 10,
      overview: true,
      spanBeats: 32,
      bpm: 120,
    });
    assert.equal(view.start, 0);
    assert.equal(view.span, 10);
    assert.equal(view.playheadFraction, position / 10);
  }
});
test("virtual padding stays blank without stretching or repeating edge samples", () => {
  const peaks = [0.1, 0.2, 0.3, 0.4];
  const padded = rebinEnvelope(peaks, -0.5, 1.5, 8);
  const expected = [0, 0, 0.1, 0.2, 0.3, 0.4, 0, 0];
  padded.forEach((value, i) => assert.ok(Math.abs(value - expected[i]) < 1e-6));
  for (const mode of ["peak", "rms"] as const) {
    assert.ok(
      rebinEnvelope(peaks, -1, 0, 20, mode).every((value) => value === 0),
    );
    assert.ok(
      rebinEnvelope(peaks, 1, 2, 20, mode).every((value) => value === 0),
    );
    const zoomed = rebinEnvelope(peaks, -0.5, 1.5, 32, mode);
    assert.ok(zoomed.slice(0, 8).every((value) => value === 0));
    assert.ok(zoomed.slice(24).every((value) => value === 0));
  }
});
test("a partially padded RMS pixel includes silent padding in its shared scale", () => {
  const partial = rebinEnvelope([0.5], -0.5, 0.5, 1, "rms");
  assert.ok(Math.abs(partial[0] - 0.5 / Math.sqrt(2)) < 1e-6);
});
test("seeking blank detail margins clamps to real file bounds and center keeps current time", () => {
  const view = getWaveformWindow({ position: 2, duration: 10 });
  assert.equal(waveformSeekSeconds(view, 0, 10), 0);
  assert.equal(waveformSeekSeconds(view, 0.5, 10), 2);
  assert.equal(waveformSeekSeconds(view, 1, 10), 10);
  const overview = getWaveformWindow({
    position: 2,
    duration: 10,
    overview: true,
  });
  assert.equal(waveformSeekSeconds(overview, 0.75, 10), 7.5);
});
test("linked beat spans give identical beat spacing for decks with different source BPMs", () => {
  const a = getWaveformWindow({
    position: 40,
    duration: 180,
    bpm: 120,
    spanBeats: 32,
  });
  const b = getWaveformWindow({
    position: 60,
    duration: 200,
    bpm: 96,
    spanBeats: 32,
  });
  assert.equal(a.span, 16);
  assert.equal(b.span, 20);
  assert.equal(beatSpacingFraction(a, 120), 1 / 32);
  assert.equal(beatSpacingFraction(b, 96), 1 / 32);
  assert.equal(
    timeToWaveformFraction(40 + 60 / 120, a),
    timeToWaveformFraction(60 + 60 / 96, b),
  );
});
test("unknown tempo does not invent a 120 BPM grid or bar label", () => {
  const unknown = getWaveformWindow({
    position: 20,
    duration: 180,
    bpm: 0,
    spanBeats: 32,
  });
  assert.equal(unknown.span, 40);
  assert.equal(beatSpacingFraction(unknown, 0), null);
  assert.equal(waveformBarBeat(20, 0), null);
  assert.equal(waveformBarBeat(8.25, 120, 0.25), "5.1");
});

test("pickup before the first grid beat retains a negative bar label at the centered head", () => {
  assert.equal(waveformBarBeat(0, 120, 0.125), "-1.4");
  assert.equal(waveformBarBeat(0.125, 120, 0.125), "1.1");
});


test("section display uses native bar endpoints for the final variable-tempo section", () => {
  const stage = { version: 2, bpm: 120, downbeat_offset_seconds: 0.2,
    beats_per_bar: 3, bar_times: [0.2, 1.7, 3.4, 5.3, 7.4],
    vocabulary: "dance" as const,
    sections: [{start_seconds: 0.2, bars: 4, label: "intro" as const, energy: 0.5, groove: 0.5, rise: 0, group: 0}],
  };
  assert.equal(displaySections(stage, 10)[0].end, 7.4);
});

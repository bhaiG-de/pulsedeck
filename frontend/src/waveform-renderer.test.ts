import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EnvelopeIndex,
  envelopeIndex,
  WaveformRaster,
  type RasterSource,
  type CanvasFactory,
} from "./waveform-renderer.ts";
import { rebinEnvelope } from "./waveform-data.ts";
function fakeCanvasFactory() {
  const factory: CanvasFactory = (width, height) =>
    ({
      width,
      height,
      getContext: () => ({
        createImageData: (w: number, h: number) => ({
          data: new Uint8ClampedArray(w * h * 4),
        }),
        putImageData: () => {},
      }),
    }) as unknown as HTMLCanvasElement;
  return factory;
}
const ctx = {
  save() {},
  restore() {},
  beginPath() {},
  rect() {},
  clip() {},
  drawImage() {},
} as unknown as CanvasRenderingContext2D;
const samples = Array.from(
  { length: 4096 },
  (_, i) => (Math.sin(i * 0.17) + 1) / 2,
);
const source = (changes: Partial<RasterSource> = {}): RasterSource => ({
  samples,
  duration: 377,
  height: 86,
  dpr: 2,
  pixelsPerSecond: 100,
  color: "#ffffff",
  overview: false,
  ...changes,
});
test("multiresolution maximum/RMS queries match legacy envelopes across zoom and silent margins", () => {
  const index = new EnvelopeIndex(samples);
  for (const [a, b, columns] of [
    [0, 1, 128],
    [-0.2, 0.3, 300],
    [0.9, 1.2, 20],
    [0.32, 0.34, 500],
  ]) {
    for (const rms of [false, true]) {
      const legacy = rebinEnvelope(
        samples,
        a,
        b,
        columns,
        rms ? "rms" : "peak",
      );
      for (let x = 0; x < columns; x++) {
        const step = (b * samples.length - a * samples.length) / columns,
          from = a * samples.length + x * step,
          to = from + step;
        assert.ok(Math.abs(index.sample(from, to, rms) - legacy[x]) < 2e-6);
      }
    }
  }
});
test("cached envelopes retain isolated transients, shared band RMS levels and zero padding", () => {
  const data = Array(4096).fill(0);
  data[2047] = 0.9;
  const i = new EnvelopeIndex(data);
  assert.ok(Math.abs(i.sample(0, 4096, false) - 0.9) < 1e-6);
  assert.equal(i.sample(-10, -1, false), 0);
  assert.equal(i.sample(4096, 5000, true), 0);
  const flat = new EnvelopeIndex([0.5, 0.5]);
  assert.ok(Math.abs(flat.sample(-2, 2, true) - Math.sqrt(0.125)) < 1e-6);
  assert.equal(envelopeIndex(data), envelopeIndex(data));
  assert.notEqual(envelopeIndex([...data]), envelopeIndex(data));
});
test("normal pan and overview repaint composite cached pixels without rebuilding source data", () => {
  const raster = new WaveformRaster(fakeCanvasFactory());
  raster.configure(source());
  raster.draw(ctx, 10, 1400);
  const initial = raster.tileBuilds;
  for (let frame = 0; frame < 120; frame++) {
    raster.configure(source());
    raster.draw(ctx, 10 + frame * 0.001, 1400);
  }
  assert.equal(raster.tileBuilds, initial);
  const overview = new WaveformRaster(fakeCanvasFactory());
  overview.configure(
    source({ pixelsPerSecond: 500 / 377, overview: true, height: 28 }),
  );
  for (let frame = 0; frame < 120; frame++) overview.draw(ctx, 0, 500);
  assert.equal(overview.tileBuilds, 1);
});
test("prewarming builds the neighboring tile before a boundary frame and the cache stays bounded", () => {
  const raster = new WaveformRaster(fakeCanvasFactory());
  raster.configure(source());
  raster.draw(ctx, 0, 1400);
  assert.equal(raster.tileBuilds, 2);
  raster.prewarm(0, 1400);
  assert.equal(raster.tileBuilds, 3);
  raster.draw(ctx, 8, 1400);
  assert.equal(raster.tileBuilds, 3);
  for (let t = 30; t < 300; t += 25) raster.draw(ctx, t, 1400);
  // Returning to the start after many tiles must rebuild evicted entries, rather than retaining a whole track raster.
  const before = raster.tileBuilds;
  raster.draw(ctx, 0, 1400);
  assert.equal(raster.tileBuilds, before + 2);
});
test("source/zoom changes invalidate raster tiles; out-of-track windows remain blank", () => {
  const raster = new WaveformRaster(fakeCanvasFactory());
  raster.configure(source());
  raster.draw(ctx, -30, 1400);
  assert.equal(raster.tileBuilds, 0);
  raster.draw(ctx, 400, 1400);
  assert.equal(raster.tileBuilds, 0);
  raster.draw(ctx, 0, 500);
  assert.equal(raster.tileBuilds, 1);
  raster.configure(source({ pixelsPerSecond: 200 }));
  raster.draw(ctx, 0, 500);
  assert.equal(raster.tileBuilds, 2);
  raster.configure(source({ samples: [...samples], pixelsPerSecond: 200 }));
  raster.draw(ctx, 0, 500);
  assert.equal(raster.tileBuilds, 3);
});

test("three-band bars fill the view using shared gain, leave comb gaps and never paint a ghost peak layer", () => {
  let pixels: Uint8ClampedArray | undefined;
  const factory: CanvasFactory = (width, height) =>
    ({
      width,
      height,
      getContext: () => ({
        createImageData: (w: number, h: number) => ({
          data: new Uint8ClampedArray(w * h * 4),
        }),
        putImageData: (image: { data: Uint8ClampedArray }) => {
          pixels = image.data;
        },
      }),
    }) as unknown as HTMLCanvasElement;
  const bands = {
    low: Array(16).fill(0.2),
    mid: Array(16).fill(0.1),
    high: Array(16).fill(0.05),
  };
  const raster = new WaveformRaster(factory);
  raster.configure(
    source({
      samples: Array(16).fill(1),
      bands,
      height: 100,
      dpr: 1,
      duration: 10,
    }),
  );
  raster.draw(ctx, 0, 100);
  const colors = new Set<string>();
  let filled = 0;
  for (let y = 0; y < 100; y++) {
    const offset = (y * 1024 + 10) * 4;
    if (pixels![offset + 3]) {
      filled++;
      colors.add(Array.from(pixels!.slice(offset, offset + 3)).join(","));
      assert.equal(pixels![offset + 3], 255);
    }
  }
  assert.ok(
    filled >= 86 && filled <= 92,
    `expected a large colored envelope, got ${filled} pixels`,
  );
  assert.deepEqual(
    colors,
    new Set(["63,125,255", "53,208,127", "244,244,242"]),
  );
  // The envelope is a comb: the column after each bar is an empty gap.
  for (let y = 0; y < 100; y++)
    assert.equal(pixels![(y * 1024 + 11) * 4 + 3], 0);
  raster.configure(
    source({
      samples: Array(16).fill(1),
      bands: {
        low: Array(16).fill(0),
        mid: Array(16).fill(0),
        high: Array(16).fill(0),
      },
      height: 100,
      dpr: 1,
      duration: 10,
    }),
  );
  raster.draw(ctx, 0, 100);
  assert.ok(
    pixels!.every((value) => value === 0),
    "silent band data cannot grow a gray outline from a separate peak array",
  );
});

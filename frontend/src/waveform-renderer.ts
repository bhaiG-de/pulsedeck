import type { WaveformBands } from "./types";
import { BAND_COLORS, hasWaveformBands } from "./waveform-data.ts";

const value = (n: number | undefined) =>
  Number.isFinite(n) ? Math.max(0, Math.min(1, Math.abs(n!))) : 0;
/** Immutable multiresolution index; maximum queries retain transients, powers preserve RMS. */
export class EnvelopeIndex {
  readonly values: Float32Array;
  readonly levels: {
    minimum: Float32Array;
    maximum: Float32Array;
    power: Float64Array;
  }[] = [];
  private prefix: Float64Array;
  constructor(samples: readonly number[]) {
    this.values = Float32Array.from(samples, value);
    this.prefix = new Float64Array(samples.length + 1);
    for (let i = 0; i < samples.length; i++)
      this.prefix[i + 1] = this.prefix[i] + this.values[i] * this.values[i];
    let minimum = this.values,
      maximum = this.values,
      power = Float64Array.from(this.values, (n) => n * n);
    this.levels.push({ minimum, maximum, power });
    while (maximum.length > 1) {
      const n = Math.ceil(maximum.length / 2),
        lo = new Float32Array(n),
        hi = new Float32Array(n),
        p = new Float64Array(n);
      for (let i = 0; i < n; i++) {
        const a = i * 2,
          b = Math.min(a + 1, maximum.length - 1);
        lo[i] = Math.min(minimum[a], minimum[b]);
        hi[i] = Math.max(maximum[a], maximum[b]);
        p[i] = power[a] + (b === a ? 0 : power[b]);
      }
      minimum = lo;
      maximum = hi;
      power = p;
      this.levels.push({ minimum, maximum, power });
    }
  }
  private integral(at: number) {
    const clamped = Math.max(0, Math.min(this.values.length, at)),
      i = Math.floor(clamped);
    return this.prefix[i] + (clamped - i) * (this.values[i] || 0) ** 2;
  }
  sample(from: number, to: number, rms: boolean): number {
    const n = this.values.length,
      step = to - from;
    if (!n || step <= 0 || to <= 0 || from >= n) return 0;
    if (step < 1) {
      const mid = (from + to) / 2;
      if (mid < 0 || mid >= n) return 0;
      const center = Math.max(0, Math.min(n - 1, mid - 0.5)),
        a = Math.floor(center),
        f = center - a;
      return this.values[a] * (1 - f) + this.values[Math.min(a + 1, n - 1)] * f;
    }
    if (rms)
      return Math.sqrt(
        Math.max(0, this.integral(to) - this.integral(from)) / step,
      );
    let a = Math.max(0, Math.floor(from)),
      b = Math.min(n, Math.ceil(to)),
      max = 0;
    while (a < b) {
      let level = 0;
      while (
        level + 1 < this.levels.length &&
        a % 2 ** (level + 1) === 0 &&
        a + 2 ** (level + 1) <= b
      )
        level++;
      max = Math.max(max, this.levels[level].maximum[a >> level]);
      a += 2 ** level;
    }
    return max;
  }
}
const indexes = new WeakMap<readonly number[], EnvelopeIndex>();
export function envelopeIndex(samples: readonly number[]): EnvelopeIndex {
  let index = indexes.get(samples);
  if (!index) {
    index = new EnvelopeIndex(samples);
    indexes.set(samples, index);
  }
  return index;
}
export interface RasterSource {
  samples: readonly number[];
  bands?: WaveformBands | null;
  duration: number;
  height: number;
  dpr: number;
  pixelsPerSecond: number;
  color: string;
  overview: boolean;
}
export type CanvasFactory = (
  width: number,
  height: number,
) => HTMLCanvasElement;
const defaultFactory: CanvasFactory = (width, height) => {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
};
const rgb = (hex: string) =>
  [
    parseInt(hex.slice(1, 3), 16),
    parseInt(hex.slice(3, 5), 16),
    parseInt(hex.slice(5, 7), 16),
  ] as const;
const lowColor = rgb(BAND_COLORS.low),
  midColor = rgb(BAND_COLORS.mid),
  highColor = rgb(BAND_COLORS.high);

/** Track-aligned raster tiles. Panning only composites existing pixels at fractional x.
 *  The envelope is drawn as a comb of flat-topped bars, like a field recorder's
 *  screen. Bar slots are anchored to track time (not to tiles), so the comb
 *  scrolls without shimmer and stays continuous across tile boundaries. */
export class WaveformRaster {
  private config: RasterSource | null = null;
  private bandGain = 1;
  private tiles = new Map<number, HTMLCanvasElement | ImageBitmap>();
  private pending = new Set<number>();
  private generation = 0;
  private worker: Worker | null = null;
  private asynchronous: boolean;
  private disposed = false;
  private ready: () => void;
  readonly tileWidth = 1024;
  tileBuilds = 0;
  private createCanvas: CanvasFactory;
  constructor(
    createCanvas: CanvasFactory = defaultFactory,
    ready: () => void = () => {},
  ) {
    this.createCanvas = createCanvas;
    this.ready = ready;
    this.asynchronous =
      createCanvas === defaultFactory && typeof window !== "undefined";
    if (
      this.asynchronous &&
      typeof Worker !== "undefined" &&
      typeof OffscreenCanvas !== "undefined"
    ) {
      try {
        this.worker = new Worker(
          new URL("./waveform-worker.ts", import.meta.url),
          { type: "module" },
        );
        this.worker.onmessage = ({ data }) => {
          const { generation, number, bitmap } = data as {
            generation: number;
            number: number;
            bitmap: ImageBitmap;
          };
          if (generation !== this.generation || this.disposed) {
            bitmap.close();
            return;
          }
          this.pending.delete(number);
          this.remember(number, bitmap);
          this.tileBuilds++;
          this.ready();
        };
        this.worker.onerror = () => {
          this.worker?.terminate();
          this.worker = null;
          const config = this.config;
          this.config = null;
          this.pending.clear();
          if (config) this.configure(config);
          this.ready();
        };
      } catch {
        this.worker = null;
      }
    }
  }
  private release(tile: HTMLCanvasElement | ImageBitmap) {
    if ("close" in tile) tile.close();
  }
  private remember(number: number, tile: HTMLCanvasElement | ImageBitmap) {
    const old = this.tiles.get(number);
    if (old) this.release(old);
    this.tiles.set(number, tile);
    while (this.tiles.size > 6) {
      const key = this.tiles.keys().next().value!;
      this.release(this.tiles.get(key)!);
      this.tiles.delete(key);
    }
  }
  dispose() {
    this.disposed = true;
    this.generation++;
    this.worker?.terminate();
    this.worker = null;
    this.tiles.forEach((tile) => this.release(tile));
    this.tiles.clear();
    this.pending.clear();
  }
  configure(next: RasterSource): void {
    const p = this.config;
    if (
      p &&
      p.samples === next.samples &&
      p.bands?.low === next.bands?.low &&
      p.bands?.mid === next.bands?.mid &&
      p.bands?.high === next.bands?.high &&
      p.duration === next.duration &&
      p.height === next.height &&
      p.dpr === next.dpr &&
      p.pixelsPerSecond === next.pixelsPerSecond &&
      p.color === next.color &&
      p.overview === next.overview
    )
      return;
    this.config = next;
    this.generation++;
    this.pending.clear();
    this.tiles.forEach((tile) => this.release(tile));
    this.tiles.clear();
    if (this.worker) {
      this.worker.postMessage({
        type: "configure",
        generation: this.generation,
        config: next,
      });
      return;
    }
    // Unsupported WebViews use idle construction below, never tile building inside paint.
    // One shared display gain preserves relative band energy. Normalize once per
    // immutable source, so the colored envelope fills the view without pumping.
    if (hasWaveformBands(next.bands, next.samples.length)) {
      const maximum = Math.max(
        ...[next.bands!.low, next.bands!.mid, next.bands!.high].map(
          (values) => {
            const index = envelopeIndex(values);
            return index.levels[index.levels.length - 1]?.maximum[0] || 0;
          },
        ),
      );
      this.bandGain = maximum > 0 ? Math.min(12, 0.96 / maximum) : 1;
    } else this.bandGain = 1;
    this.tiles.clear();
  }
  private tile(number: number): HTMLCanvasElement | ImageBitmap | undefined {
    const hit = this.tiles.get(number);
    if (hit) {
      this.tiles.delete(number);
      this.tiles.set(number, hit);
      return hit;
    }
    if (this.asynchronous) {
      if (!this.pending.has(number) && this.pending.size < 6) {
        this.pending.add(number);
        const generation = this.generation;
        if (this.worker)
          this.worker.postMessage({ type: "tile", generation, number });
        else {
          const build = () => {
            if (this.disposed || generation !== this.generation) return;
            this.buildTile(number);
            this.pending.delete(number);
            this.ready();
          };
          if ("requestIdleCallback" in window)
            window.requestIdleCallback(build, { timeout: 150 });
          else setTimeout(build, 0);
        }
      }
      return undefined;
    }
    return this.buildTile(number);
  }
  private buildTile(number: number): HTMLCanvasElement {
    const c = this.config!;
    const width = Math.ceil(this.tileWidth * c.dpr),
      height = Math.max(1, Math.ceil(c.height * c.dpr)),
      canvas = this.createCanvas(width, height),
      ctx = canvas.getContext("2d")!;
    const image = ctx.createImageData(width, height),
      bytes = image.data,
      center = height / 2,
      scale = height * 0.46,
      n = c.samples.length;
    const spectral = hasWaveformBands(c.bands, n),
      peak = spectral ? null : envelopeIndex(c.samples),
      lo = spectral ? envelopeIndex(c.bands!.low) : null,
      mi = spectral ? envelopeIndex(c.bands!.mid) : null,
      hi = spectral ? envelopeIndex(c.bands!.high) : null,
      plain = rgb(c.color);
    // Bar slot in device pixels: a bar plus a gap, measured from track start.
    const period = Math.max(2, Math.round((c.overview ? 2 : 3) * c.dpr)),
      barWidth = Math.max(1, Math.round(period * (c.overview ? 0.5 : 0.67))),
      trackStep = n / (c.duration * c.pixelsPerSecond * c.dpr),
      origin = number * width;
    let bar = -1,
      l = 0,
      m = 0,
      h = 0,
      p = 0;
    for (let x = 0; x < width; x++) {
      const gx = origin + x;
      if (gx % period >= barWidth) continue;
      const slot = Math.floor(gx / period);
      if (slot !== bar) {
        bar = slot;
        // Each bar carries the energy of its whole slot, so its top is flat.
        const from = slot * period * trackStep,
          to = from + period * trackStep;
        if (spectral) {
          l = lo!.sample(from, to, true) * scale * this.bandGain;
          m = mi!.sample(from, to, true) * scale * this.bandGain;
          h = hi!.sample(from, to, true) * scale * this.bandGain;
        } else p = peak!.sample(from, to, false) * scale;
      }
      const reach = spectral ? Math.max(l, m, h) : p;
      for (
        let y = Math.max(0, Math.floor(center - reach));
        y < Math.min(height, Math.ceil(center + reach));
        y++
      ) {
        const dist = Math.abs(y + 0.5 - center);
        let color: readonly number[] = plain,
          alpha = spectral ? 1 : c.overview ? 0.7 : 0.9,
          smallest = Infinity;
        if (spectral) {
          // The innermost band containing this pixel shows on top.
          if (dist <= l) {
            smallest = l;
            color = lowColor;
          }
          if (dist <= m && m < smallest) {
            smallest = m;
            color = midColor;
          }
          if (dist <= h && h < smallest) color = highColor;
          if (smallest === Infinity && !(dist <= h)) continue;
        }
        const offset = (y * width + x) * 4;
        bytes[offset] = color[0];
        bytes[offset + 1] = color[1];
        bytes[offset + 2] = color[2];
        bytes[offset + 3] = Math.round(alpha * 255);
      }
    }
    ctx.putImageData(image, 0, 0);
    this.remember(number, canvas);
    this.tileBuilds++;
    return canvas;
  }
  prewarm(startSeconds: number, width: number, direction = 1): void {
    const c = this.config;
    if (!c || c.duration <= 0) return;
    const begin = startSeconds * c.pixelsPerSecond;
    const number =
      direction < 0
        ? Math.floor(begin / this.tileWidth) - 1
        : Math.floor((begin + width) / this.tileWidth) + 1;
    if (
      number >= 0 &&
      number < Math.ceil((c.duration * c.pixelsPerSecond) / this.tileWidth)
    )
      this.tile(number);
  }
  draw(
    ctx: CanvasRenderingContext2D,
    startSeconds: number,
    width: number,
  ): void {
    const c = this.config;
    if (!c || !c.samples.length || c.duration <= 0 || c.pixelsPerSecond <= 0)
      return;
    const begin = startSeconds * c.pixelsPerSecond,
      first = Math.max(0, Math.floor(begin / this.tileWidth)),
      last = Math.min(
        Math.ceil((c.duration * c.pixelsPerSecond) / this.tileWidth) - 1,
        Math.floor((begin + width) / this.tileWidth),
      );
    const audioLeft = Math.max(0, -begin),
      audioRight = Math.min(width, c.duration * c.pixelsPerSecond - begin);
    ctx.save();
    ctx.beginPath();
    ctx.rect(audioLeft, 0, Math.max(0, audioRight - audioLeft), c.height);
    ctx.clip();
    for (let number = first; number <= last; number++) {
      const tile = this.tile(number);
      if (!tile) continue;
      ctx.drawImage(
        tile,
        number * this.tileWidth - begin,
        0,
        this.tileWidth,
        c.height,
      );
    }
    ctx.restore();
  }
}

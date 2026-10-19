/** Bounded diagnostics, sampled through the existing UI state interface.
 * Timing records stay local and never log or retain source/user data. */
class WindowedTimings {
  private values: number[] = [];
  record(value: number) {
    if (!Number.isFinite(value) || value < 0) return;
    if (this.values.length === 120) this.values.shift();
    this.values.push(value);
  }
  summary() {
    const sorted = [...this.values].sort((a, b) => a - b);
    return {
      samples: sorted.length,
      median_ms: sorted[Math.floor(sorted.length / 2)] ?? 0,
      p90_ms: sorted[Math.ceil(sorted.length * 0.9) - 1] ?? 0,
      p99_ms: sorted[Math.ceil(sorted.length * 0.99) - 1] ?? 0,
      p95_ms: sorted[Math.ceil(sorted.length * 0.95) - 1] ?? 0,
      max_ms: sorted.at(-1) ?? 0,
    };
  }
}
export const transportTimings = new WindowedTimings();
export const libraryFrameTimings = new WindowedTimings();
export const libraryRenderTimings = new WindowedTimings();
export const libraryPageTimings = new WindowedTimings();
export const metadataTimings = new WindowedTimings();
const waveforms = new Map<
  string,
  {
    paint: WindowedTimings;
    frame: WindowedTimings;
    last: number | null;
    paints: number;
    tiles: () => number;
  }
>();
export function registerWaveformPerformance(key: string, tiles: () => number) {
  const metrics = {
    paint: new WindowedTimings(),
    frame: new WindowedTimings(),
    last: null as number | null,
    paints: 0,
    tiles,
  };
  waveforms.set(key, metrics);
  return {
    paint(_now: number, milliseconds: number, _animating: boolean) {
      metrics.paints++;
      metrics.paint.record(milliseconds);
    },
    frame(now: number, animating: boolean) {
      if (animating && metrics.last !== null && now > metrics.last)
        metrics.frame.record(now - metrics.last);
      metrics.last = animating ? now : null;
    },
    dispose() {
      if (waveforms.get(key) === metrics) waveforms.delete(key);
    },
  };
}
export function performanceSnapshot() {
  return {
    display: {
      focused: document.hasFocus(),
      visibility: document.visibilityState,
      dpr: window.devicePixelRatio,
    },
    library: {
      frame: libraryFrameTimings.summary(),
      render: libraryRenderTimings.summary(),
      page: libraryPageTimings.summary(),
    },
    transport: transportTimings.summary(),
    metadata: metadataTimings.summary(),
    waveforms: Object.fromEntries(
      [...waveforms].map(([key, m]) => [
        key,
        {
          paint: m.paint.summary(),
          frame: m.frame.summary(),
          paints: m.paints,
          tiles: m.tiles(),
        },
      ]),
    ),
  };
}

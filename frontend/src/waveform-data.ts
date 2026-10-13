import type { WaveformBands } from "./types";

export const BAND_COLORS = {
  low: "#0055e1",
  mid: "#ffa600",
  high: "#ffffff",
} as const;

export function hasWaveformBands(
  bands: WaveformBands | null | undefined,
  peakCount: number,
): bands is WaveformBands {
  return (
    !!bands &&
    peakCount > 0 &&
    bands.low.length === peakCount &&
    bands.mid.length === peakCount &&
    bands.high.length === peakCount
  );
}

export interface WaveformWindow {
  start: number;
  end: number;
  span: number;
  playheadFraction: number;
}

/** Detail uses a virtual window around the playhead, including blank file margins. */
export function getWaveformWindow({
  position,
  duration,
  zoom = 1,
  overview = false,
  bpm,
  spanBeats,
}: {
  position: number;
  duration: number;
  zoom?: number;
  overview?: boolean;
  bpm?: number;
  spanBeats?: number;
}): WaveformWindow {
  const length = Number.isFinite(duration) ? Math.max(0, duration) : 0;
  const playhead = Number.isFinite(position)
    ? Math.max(0, Math.min(length, position))
    : 0;
  if (overview)
    return {
      start: 0,
      end: length,
      span: length,
      playheadFraction: length ? playhead / length : 0,
    };
  const scale = Number.isFinite(zoom) && zoom > 0 ? zoom : 1;
  const beatSpan =
    Number.isFinite(spanBeats) &&
    spanBeats! > 0 &&
    Number.isFinite(bpm) &&
    bpm! > 0
      ? (spanBeats! * 60) / bpm!
      : null;
  const span = beatSpan ?? 40 / scale;
  return {
    start: playhead - span / 2,
    end: playhead + span / 2,
    span,
    playheadFraction: 0.5,
  };
}

export function timeToWaveformFraction(
  seconds: number,
  window: WaveformWindow,
): number {
  return window.span > 0 ? (seconds - window.start) / window.span : 0;
}

/** Clicks in blank margins seek to the actual file boundary. */
export function waveformSeekSeconds(
  window: WaveformWindow,
  fraction: number,
  duration: number,
): number {
  const relative = Number.isFinite(fraction)
    ? Math.max(0, Math.min(1, fraction))
    : 0;
  const length = Number.isFinite(duration) ? Math.max(0, duration) : 0;
  return Math.max(0, Math.min(length, window.start + relative * window.span));
}

export function beatSpacingFraction(
  window: WaveformWindow,
  bpm: number,
): number | null {
  return Number.isFinite(bpm) && bpm > 0 && window.span > 0
    ? 60 / bpm / window.span
    : null;
}

const amplitude = (n: number | undefined) =>
  Number.isFinite(n) ? Math.max(0, Math.min(1, Math.abs(n!))) : 0;

/** Preserve narrow peaks when shrinking; interpolate the existing envelope when enlarging. */
export function rebinEnvelope(
  samples: readonly number[],
  start: number,
  end: number,
  columns: number,
  mode: "peak" | "rms" = "peak",
): Float32Array {
  const count = Number.isFinite(columns) ? Math.max(0, Math.floor(columns)) : 0;
  const output = new Float32Array(count);
  if (
    !samples.length ||
    !count ||
    !Number.isFinite(start) ||
    !Number.isFinite(end) ||
    end <= start
  )
    return output;
  // Keep virtual coordinates: clipping this range would stretch the audio over
  // padding and repeat endpoint data while the playhead approached either edge.
  const first = start * samples.length;
  const last = end * samples.length;
  const step = (last - first) / count;
  for (let x = 0; x < count; x++) {
    const from = first + x * step;
    const to = from + step;
    if (to <= 0 || from >= samples.length) continue;
    if (step < 1) {
      const sourceCenter = (from + to) / 2;
      if (sourceCenter < 0 || sourceCenter >= samples.length) continue;
      const center = Math.max(
        0,
        Math.min(samples.length - 1, sourceCenter - 0.5),
      );
      const lower = Math.floor(center),
        fraction = center - lower;
      output[x] =
        amplitude(samples[lower]) * (1 - fraction) +
        amplitude(samples[Math.min(samples.length - 1, lower + 1)]) * fraction;
      continue;
    }
    let max = 0,
      power = 0;
    for (
      let i = Math.max(0, Math.floor(from));
      i < Math.min(samples.length, Math.ceil(to));
      i++
    ) {
      const overlap = Math.max(0, Math.min(to, i + 1) - Math.max(from, i));
      if (!overlap) continue;
      const value = amplitude(samples[i]);
      max = Math.max(max, value);
      power += value * value * overlap;
    }
    output[x] = mode === "peak" ? max : Math.sqrt(power / step);
  }
  return output;
}

/** Interpolation between columns without independent normalization of any band. */
export function smoothEnvelope(samples: Float32Array): Float32Array {
  const result = new Float32Array(samples.length);
  for (let i = 0; i < samples.length; i++)
    result[i] =
      samples[i] * 0.75 +
      samples[Math.max(0, i - 1)] * 0.125 +
      samples[Math.min(samples.length - 1, i + 1)] * 0.125;
  return result;
}

/** 4/4 bar.beat label on a known source beatgrid; no guessed tempo. */
export function waveformBarBeat(
  position: number,
  bpm: number,
  offset = 0,
): string | null {
  if (
    !Number.isFinite(position) ||
    !Number.isFinite(bpm) ||
    bpm <= 0 ||
    !Number.isFinite(offset)
  )
    return null;
  const index = Math.floor(((position - offset) * bpm) / 60 + 1e-8);
  const bar = Math.floor(index / 4) + (index >= 0 ? 1 : 0);
  const beat = (((index % 4) + 4) % 4) + 1;
  return `${bar}.${beat}`;
}

export const HOT_CUE_COLORS = [
  "#ed7378",
  "#ea9d65",
  "#e8c16b",
  "#a6c96e",
  "#64c8a6",
  "#69bddb",
  "#8a9ae8",
  "#c58adc",
] as const;

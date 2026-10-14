import type { DeckSnapshot, TrackRecord } from "./types";

/** A source-time view of the native engine's piecewise beat clock. */
export interface BeatGrid {
  valid: boolean;
  bpm: number;
  offset: number;
  beats: readonly number[] | null;
  downbeats: readonly number[];
  anchor: number;
  scale: number;
  manual: boolean;
  hasDownbeats: boolean;
  meter: number;
}
const finite = (value: number | null | undefined, fallback: number) =>
  value != null && Number.isFinite(value) ? value : fallback;
export function uniformBeatGrid(
  bpm: number,
  offset: number,
  valid: boolean,
  downbeat: boolean,
): BeatGrid {
  return {
    valid: valid && Number.isFinite(bpm) && bpm > 0 && Number.isFinite(offset),
    bpm,
    offset,
    beats: null,
    downbeats: [],
    anchor: 0,
    scale: 1,
    manual: downbeat,
    hasDownbeats: downbeat,
    meter: 4,
  };
}
export function deckBeatGrid(deck: DeckSnapshot, track = deck.track): BeatGrid {
  const rhythm = track?.neural_rhythm;
  const bpm = finite(deck.bpm, finite(track?.bpm, finite(rhythm?.bpm, 0)));
  const offset = finite(deck.beatgrid_offset_seconds, 0);
  const grid = uniformBeatGrid(
    bpm,
    offset,
    !!deck.beatgrid_valid,
    !!deck.beatgrid_downbeat,
  );
  grid.meter = rhythm?.beats_per_bar && rhythm.beats_per_bar >= 2 && rhythm.beats_per_bar <= 12 ? rhythm.beats_per_bar : 4;
  grid.manual = deck.beatgrid_manual ?? track?.beatgrid_source === "manual";
  if (
    deck.beatgrid_uniform ||
    !rhythm ||
    rhythm.beats_seconds.length < 2 ||
    !Number.isFinite(rhythm.bpm) ||
    rhythm.bpm <= 0 ||
    bpm <= 0
  )
    return grid;
  const anchor = finite(
    deck.beatgrid_anchor_index,
    finite(
      track?.manual_grid_anchor_index,
      lowerBound(
        rhythm.beats_seconds,
        rhythm.downbeats_seconds[0] ?? rhythm.beats_seconds[0],
      ),
    ),
  );
  return {
    ...grid,
    beats: rhythm.beats_seconds,
    downbeats: rhythm.downbeats_seconds,
    anchor: Math.max(
      0,
      Math.min(rhythm.beats_seconds.length - 1, Math.trunc(anchor)),
    ),
    scale: rhythm.bpm / bpm,
    hasDownbeats: grid.manual || rhythm.downbeats_seconds.length > 0,
  };
}
function lowerBound(times: readonly number[], value: number): number {
  let low = 0,
    high = times.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (times[middle] < value) low = middle + 1;
    else high = middle;
  }
  return low;
}
function sourcePosition(times: readonly number[], seconds: number): number {
  let left = lowerBound(times, seconds);
  if (times[left] === seconds) return left;
  left = Math.max(0, Math.min(times.length - 2, left - 1));
  return left + (seconds - times[left]) / (times[left + 1] - times[left]);
}
function sourceTime(times: readonly number[], beat: number): number {
  const left = Math.max(0, Math.min(times.length - 2, Math.floor(beat)));
  return times[left] + (beat - left) * (times[left + 1] - times[left]);
}
export function gridBeatPosition(
  grid: BeatGrid,
  seconds: number,
): number | null {
  if (!grid.valid || !Number.isFinite(seconds)) return null;
  return grid.beats
    ? sourcePosition(
        grid.beats,
        grid.beats[grid.anchor] + (seconds - grid.offset) / grid.scale,
      ) - grid.anchor
    : ((seconds - grid.offset) * grid.bpm) / 60;
}
export function gridBeatTime(grid: BeatGrid, beat: number): number | null {
  if (!grid.valid || !Number.isFinite(beat)) return null;
  return grid.beats
    ? grid.offset +
        (sourceTime(grid.beats, beat + grid.anchor) - grid.beats[grid.anchor]) *
          grid.scale
    : grid.offset + (beat * 60) / grid.bpm;
}
export function reportedBeatPosition(deck: DeckSnapshot): number | null {
  if (!deck.beatgrid_valid) return null;
  if (deck.beat_position != null && Number.isFinite(deck.beat_position))
    return deck.beat_position;
  return gridBeatPosition(
    deckBeatGrid(deck),
    finite(deck.position_seconds, finite(deck.position, 0)),
  );
}
function downbeatIndex(grid: BeatGrid, index: number): boolean {
  if (!grid.hasDownbeats) return false;
  if (grid.manual || !grid.beats) return (index - grid.anchor) % grid.meter === 0;
  const at = lowerBound(grid.downbeats, grid.beats[index]);
  return grid.downbeats[at] === grid.beats[index];
}
export interface BeatMarker {
  time: number;
  index: number;
  downbeat: boolean;
}
/** Draw analysis events, including declared pulse reconstructions; no clock extrapolation markers. */
export function visibleGridBeats(
  grid: BeatGrid,
  start: number,
  end: number,
  duration: number,
): BeatMarker[] {
  if (
    !grid.valid ||
    !Number.isFinite(start) ||
    !Number.isFinite(end) ||
    end < start
  )
    return [];
  const lo = Math.max(0, start),
    hi = Math.min(duration, end);
  if (hi < lo) return [];
  const markers: BeatMarker[] = [];
  if (grid.beats) {
    const rawStart = grid.beats[grid.anchor] + (lo - grid.offset) / grid.scale;
    for (
      let index = lowerBound(grid.beats, rawStart);
      index < grid.beats.length;
      index++
    ) {
      const time =
        grid.offset +
        (grid.beats[index] - grid.beats[grid.anchor]) * grid.scale;
      if (time > hi) break;
      markers.push({ time, index, downbeat: downbeatIndex(grid, index) });
    }
  } else {
    const first = Math.ceil(((lo - grid.offset) * grid.bpm) / 60);
    for (let index = first; ; index++) {
      const time = grid.offset + (index * 60) / grid.bpm;
      if (time > hi) break;
      markers.push({ time, index, downbeat: downbeatIndex(grid, index) });
    }
  }
  return markers;
}
export function snapToGridBeat(
  grid: BeatGrid,
  seconds: number,
  duration: number,
): number {
  const value = Math.max(0, Math.min(duration, seconds));
  const position = gridBeatPosition(grid, value);
  if (position === null) return value;
  let beat = Math.round(position);
  if (grid.beats) {
    const rawStart = grid.beats[grid.anchor] - grid.offset / grid.scale;
    const rawEnd =
      grid.beats[grid.anchor] + (duration - grid.offset) / grid.scale;
    const first = lowerBound(grid.beats, rawStart);
    let last = lowerBound(grid.beats, rawEnd);
    if (grid.beats[last] !== rawEnd) last--;
    if (first > last || first >= grid.beats.length) return value;
    beat = Math.max(first, Math.min(last, beat + grid.anchor)) - grid.anchor;
  }
  return Math.max(0, Math.min(duration, gridBeatTime(grid, beat) ?? value));
}
/** Model bar.beat counts reset at actual predicted downbeats, including 3/4 or
 * changing meter. A manual anchor preserves the detected bar length, with four as the unknown-meter fallback. */
export function gridBarBeat(grid: BeatGrid, seconds: number): string | null {
  const position = gridBeatPosition(grid, seconds);
  if (position === null || !grid.hasDownbeats) return null;
  const relative = Math.floor(position + 1e-8);
  if (grid.manual || !grid.beats) {
    const bar = Math.floor(relative / grid.meter) + (relative >= 0 ? 1 : 0);
    const beat = (((relative % grid.meter) + grid.meter) % grid.meter) + 1;
    return `${bar}.${beat}`;
  }
  const index = relative + grid.anchor;
  if (index < 0 || index >= grid.beats.length) return null;
  const raw = grid.beats[index];
  let downbeat = lowerBound(grid.downbeats, raw);
  if (grid.downbeats[downbeat] !== raw) downbeat--;
  if (downbeat < 0) return null;
  const startIndex = lowerBound(grid.beats, grid.downbeats[downbeat]);
  return `${downbeat + 1}.${index - startIndex + 1}`;
}
export function gridStatus(
  deck: DeckSnapshot,
  track?: TrackRecord | null,
): string {
  const source = track ?? deck.track;
  const manual = deck.beatgrid_manual ?? source?.beatgrid_source === "manual";
  const rhythm = source?.neural_rhythm;
  if (manual)
    return rhythm && !deck.beatgrid_uniform
      ? "Manual bar 1 · model timing retained"
      : "Manual bar 1 · uniform timing";
  if (deck.beatgrid_uniform) return "Manual tempo · align bar one";
  if (rhythm)
    return rhythm.downbeats_seconds.length
      ? "Model beats and downbeats · verify bar alignment"
      : "Model beats · downbeats unconfirmed";
  if (source?.rhythm_error)
    return deck.beatgrid_valid
      ? "Model analysis failed · estimated beat fallback"
      : "Model analysis failed · grid unknown";
  return deck.beatgrid_valid
    ? "Estimated beats · downbeats unconfirmed"
    : "Grid unknown · align or analyze";
}

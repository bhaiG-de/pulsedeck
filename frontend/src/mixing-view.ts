/** Source-time grid phase; tempo affects wall-clock speed, not sample positions. */
export function beatPosition(
  position: number,
  bpm: number,
  offset = 0,
): number | null {
  if (
    !Number.isFinite(position) ||
    !Number.isFinite(bpm) ||
    bpm <= 0 ||
    !Number.isFinite(offset)
  )
    return null;
  return ((position - offset) * bpm) / 60;
}
export function beatPhaseDelta(
  a: number | null,
  b: number | null,
): number | null {
  if (a === null || b === null) return null;
  return ((((b - a + 0.5) % 1) + 1) % 1) - 0.5;
}
export function snapToBeat(
  seconds: number,
  bpm: number,
  offset: number,
  duration: number,
): number {
  const value = Math.max(0, Math.min(duration, seconds));
  if (!Number.isFinite(bpm) || bpm <= 0) return value;
  const beat = 60 / bpm;
  return Math.max(
    0,
    Math.min(duration, offset + Math.round((value - offset) / beat) * beat),
  );
}

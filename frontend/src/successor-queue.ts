export interface QueueTrack {
  source: string;
  title: string;
  artist?: string;
  id?: number;
  state?: string;
  availability?: string;
  reason?: string | null;
  technique?: string | null;
}
export interface SuccessorQueue {
  queue_id: string | null;
  revision: number;
  state: string;
  terminal: boolean;
  readiness?: string;
  coverage?: {
    indexed_members: number;
    analysed_members: number;
    needs_analysis_members: number;
  } | null;
  reason?: string | null;
  mode?: "ordered" | "collection";
  items: QueueTrack[];
  completed_moves?: number;
  current_item?: number | null;
  started_unix_ms?: number;
  finish_unix_ms?: number | null;
  policy?: {
    playlist?: string | null;
    provider?: string | null;
    energy_direction: string;
    max_stretch_percent: number;
    techniques?: string[] | null;
    max_skips: number;
  };
}
export function queueStartArgs(settings: {
  mode: "ordered" | "collection";
  collection: string;
  sources: string[];
  direction: string;
  technique: string;
  minutes: string;
  stretch: string;
  replace: boolean;
}): Record<string, unknown> {
  const stretch = Number(settings.stretch);
  const seconds = settings.minutes.trim()
    ? Number(settings.minutes) * 60
    : null;
  if (
    !settings.stretch.trim() ||
    !Number.isFinite(stretch) ||
    stretch < 0 ||
    stretch > 100
  )
    throw new Error("Choose a maximum tempo change between 0 and 100%.");
  if (
    seconds !== null &&
    (!Number.isFinite(seconds) || seconds < 1 || seconds > 86400)
  )
    throw new Error(
      "Choose a duration between 1 second and 24 hours, or leave it blank.",
    );
  if (settings.mode === "ordered" && !settings.sources.length)
    throw new Error("Add tracks to your order, or choose a collection.");
  return {
    ...(settings.mode === "ordered"
      ? { sources: settings.sources }
      : settings.collection === "liked"
        ? { playlist: "spotify:liked", provider: "spotify" }
        : { provider: "local" }),
    energy_direction: settings.direction,
    max_stretch_percent: stretch,
    ...(settings.technique !== "all"
      ? { techniques: [settings.technique] }
      : {}),
    replace_prepared: settings.replace,
    max_skips: 0,
    ...(seconds !== null ? { duration_seconds: seconds } : {}),
  };
}
export function mergeQueue(
  previous: SuccessorQueue | null,
  next: SuccessorQueue | null,
): SuccessorQueue | null {
  if (!next?.queue_id) return previous ?? next;
  if (previous?.queue_id === next.queue_id && previous.revision > next.revision)
    return previous;
  if (
    previous?.queue_id !== next.queue_id &&
    (previous?.started_unix_ms ?? 0) > (next.started_unix_ms ?? 0)
  )
    return previous;
  return next;
}
export function reorderQueue(
  sources: string[],
  index: number,
  delta: number,
): string[] {
  const destination = index + delta;
  if (
    index < 0 ||
    index >= sources.length ||
    destination < 0 ||
    destination >= sources.length
  )
    return sources;
  const next = [...sources];
  [next[index], next[destination]] = [next[destination], next[index]];
  return next;
}

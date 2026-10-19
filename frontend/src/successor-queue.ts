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

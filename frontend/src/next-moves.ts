import type { EnergyDirection } from "./suggestions";
export interface PassageFeatures {
  start_seconds: number;
  end_seconds: number;
  low: number;
  mid: number;
  high: number;
  level: number;
  pulse_spread: number;
  local_bpm: number;
}
export interface NextMove {
  id: string;
  source: string;
  title: string;
  artist: string;
  spotify: boolean;
  reference_deck: number;
  target_deck: number;
  technique: string;
  overlap_bars: number;
  entry_seconds: number;
  launch_seconds: number;
  audible_seconds: number;
  handover_seconds: number;
  clear_seconds: number;
  target_bpm: number;
  native_bpm: number;
  tempo_percent: number;
  review_required: boolean;
  evidence: string[];
  warnings: string[];
  launch_in_seconds: number;
  prepare_within_seconds: number;
  preparation_budget_seconds: number;
  continuation_seconds?: number;
  minimum_continuation_seconds?: number;
  next_decision_within_seconds?: number;
  readiness: string;
  outgoing: PassageFeatures;
  incoming: PassageFeatures;
  score_components: Record<string, number>;
}
export interface PlannerMember {
  source: string;
  title: string;
  artist: string;
  duration_seconds: number;
}
export interface MoveList {
  considered_tracks?: number;
  rejected_window_checks?: Record<string, number>;
  version: number;
  reference_deck: number | null;
  target_deck: number | null;
  reference_title: string | null;
  reference_position_seconds: number;
  options: NextMove[];
  coverage: {
    total_members: number | null;
    indexed_members: number;
    analysed_members: number;
    needs_analysis_members: number;
    complete: boolean;
    needs_analysis: PlannerMember[];
    needs_analysis_truncated: boolean;
    scope: string | null;
  };
  capabilities: { technique: string; available: boolean; reason: string }[];
  reasons: string[];
  elapsed_ms: number;
}
export interface MixOperation {
  prepared_id: string;
  operation_id: string;
  state: string;
  job_id?: string;
  option?: NextMove;
  error?: { message: string };
  ready?: boolean;
  automatic?: boolean;
  fallback_used?: boolean;
  terminal?: boolean;
  event_sequence?: number | null;
}
export const TECHNIQUE_NAMES: Record<string, string> = {
  eq_blend: "EQ / layer blend",
  filter_blend: "Filter blend",
  short_exchange: "Short phrase exchange",
  bass_exchange: "Bass exchange",
  payoff_replacement: "Payoff replacement",
  breakdown_bridge: "Breakdown bridge",
  vocal_handoff: "Vocal handoff",
  echo_out: "Echo out",
  reverb_exit: "Reverb exit",
  loop_blend: "Loop assisted blend",
  tempo_bridge: "Tempo bridge",
  key_shift_bridge: "Key shift bridge",
  stem_blend: "Stem blend",
  performance_gesture: "Performance gesture",
};
export function sourceTime(seconds: number): string {
  const t = Math.max(0, seconds);
  return `${Math.floor(t / 60)}:${(t % 60).toFixed(1).padStart(4, "0")}`;
}
export function moveCountdown(
  move: NextMove,
  referencePosition: number,
  rate: number,
) {
  return (move.launch_seconds - referencePosition) / (rate > 0 ? rate : 1);
}
export function sameMove(
  a: NextMove | null | undefined,
  b: NextMove | null | undefined,
) {
  return !!(
    a &&
    b &&
    a.source === b.source &&
    a.technique === b.technique &&
    a.reference_deck === b.reference_deck &&
    a.target_deck === b.target_deck &&
    Math.abs(a.entry_seconds - b.entry_seconds) < 0.05 &&
    Math.abs(a.launch_seconds - b.launch_seconds) < 0.05 &&
    Math.abs(a.clear_seconds - b.clear_seconds) < 0.05
  );
}
export function plannerRequestKey(
  loadRevisions: number[],
  controls: number[],
  positions: number[],
  direction: EnergyDirection,
  scope: string,
) {
  return JSON.stringify([loadRevisions, controls, positions, direction, scope]);
}

/** Keep full option evidence through compact feedback and reject delayed stale snapshots. */
export function mergeMoveFeedback(
  current: MixOperation,
  update: Partial<MixOperation>,
): MixOperation {
  if (update.operation_id !== current.operation_id) return current;
  if (
    update.event_sequence != null &&
    current.event_sequence != null &&
    update.event_sequence < current.event_sequence
  )
    return current;
  return { ...current, ...update };
}

import type { DeckSnapshot, EngineSnapshot } from "./types";

/** Mirrors `suggest::SuggestionList` from `suggest_next`. */
export type EnergyDirection = "keep" | "raise" | "lower";
export type KeyRelation =
  "same" | "step_up" | "step_down" | "relative" | "clash" | "unknown";
export interface TempoMatch {
  /** Tempo change the other deck needs to match the playing deck. */
  percent: number;
  /** 2: a half-time track counted double; 0.5: a double-time track counted half. */
  multiple: number;
  score: number;
}
export interface Suggestion {
  source: string;
  spotify: boolean;
  track_id: number | null;
  title: string;
  artist: string;
  duration_seconds: number;
  bpm: number;
  camelot: string | null;
  energy: number | null;
  energy_level: number | null;
  key: KeyRelation;
  key_clash: boolean;
  tempo: TempoMatch | null;
  energy_delta: number | null;
  score: number;
  reason: string;
}
export interface SuggestionReference {
  source: string;
  title: string;
  bpm: number | null;
  bpm_ambiguous: boolean;
  camelot: string | null;
  key_confidence: number | null;
  energy: number | null;
  energy_level: number | null;
}
export interface SuggestionList {
  deck: number | null;
  target_deck: number | null;
  direction: EnergyDirection;
  reference: SuggestionReference | null;
  tempo_range: number;
  suggestions: Suggestion[];
  considered: number;
  played: number;
  out_of_range: number;
  without_tempo: number;
  pending: number;
  elapsed_ms: number;
}

export const ENERGY_DIRECTIONS: readonly EnergyDirection[] = [
  "keep",
  "raise",
  "lower",
];
export const ENERGY_LABELS: Record<EnergyDirection, string> = {
  keep: "keep energy",
  raise: "raise energy",
  lower: "lower energy",
};

/** The browse knob steps through the energy choices and stops at either end. */
export function stepDirection(
  current: EnergyDirection,
  delta: number,
): EnergyDirection {
  const index = ENERGY_DIRECTIONS.indexOf(current);
  const step = Math.trunc(Number.isFinite(delta) ? delta : 0);
  const next = Math.max(
    0,
    Math.min(ENERGY_DIRECTIONS.length - 1, index + step),
  );
  return ENERGY_DIRECTIONS[next];
}

const deckLetter = (deck: number | null | undefined) =>
  deck === 1 ? "B" : "A";

function deckKey(deck: DeckSnapshot | undefined, crossfaderGain: number) {
  if (!deck) return "-";
  const fader =
    typeof deck.channel_fader === "number" &&
    Number.isFinite(deck.channel_fader)
      ? deck.channel_fader
      : 1;
  // Audibility, in tenths, is what can move the suggestions between decks.
  const level = Math.round(
    Math.max(0, Math.min(1, fader)) * crossfaderGain * 10,
  );
  const tempo = Math.round((deck.tempo_percent ?? 0) * 10);
  const bpm = Math.round((deck.bpm ?? 0) * 100);
  return [deck.track?.path ?? "", deck.playing ? 1 : 0, level, tempo, bpm].join(
    ":",
  );
}

/**
 * Everything `suggest_next` depends on, quantised so the playhead moving or a
 * fader twitching does not refetch: the loaded tracks, which deck is audible,
 * the heard tempo and the energy choice. Refetch whenever it changes.
 */
export function suggestionRequestKey(
  engine: EngineSnapshot,
  direction: EnergyDirection,
): string {
  const position = Math.max(-1, Math.min(1, engine.crossfader ?? 0));
  const angle = ((position + 1) * Math.PI) / 4;
  const gains = [Math.cos(angle), Math.sin(angle)];
  return [
    deckKey(engine.decks[0], gains[0]),
    deckKey(engine.decks[1], gains[1]),
    direction,
  ].join("|");
}

/** "following A · 124.0 bpm · 8A · energy 6 → loads into B (±6%)" */
export function referenceSummary(list: SuggestionList): string {
  const reference = list.reference;
  if (!reference || list.deck === null) return "";
  const parts = [`following ${deckLetter(list.deck)}`];
  if (reference.bpm) parts.push(`${reference.bpm.toFixed(1)} bpm`);
  if (reference.camelot) parts.push(reference.camelot);
  if (reference.energy_level) parts.push(`energy ${reference.energy_level}`);
  return `${parts.join(" · ")} → loads into ${deckLetter(list.target_deck)} (±${list.tempo_range}%)`;
}

/** Why the list is empty, or null when it is not. */
export function emptyMessage(list: SuggestionList | null): string | null {
  if (list?.suggestions.length) return null;
  if (!list || list.deck === null || !list.reference)
    return "Load a track on either deck. Suggestions follow the deck the crowd hears.";
  if (list.pending > 0) return `Preparing ${list.pending} tracks…`;
  if (list.out_of_range > 0)
    return `Nothing within deck ${deckLetter(list.target_deck)}'s ±${list.tempo_range}% tempo range; ${list.out_of_range} track${list.out_of_range === 1 ? " is" : "s are"} outside it. Widen the range to see them.`;
  if (list.played > 0)
    return "Every matching track has been played this session.";
  return "Nothing else in the library or the Spotify cache has a BPM yet. Import or analyse more tracks.";
}

/** Footer counts: what was left out and how long the ranking took. */
export function listCounts(list: SuggestionList): string {
  const parts = [`${list.considered} in reach`];
  if (list.out_of_range) parts.push(`${list.out_of_range} out of tempo range`);
  if (list.played) parts.push(`${list.played} played`);
  if (list.pending) parts.push(`${list.pending} preparing`);
  parts.push(`${list.elapsed_ms.toFixed(1)} ms`);
  return parts.join(" · ");
}

/** Drag data the decks already accept: a file path or a Spotify track. */
export function dragPayload(suggestion: Suggestion): [string, string] {
  return suggestion.spotify
    ? [
        "application/pulsedeck-spotify-track",
        JSON.stringify({
          uri: suggestion.source,
          title: suggestion.title,
          artists: suggestion.artist ? [suggestion.artist] : [],
        }),
      ]
    : ["application/pulsedeck-track", suggestion.source];
}

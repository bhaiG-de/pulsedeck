import { useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { DeckSnapshot, EngineSnapshot } from "./types";
import { deckBeatGrid, reportedBeatPosition } from "./beat-grid";
import { SECTION_NAMES, displaySections } from "./waveform-data";
import {
  barNumber,
  liveAlignment,
  liveReason,
  planGuides,
  planReason,
  planTransitions,
  startCountdown,
  transitionRoles,
  type DeckStructure,
  type TransitionPlan,
  type WaveformGuide,
} from "./transition";
import "./styles/transition.css";

/** The decks' record colours, as the waveforms draw them. */
const DECK_COLORS = ["#f5d90a", "#86cfee"];
const letter = (deck: number) => (deck === 0 ? "A" : "B");
const num = (value: unknown, fallback = 0) =>
  typeof value === "number" && Number.isFinite(value) ? value : fallback;
const noGuides: WaveformGuide[] = [];

function useStructure(deck: DeckSnapshot | undefined): DeckStructure | null {
  const track = deck?.track;
  const bpm = num(deck?.bpm, num(track?.bpm));
  const offset = num(deck?.beatgrid_offset_seconds);
  return useMemo(() => {
    if (!track || !deck?.beatgrid_valid || !deck.beatgrid_downbeat) return null;
    if (track.sections && (Math.abs(track.sections.bpm - bpm) > 0.01 || Math.abs(track.sections.downbeat_offset_seconds - offset) > 0.002)) return null;
    const duration = num(track.duration_seconds, num(deck?.duration_seconds));
    return {
      bpm,
      offset,
      grid: deckBeatGrid(deck, track),
      barTimes: track.sections?.bar_times,
      beatsPerBar: track.sections?.beats_per_bar ?? track.neural_rhythm?.beats_per_bar ?? 4,
      duration,
      sections: displaySections(track.sections, duration),
      vocabulary:
        track.sections?.vocabulary_override ?? track.sections?.vocabulary,
    };
    // The track object keeps its identity between transport frames.
  }, [track, bpm, offset, deck?.beatgrid_valid, deck?.beatgrid_downbeat, deck?.beatgrid_manual, deck?.beatgrid_uniform, deck?.beatgrid_anchor_index]);
}

export interface TransitionGuideState {
  out: 0 | 1 | null;
  incoming: 0 | 1 | null;
  live: boolean;
  plans: TransitionPlan[];
  choice: number;
  setChoice: (index: number) => void;
  /** One line for the toolbar, or why there is nothing to suggest. */
  text: string;
  /** The deck and seconds the "cue" key moves the waiting deck's cue to. */
  cue: { deck: number; seconds: number } | null;
  guides: [WaveformGuide[], WaveformGuide[]];
  countdowns: [
    { text: string; color: string } | null,
    { text: string; color: string } | null,
  ];
}

/**
 * The transition guide for the current decks: plans while the waiting deck
 * is paused, a live alignment check once it plays. Plans are recomputed once
 * per beat of the playing deck, not every frame.
 */
export function useTransitionGuide(
  engine: EngineSnapshot,
  enabled: boolean,
): TransitionGuideState {
  const structures = [
    useStructure(engine.decks[0]),
    useStructure(engine.decks[1]),
  ];
  const roles = enabled ? transitionRoles(engine) : null;
  const out = roles?.out ?? null;
  const incoming = roles?.incoming ?? null;
  const outStructure = out === null ? null : structures[out];
  const inStructure = incoming === null ? null : structures[incoming];
  const outPosition =
    out === null ? 0 : num(engine.decks[out]?.position_seconds);
  const beat = out === null ? 0 : Math.floor(reportedBeatPosition(engine.decks[out]) ?? 0);
  const live = !!roles?.live;
  const plans = useMemo(
    () =>
      live || !outStructure
        ? []
        : planTransitions(outStructure, inStructure, outPosition),
    // Once per beat keeps the list steady while the playhead moves.
    [live, outStructure, inStructure, beat],
  );
  const [choice, setChoice] = useState(0);
  const planKey = `${out}:${engine.decks[0]?.track?.path}:${engine.decks[1]?.track?.path}`;
  useEffect(() => setChoice(0), [planKey]);
  const state: TransitionGuideState = {
    out,
    incoming,
    live,
    plans,
    choice: Math.min(choice, Math.max(0, plans.length - 1)),
    setChoice,
    text: "",
    cue: null,
    guides: [noGuides, noGuides],
    countdowns: [null, null],
  };
  if (!enabled) return state;
  if (out === null || incoming === null) {
    state.text = "Play one deck and load the other";
    return state;
  }
  const [outLetter, inLetter] = [letter(out), letter(incoming)];
  const inColor = DECK_COLORS[incoming];
  if (!outStructure?.sections.length || !inStructure?.sections.length) {
    state.text = "Needs song sections on both decks (beat analysis)";
    return state;
  }
  if (outStructure.beatsPerBar !== inStructure.beatsPerBar) {
    state.text = "Mix guide needs matching meters on both decks";
    return state;
  }
  const guides: [WaveformGuide[], WaveformGuide[]] = [[], []];
  if (live) {
    const alignment = liveAlignment(
      outStructure,
      inStructure,
      outPosition,
      num(engine.decks[incoming]?.position_seconds),
    );
    if (alignment) {
      state.text =
        liveReason(alignment, outStructure, outLetter, inLetter) ||
        `${inLetter} is playing; nothing left to line up`;
      if (alignment.landing !== null && alignment.landingLabel)
        guides[out] = [
          {
            kind: "landing",
            at: alignment.landing,
            label: `${inLetter} ${SECTION_NAMES[alignment.landingLabel].toUpperCase()}`,
            color: inColor,
          },
        ];
    }
    state.guides = guides;
    return state;
  }
  const plan = plans[state.choice];
  if (!plan) {
    state.text = `No phrase left in ${outLetter} to start ${inLetter} on`;
    return state;
  }
  const marks = planGuides(plan, inLetter, inColor);
  guides[out] = marks.out;
  guides[incoming] = marks.incoming;
  state.guides = guides;
  const label = startCountdown(plan, outStructure, outPosition, inLetter);
  if (label) state.countdowns[out] = { text: label, color: inColor };
  state.cue = { deck: incoming, seconds: plan.cueAt };
  const fromTop = Math.abs(plan.cueAt - inStructure.sections[0].start) < 1e-3;
  const from = fromTop
    ? ""
    : ` from ${inLetter} bar ${barNumber(inStructure, plan.cueAt)}`;
  state.text = `start ${inLetter} at ${outLetter} bar ${barNumber(outStructure, plan.startAt)}${from} · ${planReason(plan, outLetter, inLetter)}`;
  return state;
}

/** The toolbar line: an on/off key, the suggestion, alternatives and cue. */
export function TransitionReadout({
  state,
  enabled,
  onToggle,
  onCue,
}: {
  state: TransitionGuideState;
  enabled: boolean;
  onToggle: () => void;
  onCue: (deck: number, seconds: number) => void;
}) {
  const count = state.plans.length;
  return (
    <div className="transition-readout">
      <button
        className={`mix-guide-toggle ${enabled ? "enabled" : ""}`}
        aria-pressed={enabled}
        title="Show when to start the other deck so the songs' sections line up"
        onClick={onToggle}
      >
        MIX GUIDE
      </button>
      {enabled && (
        <span className="transition-text" title={state.text}>
          {state.text}
        </span>
      )}
      {enabled && !state.live && count > 1 && (
        <span className="transition-choices">
          <button
            aria-label="Previous suggestion"
            disabled={state.choice === 0}
            onClick={() => state.setChoice(state.choice - 1)}
          >
            <ChevronLeft size={12} />
          </button>
          <span>
            {state.choice + 1}/{count}
          </span>
          <button
            aria-label="Next suggestion"
            disabled={state.choice >= count - 1}
            onClick={() => state.setChoice(state.choice + 1)}
          >
            <ChevronRight size={12} />
          </button>
        </span>
      )}
      {enabled && state.cue && (
        <button
          title={`Move deck ${letter(state.cue.deck)}'s cue point to the suggested start`}
          onClick={() => onCue(state.cue!.deck, state.cue!.seconds)}
        >
          CUE {letter(state.cue.deck)}
        </button>
      )}
    </div>
  );
}

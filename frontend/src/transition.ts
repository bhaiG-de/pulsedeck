import type { EngineSnapshot, SectionLabel } from "./types";
import { SECTION_NAMES, type DisplaySection } from "./waveform-data.ts";

/**
 * Transition guide: when to start the waiting deck so the two tracks'
 * sections line up. Everything is counted in bars on each deck's own grid,
 * so it holds once the decks are beatmatched whatever their source tempos.
 * The DJ still does the mix; this only points out the moment.
 */

/** One deck's structure on its own beatgrid. */
export interface DeckStructure {
  /** The grid's BPM, without the deck's tempo change. */
  bpm: number;
  /** A downbeat (beat 1), in seconds. */
  offset: number;
  duration: number;
  sections: DisplaySection[];
  /** Hip-hop mixes run shorter than dance mixes. */
  vocabulary?: "dance" | "hip_hop" | null;
}

export type TransitionKind = "intro_over_outro" | "drop_swap" | "phrase";
type Landing = "out_end" | "swap" | "late" | "clash" | "calm";

export interface TransitionPlan {
  /** Playing-deck seconds at which to start the waiting deck. */
  startAt: number;
  /** Waiting-deck seconds to start from. */
  cueAt: number;
  /** Bars both tracks play before the new one takes over. */
  togetherBars: number;
  /** Playing-deck seconds where the new track's first drop or hook starts. */
  landing: number | null;
  landingLabel: SectionLabel | null;
  landingKind: Landing | null;
  /** Playing-deck seconds where its music ends. */
  outEnd: number;
  /** One bar of the playing deck, in its seconds. */
  bar: number;
  /** The playing track's section at the start, and the new track's. */
  outLabel: SectionLabel | null;
  cueLabel: SectionLabel | null;
  /** The playing track's last drop or hook, when the new one takes over from it. */
  swapLabel: SectionLabel | null;
  kind: TransitionKind;
  score: number;
}

const MAIN = 0.75;
const barSeconds = (deck: DeckStructure) => 240 / deck.bpm;

export function usableStructure(
  deck: DeckStructure | null | undefined,
): deck is DeckStructure {
  return (
    !!deck &&
    Number.isFinite(deck.bpm) &&
    deck.bpm > 0 &&
    Number.isFinite(deck.offset) &&
    deck.duration > 0 &&
    deck.sections.length > 0
  );
}

/** How much a section fills the mix: drops and hooks 1, intros and outros 0.2. */
export function intensity(section: DisplaySection | undefined): number {
  if (!section) return 0;
  switch (section.label) {
    case "drop":
    case "hook":
      return 1;
    case "verse":
      return 0.6;
    case "build_up":
      return 0.5;
    case "breakdown":
      return 0.3;
    case "intro":
    case "outro":
      return 0.2;
    default:
      return 0.5;
  }
}

function sectionAt(deck: DeckStructure, seconds: number) {
  return deck.sections.find((s) => s.start <= seconds && seconds < s.end);
}

/** Phrase starts: every section start, and every 8 bars inside a section. */
export function phraseStarts(deck: DeckStructure): number[] {
  const bar = barSeconds(deck);
  const starts: number[] = [];
  for (const s of deck.sections)
    for (let k = 0; s.start + k * 8 * bar < s.end - bar / 2; k++)
      starts.push(s.start + k * 8 * bar);
  return starts;
}

/** How well a stretch of `bars` played together suits the style: dance
 * mixes like 16 to 32 bars, hip-hop mixes 8 to 16. */
export function togetherFit(bars: number, hipHop = false): number {
  const [shortest, longest] = hipHop ? [8, 16] : [16, 32];
  if (bars < shortest) return 0.8 * (Math.max(0, bars) / shortest);
  if (bars > longest)
    return Math.max(0.2, 1 - (bars - longest) / (2 * longest));
  return 1;
}

/** Bar number as the waveform counts it (bar 1 starts on the grid's beat 1). */
export function barNumber(deck: DeckStructure, seconds: number): number {
  const index = Math.floor(((seconds - deck.offset) * deck.bpm) / 60 + 1e-8);
  return Math.floor(index / 4) + (index >= 0 ? 1 : 0);
}

/**
 * The best moments, from now on, to start the waiting deck, best first; at
 * most one plan per start moment. A plan stays listed until one beat after
 * its moment so the countdown can reach "now".
 */
export function planTransitions(
  out: DeckStructure | null | undefined,
  incoming: DeckStructure | null | undefined,
  outPosition: number,
  limit = 3,
): TransitionPlan[] {
  if (!usableStructure(out) || !usableStructure(incoming)) return [];
  const outBar = barSeconds(out);
  const inBar = barSeconds(incoming);
  const outEnd = out.sections[out.sections.length - 1].end;
  const outMains = out.sections.filter((s) => intensity(s) >= MAIN);
  const lastMain = outMains[outMains.length - 1];
  const firstMainIndex = incoming.sections.findIndex(
    (s) => intensity(s) >= MAIN,
  );
  const firstMain =
    firstMainIndex >= 0 ? incoming.sections[firstMainIndex] : null;
  const firstSection = incoming.sections[0];
  const hipHop =
    out.vocabulary === "hip_hop" || incoming.vocabulary === "hip_hop";
  const earliest = outPosition - outBar / 4;
  const outStart = out.sections[0].start;
  const near = (a: number, b: number | undefined) =>
    b !== undefined && Math.abs(a - b) < outBar / 2;
  // Analysed outros often run a bar or two long, so "as it ends" allows it.
  const atEnd = (t: number) => t > outEnd - 2.5 * outBar && t < outEnd + outBar;
  const best = new Map<number, TransitionPlan>();
  for (const startAt of phraseStarts(out)) {
    if (startAt < earliest || startAt > outEnd - 4 * outBar) continue;
    const outSection = sectionAt(out, startAt + outBar / 2);
    // Start points in the new track: the sections before its first drop or
    // hook (usually the intro), and the points exactly as many bars before
    // that drop as there are to the playing track's end or its last drop.
    const cues = incoming.sections
      .slice(0, Math.max(1, firstMainIndex))
      .slice(0, 3)
      .map((s) => s.start);
    if (firstMain)
      for (const target of [lastMain?.end, outEnd]) {
        if (target === undefined || target <= startAt) continue;
        const bars = Math.round((target - startAt) / outBar);
        const cue = firstMain.start - bars * inBar;
        if (
          bars >= 4 &&
          bars <= 64 &&
          cue >= firstSection.start - inBar / 2 &&
          !cues.some((c) => Math.abs(c - cue) < inBar / 2)
        )
          cues.push(Math.max(firstSection.start, cue));
      }
    for (const cueAt of cues) {
      const toMain = firstMain ? (firstMain.start - cueAt) / inBar : null;
      const landing = toMain === null ? null : startAt + toMain * outBar;
      const togetherEnd = Math.min(outEnd, landing ?? outEnd);
      const togetherBars = Math.max(
        0,
        Math.round((togetherEnd - startAt) / outBar),
      );
      // Two full sections at once (a drop over a drop) is what to avoid.
      let clash = 0;
      const counted = Math.min(togetherBars, 64);
      for (let b = 0; b < counted; b++)
        clash += Math.min(
          intensity(sectionAt(out, startAt + (b + 0.5) * outBar)),
          intensity(sectionAt(incoming, cueAt + (b + 0.5) * inBar)),
        );
      clash = counted ? clash / counted : 0.5;
      let landingFit = 0.5;
      let landingKind: Landing | null = null;
      if (landing !== null) {
        if (atEnd(landing)) [landingFit, landingKind] = [1, "out_end"];
        else if (
          lastMain &&
          lastMain.end > startAt &&
          near(landing, lastMain.end)
        )
          [landingFit, landingKind] = [1, "swap"];
        else if (landing > outEnd)
          [landingFit, landingKind] = [
            Math.max(0, 0.4 - (landing - outEnd) / outBar / 32),
            "late",
          ];
        else if (intensity(sectionAt(out, landing + outBar / 2)) >= MAIN)
          [landingFit, landingKind] = [0, "clash"];
        else [landingFit, landingKind] = [0.5, "calm"];
      }
      const lengthFit = togetherFit(togetherBars, hipHop);
      // Mixing out usually happens late in the playing track.
      const progress = (startAt - outStart) / Math.max(1e-9, outEnd - outStart);
      const lateFit = Math.max(0, Math.min(1, (progress - 0.3) / 0.4));
      const onSection = out.sections.some((s) => near(startAt, s.start));
      const cueSection = sectionAt(incoming, cueAt + inBar / 2);
      // A start point on the new track's phrases: a section start, or a
      // multiple of 8 bars before its drop.
      const cueOnPhrase =
        incoming.sections.some((s) => Math.abs(s.start - cueAt) < inBar / 2) ||
        (toMain !== null &&
          Math.abs(toMain / 8 - Math.round(toMain / 8)) < 1e-3);
      const score =
        0.35 * landingFit +
        0.25 * (1 - clash) +
        0.2 * lengthFit +
        0.1 * lateFit +
        (onSection ? 0.05 : 0) +
        (cueOnPhrase ? 0.05 : 0);
      const kind: TransitionKind =
        landingKind === "swap"
          ? "drop_swap"
          : intensity(outSection) <= 0.3 && intensity(cueSection) <= 0.3
            ? "intro_over_outro"
            : "phrase";
      const plan: TransitionPlan = {
        startAt,
        cueAt,
        togetherBars,
        landing,
        landingLabel: firstMain?.label ?? null,
        landingKind,
        outEnd,
        bar: outBar,
        outLabel: outSection?.label ?? null,
        cueLabel: cueSection?.label ?? null,
        swapLabel: landingKind === "swap" ? (lastMain?.label ?? null) : null,
        kind,
        score,
      };
      const previous = best.get(startAt);
      if (!previous || plan.score > previous.score) best.set(startAt, plan);
    }
  }
  return [...best.values()]
    .sort((a, b) => b.score - a.score || a.startAt - b.startAt)
    .slice(0, limit);
}

const name = (label: SectionLabel | null) =>
  label ? SECTION_NAMES[label].toLowerCase() : "section";

/** "B's intro over A's outro · 16 bars together · B's drop lands as A ends" */
export function planReason(
  plan: TransitionPlan,
  outLetter: string,
  inLetter: string,
): string {
  const parts = [
    `${inLetter}'s ${name(plan.cueLabel)} over ${outLetter}'s ${name(plan.outLabel)}`,
    `${plan.togetherBars} bar${plan.togetherBars === 1 ? "" : "s"} together`,
  ];
  const main = `${inLetter}'s ${name(plan.landingLabel)}`;
  if (plan.landingKind === "out_end")
    parts.push(`${main} lands as ${outLetter} ends`);
  else if (plan.landingKind === "swap")
    parts.push(
      `${main} takes over from ${outLetter}'s ${name(plan.swapLabel)}`,
    );
  else if (plan.landingKind === "late" && plan.landing !== null) {
    const bars = Math.round((plan.landing - plan.outEnd) / plan.bar);
    parts.push(
      `${main} comes ${bars} bar${bars === 1 ? "" : "s"} after ${outLetter} ends`,
    );
  } else if (plan.landingKind === "clash")
    parts.push(`${main} lands on ${outLetter}'s drop (clash)`);
  else if (plan.landingKind === "calm") parts.push(`${main} lands on a phrase`);
  return parts.join(" · ");
}

/** The countdown beside the playing deck's playhead: "START B in 8" (bars),
 * then "START B in 3 beats", then "START B NOW"; null once it has passed. */
export function startCountdown(
  plan: TransitionPlan,
  out: DeckStructure,
  outPosition: number,
  inLetter: string,
): string | null {
  const beats = ((plan.startAt - outPosition) * out.bpm) / 60;
  if (beats < -1) return null;
  if (beats < 0.5) return `START ${inLetter} NOW`;
  if (beats < 3.5) {
    const n = Math.round(beats);
    return `START ${inLetter} in ${n} beat${n === 1 ? "" : "s"}`;
  }
  return `START ${inLetter} in ${Math.ceil(beats / 4 - 1e-6)}`;
}

export interface LiveAlignment {
  /** Playing-deck seconds where the new track's next drop or hook lands. */
  landing: number | null;
  landingLabel: SectionLabel | null;
  /** Where the new track's next phrase starts against the playing track's
   * nearest phrase start, in bars; 0 means on the phrase. */
  phraseOffsetBars: number | null;
}

/** With both decks playing: where the new track's sections will fall. */
export function liveAlignment(
  out: DeckStructure | null | undefined,
  incoming: DeckStructure | null | undefined,
  outPosition: number,
  inPosition: number,
): LiveAlignment | null {
  if (!usableStructure(out) || !usableStructure(incoming)) return null;
  const outBar = barSeconds(out);
  const inBar = barSeconds(incoming);
  const project = (inSeconds: number) =>
    outPosition + ((inSeconds - inPosition) / inBar) * outBar;
  const main = incoming.sections.find(
    (s) => s.start > inPosition + 1e-3 && intensity(s) >= MAIN,
  );
  // The next 8-bar phrase, so long sections still have something to check.
  const next = phraseStarts(incoming).find((t) => t > inPosition + 1e-3);
  let phraseOffsetBars: number | null = null;
  if (next !== undefined) {
    const projected = project(next);
    // The playing track's end is a phrase boundary too.
    const phrases = [
      ...phraseStarts(out),
      out.sections[out.sections.length - 1].end,
    ];
    if (phrases.length) {
      const nearest = phrases.reduce((a, b) =>
        Math.abs(b - projected) < Math.abs(a - projected) ? b : a,
      );
      phraseOffsetBars = Math.round(((projected - nearest) / outBar) * 4) / 4;
    }
  }
  return {
    landing: main ? project(main.start) : null,
    landingLabel: main?.label ?? null,
    phraseOffsetBars,
  };
}

/** "B's drop lands as A ends", or how far B sits from A's phrase. */
export function liveReason(
  live: LiveAlignment,
  out: DeckStructure,
  outLetter: string,
  inLetter: string,
): string {
  const parts: string[] = [];
  if (live.phraseOffsetBars !== null)
    parts.push(
      live.phraseOffsetBars === 0
        ? `${inLetter} is on ${outLetter}'s phrase`
        : `${inLetter} is ${Math.abs(live.phraseOffsetBars)} bar${Math.abs(live.phraseOffsetBars) === 1 ? "" : "s"} ${live.phraseOffsetBars > 0 ? "behind" : "ahead of"} ${outLetter}'s phrase`,
    );
  if (live.landing !== null) {
    const outEnd = out.sections[out.sections.length - 1].end;
    const bar = barSeconds(out);
    const main = `${inLetter}'s ${name(live.landingLabel)}`;
    parts.push(
      Math.abs(live.landing - outEnd) < bar / 2
        ? `${main} lands as ${outLetter} ends`
        : live.landing > outEnd
          ? `${main} comes after ${outLetter} ends`
          : intensity(sectionAt(out, live.landing + bar / 2)) >= MAIN
            ? `${main} lands on ${outLetter}'s ${name(sectionAt(out, live.landing + bar / 2)?.label ?? null)} (clash)`
            : `${main} lands at ${outLetter} bar ${barNumber(out, live.landing)}`,
    );
  }
  return parts.join(" · ");
}

/** Which deck is going out (the one the crowd hears) and which comes in. */
export function transitionRoles(
  engine: EngineSnapshot,
): { out: 0 | 1; incoming: 0 | 1; live: boolean } | null {
  const decks = engine.decks;
  if (!decks?.[0]?.track || !decks?.[1]?.track) return null;
  const position = Math.max(-1, Math.min(1, engine.crossfader ?? 0));
  const angle = ((position + 1) * Math.PI) / 4;
  const fade = [Math.cos(angle), Math.sin(angle)];
  const level = [0, 1].map((d) => {
    const fader = decks[d].channel_fader;
    return (
      fade[d] *
      (typeof fader === "number" && Number.isFinite(fader)
        ? Math.max(0, Math.min(1, fader))
        : 1)
    );
  });
  const audible = [0, 1].filter((d) => decks[d].playing && level[d] > 0.05);
  if (!audible.length) return null;
  const out = (
    audible.length === 1
      ? audible[0]
      : Math.abs(level[0] - level[1]) <= 0.1
        ? decks[1].is_master
          ? 1
          : 0
        : level[0] >= level[1]
          ? 0
          : 1
  ) as 0 | 1;
  const incoming = (1 - out) as 0 | 1;
  return { out, incoming, live: !!decks[incoming].playing };
}

export interface WaveformGuide {
  kind: "start" | "landing" | "cue";
  /** Seconds on the deck the guide is drawn on. */
  at: number;
  /** End of the shaded "together" span, for a start guide. */
  until?: number;
  label: string;
  color: string;
}

/** Guides for both decks: the start moment, the together span and the
 * landing on the playing deck; the start point on the waiting deck. */
export function planGuides(
  plan: TransitionPlan,
  inLetter: string,
  inColor: string,
): { out: WaveformGuide[]; incoming: WaveformGuide[] } {
  const out: WaveformGuide[] = [
    {
      kind: "start",
      at: plan.startAt,
      until: Math.min(plan.outEnd, plan.landing ?? plan.outEnd),
      label: `START ${inLetter}`,
      color: inColor,
    },
  ];
  if (plan.landing !== null && plan.landingLabel)
    out.push({
      kind: "landing",
      at: plan.landing,
      label: `${inLetter} ${SECTION_NAMES[plan.landingLabel].toUpperCase()}`,
      color: inColor,
    });
  return {
    out,
    incoming: [{ kind: "cue", at: plan.cueAt, label: "START", color: inColor }],
  };
}

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  barNumber,
  liveAlignment,
  liveReason,
  planGuides,
  planReason,
  planTransitions,
  startCountdown,
  togetherFit,
  transitionRoles,
  type DeckStructure,
} from "./transition.ts";
import type { EngineSnapshot, SectionLabel, TrackRecord } from "./types";

/** A track laid out as [label, bars] sections from beat 1 at `offset`. */
function deck(
  bpm: number,
  layout: [SectionLabel | null, number][],
  offset = 0,
  vocabulary: "dance" | "hip_hop" = "dance",
): DeckStructure {
  const bar = 240 / bpm;
  let start = offset;
  const sections = layout.map(([label, bars]) => {
    const section = { start, end: start + bars * bar, bars, label };
    start = section.end;
    return section;
  });
  return { bpm, offset, duration: start + 1, sections, vocabulary };
}
const bars = (n: number, bpm = 128) => (n * 240) / bpm;
const close = (a: number | null, b: number) =>
  assert.ok(a !== null && Math.abs(a - b) < 1e-6, `${a} ≠ ${b}`);

// Intro 16, drop 32, breakdown 16, drop 32, outro 16: ends at bar 112.
const playing = deck(128, [
  ["intro", 16],
  ["drop", 32],
  ["breakdown", 16],
  ["drop", 32],
  ["outro", 16],
]);
const waiting = deck(128, [
  ["intro", 16],
  ["drop", 32],
  ["outro", 16],
]);

test("the best start puts the new intro over the outro so its drop lands as the old track ends", () => {
  const [best, second] = planTransitions(playing, waiting, 0);
  close(best.startAt, bars(96));
  close(best.cueAt, 0);
  close(best.landing, bars(112));
  assert.equal(best.togetherBars, 16);
  assert.equal(best.kind, "intro_over_outro");
  assert.equal(
    planReason(best, "A", "B"),
    "B's intro over A's outro · 16 bars together · B's drop lands as A ends",
  );
  // The runner-up swaps drops: B's drop starts as A's last drop ends.
  close(second.startAt, bars(80));
  close(second.landing, bars(96));
  assert.equal(second.kind, "drop_swap");
  assert.match(
    planReason(second, "A", "B"),
    /B's drop takes over from A's drop/,
  );
});

test("plans never land a drop on a drop and stay on the playing track's phrases", () => {
  const plans = planTransitions(playing, waiting, 0, 10);
  assert.ok(plans.length >= 3);
  for (const plan of plans.slice(0, 3)) {
    assert.notEqual(plan.landingKind, "clash");
    const phrase = (plan.startAt / bars(1)) % 8;
    assert.ok(phrase < 1e-6 || 8 - phrase < 1e-6, `${plan.startAt}`);
  }
  assert.deepEqual(
    plans.map((p) => p.startAt),
    [...new Set(plans.map((p) => p.startAt))],
  );
});

test("the new track can start part-way into its intro so its drop lands exactly", () => {
  const irregular = deck(128, [
    ["intro", 12],
    ["breakdown", 20],
    ["drop", 32],
  ]);
  const [best] = planTransitions(playing, irregular, bars(60));
  close(best.startAt, bars(96));
  close(best.cueAt, bars(16));
  assert.equal(best.landingKind, "out_end");
  assert.equal(best.togetherBars, 16);
  assert.equal(
    planReason(best, "A", "B"),
    "B's breakdown over A's outro · 16 bars together · B's drop lands as A ends",
  );
});

test("an outro a bar longer than the new intro still lands at the end", () => {
  const longOutro = deck(128, [
    ["intro", 16],
    ["drop", 32],
    ["outro", 17],
  ]);
  const [best] = planTransitions(longOutro, waiting, 0);
  close(best.startAt, bars(48));
  assert.equal(best.landingKind, "out_end");
});

test("bars line up across different source tempos", () => {
  const slower = deck(124, [
    ["intro", 16],
    ["drop", 32],
    ["outro", 16],
  ]);
  const [best] = planTransitions(slower, waiting, 0);
  close(best.startAt, bars(48, 124));
  close(best.landing, bars(64, 124));
  close(best.cueAt, 0);
});

test("only moments still ahead are offered, until a beat after each", () => {
  const beat = 60 / 128;
  const atOutro = planTransitions(playing, waiting, bars(96) + 0.5 * beat);
  assert.ok(atOutro.some((p) => Math.abs(p.startAt - bars(96)) < 1e-6));
  const later = planTransitions(playing, waiting, bars(96) + 2 * beat);
  assert.ok(later.every((p) => p.startAt > bars(96)));
  assert.deepEqual(planTransitions(playing, waiting, bars(111)), []);
});

test("hip-hop prefers shorter overlaps than dance", () => {
  assert.equal(togetherFit(8, true), 1);
  assert.ok(togetherFit(8) < togetherFit(16));
  assert.equal(togetherFit(32), 1);
  assert.ok(togetherFit(32, true) < togetherFit(16, true));
  assert.ok(togetherFit(0) === 0 && togetherFit(128) >= 0.2);
});

test("without sections there is nothing to plan", () => {
  const bare = { ...waiting, sections: [] };
  assert.deepEqual(planTransitions(playing, bare, 0), []);
  assert.deepEqual(planTransitions(null, waiting, 0), []);
  assert.equal(liveAlignment(playing, bare, 0, 0), null);
});

test("the countdown runs in bars like the drop countdown, then beats, then says now", () => {
  const [best] = planTransitions(playing, waiting, 0);
  const beat = 60 / 128;
  assert.equal(
    startCountdown(best, playing, best.startAt - bars(8), "B"),
    "START B in 8",
  );
  assert.equal(
    startCountdown(best, playing, best.startAt - bars(7.5), "B"),
    "START B in 8",
  );
  assert.equal(
    startCountdown(best, playing, best.startAt - 2 * beat, "B"),
    "START B in 2 beats",
  );
  assert.equal(
    startCountdown(best, playing, best.startAt - 0.2 * beat, "B"),
    "START B NOW",
  );
  assert.equal(
    startCountdown(best, playing, best.startAt + 0.5 * beat, "B"),
    "START B NOW",
  );
  assert.equal(
    startCountdown(best, playing, best.startAt + 2 * beat, "B"),
    null,
  );
});

test("guides mark the start, the together span and the landing", () => {
  const [best] = planTransitions(playing, waiting, 0);
  const guides = planGuides(best, "B", "#86cfee");
  assert.deepEqual(
    guides.out.map((g) => [g.kind, g.label]),
    [
      ["start", "START B"],
      ["landing", "B DROP"],
    ],
  );
  close(guides.out[0].until ?? null, bars(112));
  assert.deepEqual(guides.incoming, [
    { kind: "cue", at: 0, label: "START", color: "#86cfee" },
  ]);
});

test("once both play, the guide checks the new track against the old one's phrases", () => {
  // A is 4 bars into its outro; B started on time, 4 bars into its intro.
  const onTime = liveAlignment(playing, waiting, bars(100), bars(4))!;
  assert.equal(onTime.phraseOffsetBars, 0);
  close(onTime.landing, bars(112));
  assert.equal(
    liveReason(onTime, playing, "A", "B"),
    "B is on A's phrase · B's drop lands as A ends",
  );
  // Started two bars late: everything in B arrives two bars after A's phrase.
  const late = liveAlignment(playing, waiting, bars(100), bars(2))!;
  assert.equal(late.phraseOffsetBars, 2);
  assert.equal(
    liveReason(late, playing, "A", "B"),
    "B is 2 bars behind A's phrase · B's drop comes after A ends",
  );
});

test("a long section still gets a phrase check from its 8-bar phrases", () => {
  const loop = deck(128, [
    ["intro", 8],
    ["verse", 64],
  ]);
  // B is 21 bars in, inside its 64-bar verse; its next phrase is bar 24.
  const live = liveAlignment(playing, loop, bars(100), bars(21))!;
  assert.equal(live.phraseOffsetBars, -1);
  assert.equal(live.landing, null);
  assert.equal(
    liveReason(live, playing, "A", "B"),
    "B is 1 bar ahead of A's phrase",
  );
});

test("bar numbers match the waveform's bar counter", () => {
  const offset = deck(120, [["intro", 4]], 0.1);
  assert.equal(barNumber(offset, 0.1), 1);
  assert.equal(barNumber(offset, 2.1), 2);
  assert.equal(barNumber(offset, 2.09), 1);
});

const track = (path: string): TrackRecord => ({ id: path, path, title: path });
function engine(change: (e: EngineSnapshot) => void = () => {}) {
  const e: EngineSnapshot = {
    crossfader: 0,
    decks: [
      { track: track("/a"), playing: true, channel_fader: 1 },
      { track: track("/b"), playing: false, channel_fader: 1 },
    ],
  };
  change(e);
  return e;
}

test("the playing deck the crowd hears goes out; the other comes in", () => {
  assert.deepEqual(transitionRoles(engine()), {
    out: 0,
    incoming: 1,
    live: false,
  });
  assert.equal(transitionRoles(engine((e) => (e.decks[1].track = null))), null);
  assert.equal(
    transitionRoles(engine((e) => (e.decks[0].playing = false))),
    null,
  );
  // Both playing: the louder deck goes out, the master on a tie.
  assert.deepEqual(
    transitionRoles(
      engine((e) => {
        e.decks[1].playing = true;
        e.decks[0].channel_fader = 0.3;
      }),
    ),
    { out: 1, incoming: 0, live: true },
  );
  assert.equal(
    transitionRoles(
      engine((e) => {
        e.decks[1].playing = true;
        e.decks[1].is_master = true;
      }),
    )?.out,
    1,
  );
  // Behind a closed crossfader the playing deck is not heard.
  assert.equal(transitionRoles(engine((e) => (e.crossfader = 1))), null);
});


test("variable tempo plans and countdowns follow native bar events", () => {
  const source = deck(120, [["intro", 16], ["drop", 80], ["outro", 16]]);
  const times = [0.173];
  for (let i = 0; i < 112; i++) times.push(times[i] + (i < 80 ? 2 : 2.8));
  const remapped: DeckStructure = {
    ...source, offset: times[0], barTimes: times, beatsPerBar: 4,
    duration: times[112] + 1,
    sections: source.sections.map(s => ({...s, start: times[Math.round(s.start / 2)], end: times[Math.round(s.end / 2)]})),
  };
  const incoming = deck(120, [["intro", 16], ["drop", 32], ["outro", 16]]);
  const [plan] = planTransitions(remapped, incoming, times[80]);
  close(plan.startAt, times[96]);
  close(plan.landing, times[112]);
  assert.equal(plan.togetherBars, 16);
  assert.equal(startCountdown(plan, remapped, times[88], "B"), "START B in 8");
  close(liveAlignment(remapped, incoming, times[96], 0)?.landing ?? null, times[112]);
});

test("mix guide declines incompatible meters instead of equating unequal bars", () => {
  const a = {...playing, beatsPerBar: 3};
  const b = {...waiting, beatsPerBar: 4};
  assert.deepEqual(planTransitions(a, b, 0), []);
  assert.equal(liveAlignment(a, b, 0, 0), null);
});

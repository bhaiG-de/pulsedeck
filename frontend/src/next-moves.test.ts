import assert from "node:assert/strict";
import test from "node:test";
import {
  moveCountdown,
  mergeMoveFeedback,
  type MixOperation,
  sameMove,
  sourceTime,
  TECHNIQUE_NAMES,
  type NextMove,
} from "./next-moves.ts";
test("countdown follows source position and rendered rate and expires after launch", () => {
  const move = { launch_seconds: 180 } as NextMove;
  assert.equal(moveCountdown(move, 60, 1), 120);
  assert.equal(moveCountdown(move, 60, 1.02), 120 / 1.02);
  assert.equal(moveCountdown(move, 181, 1), -1);
});
test("prepared choice stays bound through target load revisions and rejects other techniques", () => {
  const a = {
    id: "old",
    source: "/a.wav",
    technique: "eq_blend",
    reference_deck: 0,
    target_deck: 1,
    entry_seconds: 0,
    launch_seconds: 180,
    clear_seconds: 212,
  } as NextMove;
  assert(sameMove(a, { ...a, id: "new-after-preparation" }));
  assert(!sameMove(a, { ...a, technique: "filter_blend" }));
  assert(!sameMove(a, { ...a, launch_seconds: 196 }));
});
test("timelines format exact source coordinates and identify distinct recipes", () => {
  assert.equal(sourceTime(64.5), "1:04.5");
  assert.equal(sourceTime(0), "0:00.0");
  assert.equal(
    new Set([
      TECHNIQUE_NAMES.eq_blend,
      TECHNIQUE_NAMES.filter_blend,
      TECHNIQUE_NAMES.short_exchange,
    ]).size,
    3,
  );
});

test("event updates retain option evidence and delayed status cannot undo completion", () => {
  const option = { source: "/actual-source.mp3" } as NextMove;
  const current: MixOperation = {
    prepared_id: "move",
    operation_id: "move",
    state: "armed",
    event_sequence: 4,
    option,
  };
  const complete = mergeMoveFeedback(current, {
    operation_id: "move",
    state: "completed",
    terminal: true,
    event_sequence: 8,
  });
  assert.equal(complete.option, option);
  assert.equal(
    mergeMoveFeedback(complete, { ...current, event_sequence: 7 }),
    complete,
  );
  assert.equal(
    mergeMoveFeedback(complete, {
      operation_id: "other",
      state: "failed",
      event_sequence: 9,
    }),
    complete,
  );
});

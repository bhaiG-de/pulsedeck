import assert from "node:assert/strict";
import test from "node:test";
import {
  mergeQueue,
  queueStartArgs,
  reorderQueue,
  type SuccessorQueue,
} from "./successor-queue.ts";
const settings = {
  mode: "collection" as const,
  collection: "liked",
  sources: [],
  direction: "keep",
  technique: "all",
  minutes: "",
  stretch: "4",
  replace: false,
};
test("Neural Mix uses the explicit collection and continues until stopped by default", () => {
  assert.deepEqual(queueStartArgs(settings), {
    playlist: "spotify:liked",
    provider: "spotify",
    energy_direction: "keep",
    max_stretch_percent: 4,
    replace_prepared: false,
    max_skips: 0,
  });
  assert.deepEqual(
    queueStartArgs({
      ...settings,
      collection: "local",
      minutes: "60",
      direction: "lower",
      technique: "eq_blend",
    }),
    {
      provider: "local",
      duration_seconds: 3600,
      energy_direction: "lower",
      techniques: ["eq_blend"],
      max_stretch_percent: 4,
      replace_prepared: false,
      max_skips: 0,
    },
  );
  const ordered = queueStartArgs({
    ...settings,
    mode: "ordered",
    sources: ["/a.wav", "/b.wav"],
  });
  assert.deepEqual(ordered.sources, ["/a.wav", "/b.wav"]);
  assert.equal(ordered.playlist, undefined);
});
test("Neural Mix rejects invalid settings before starting playback automation", () => {
  for (const minutes of ["NaN", "Infinity", "0", "-1", "1441"])
    assert.throws(() => queueStartArgs({ ...settings, minutes }), /duration/);
  for (const stretch of ["", "NaN", "Infinity", "-1", "101"])
    assert.throws(() => queueStartArgs({ ...settings, stretch }), /tempo/);
  assert.throws(
    () => queueStartArgs({ ...settings, mode: "ordered" }),
    /Add tracks/,
  );
});
test("late initial status and old queue events cannot replace newer state", () => {
  const current = {
    queue_id: "new",
    revision: 5,
    started_unix_ms: 20,
    items: [],
    state: "running",
    terminal: false,
  } as SuccessorQueue;
  assert.equal(mergeQueue(current, null), current);
  assert.equal(mergeQueue(current, { ...current, revision: 4 }), current);
  assert.equal(
    mergeQueue(current, { ...current, queue_id: "old", started_unix_ms: 10 }),
    current,
  );
  assert.equal(
    mergeQueue(current, {
      ...current,
      revision: 6,
      state: "stopped",
      terminal: true,
    })?.state,
    "stopped",
  );
});
test("queue order edits move only the requested pending source", () => {
  const original = ["first", "second", "third"];
  assert.deepEqual(reorderQueue(original, 1, -1), ["second", "first", "third"]);
  assert.deepEqual(original, ["first", "second", "third"]);
  assert.equal(reorderQueue(original, 0, -1), original);
});

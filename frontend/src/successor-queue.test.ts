import assert from "node:assert/strict";
import test from "node:test";
import { mergeQueue, reorderQueue, type SuccessorQueue } from "./successor-queue.ts";
test("late initial status and old queue events cannot replace newer state", () => {
  const current = { queue_id: "new", revision: 5, started_unix_ms: 20, items: [], state: "running", terminal: false } as SuccessorQueue;
  assert.equal(mergeQueue(current, null), current);
  assert.equal(mergeQueue(current, { ...current, revision: 4 }), current);
  assert.equal(mergeQueue(current, { ...current, queue_id: "old", started_unix_ms: 10 }), current);
  assert.equal(mergeQueue(current, { ...current, revision: 6, state: "stopped", terminal: true })?.state, "stopped");
});
test("queue order edits move only the requested pending source", () => {
  const original = ["first", "second", "third"];
  assert.deepEqual(reorderQueue(original, 1, -1), ["second", "first", "third"]);
  assert.deepEqual(original, ["first", "second", "third"]);
  assert.equal(reorderQueue(original, 0, -1), original);
});

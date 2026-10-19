import { test } from "node:test";
import assert from "node:assert/strict";
import { LatestControlWriter, waitForUiCommit } from "./control-state.ts";

const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

test("slow control writes cannot overwrite the latest gesture or flood the backend", async () => {
  const sent: number[] = [];
  let finish!: () => void;
  let settled = 0;
  const writer = new LatestControlWriter(
    async (value) => {
      sent.push(value);
      await new Promise<void>((resolve) => {
        finish = resolve;
      });
    },
    () => settled++,
  );
  writer.set(-2);
  writer.set(1);
  writer.set(3);
  assert.deepEqual(sent, [-2]);
  finish();
  await tick();
  assert.deepEqual(sent, [-2, 3]);
  assert.equal(settled, 0);
  finish();
  await tick();
  assert.equal(settled, 1);
});

test("source replacement discards unsent gesture values and stale callbacks", async () => {
  const sent: number[] = [];
  let finish!: () => void;
  let settled = 0;
  const writer = new LatestControlWriter(
    async (value) => {
      sent.push(value);
      await new Promise<void>((resolve) => {
        finish = resolve;
      });
    },
    () => settled++,
  );
  writer.set(1);
  writer.set(5);
  writer.dispose();
  finish();
  await tick();
  writer.set(10);
  assert.deepEqual(sent, [1]);
  assert.equal(settled, 0);
});

test("a rejected intermediate write does not lose the final requested tempo", async () => {
  let reject!: (error: unknown) => void;
  const sent: number[] = [],
    results: unknown[] = [];
  const writer = new LatestControlWriter(
    async (value) => {
      sent.push(value);
      if (sent.length === 1)
        await new Promise<void>((_, fail) => {
          reject = fail;
        });
    },
    (error) => results.push(error),
  );
  writer.set(-1);
  writer.set(2);
  reject(new Error("old source"));
  await tick();
  assert.deepEqual(sent, [-1, 2]);
  assert.deepEqual(results, [undefined]);
});

test("hidden WebViews acknowledge UI commits when display frames are suspended", { timeout: 1000 }, async () => {
  const request = globalThis.requestAnimationFrame;
  const cancel = globalThis.cancelAnimationFrame;
  let cancelled = 0;
  globalThis.requestAnimationFrame = () => 123;
  globalThis.cancelAnimationFrame = (id) => { cancelled = id; };
  try {
    await waitForUiCommit();
    assert.equal(cancelled, 123);
  } finally {
    globalThis.requestAnimationFrame = request;
    globalThis.cancelAnimationFrame = cancel;
  }
});

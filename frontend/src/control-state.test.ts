import test from "node:test";
import assert from "node:assert/strict";
import {
  ControlRegistry,
  controlNumber,
  controlBoolean,
  controlEnum,
  controlDraft,
  controlInteger,
  ControlInputError,
} from "./control-state.ts";
test("UI registry invokes real handlers, observes state, and distinguishes unmounted targets", async () => {
  const registry = new ControlRegistry();
  let theme = "hardware-dark";
  registry.register(
    "app",
    {
      theme: (v) => {
        theme = controlEnum(v, ["hardware-dark", "anatomy-light"]);
      },
    },
    { theme },
  );
  await registry.invoke("app", "theme", "anatomy-light");
  assert.equal(theme, "anatomy-light");
  registry.register("app", { theme: () => {} }, { theme });
  assert.deepEqual(registry.snapshot("app"), {
    commands: ["theme"],
    state: { theme },
  });
  await assert.rejects(registry.invoke("app", "unknown", null), /Unsupported/);
  registry.remove("app");
  await assert.rejects(
    registry.invoke("app", "theme", "hardware-dark"),
    /unavailable/,
  );
});
test("UI values reject invalid types and nonfinite or out-of-range numbers", () => {
  for (const value of [NaN, Infinity, "32", 129, -1])
    assert.throws(() => controlNumber(value, 8, 128));
  assert.equal(controlNumber(32, 8, 128), 32);
  assert.throws(() => controlBoolean("false"));
  assert.equal(controlBoolean(false), false);
  assert.throws(() => controlEnum("wrong", ["local", "spotify"]));
});
test("draft validation rejects unknown fields and gives structured repair details before mutation", () => {
  let applied = false;
  try {
    controlDraft({ buffer: 256, channel: 1.5 }, {
      buffer: (v) => controlInteger(v, 16, 8192),
      channel: (v) => controlInteger(v, 0, 15),
    });
    applied = true;
  } catch (error) {
    assert.ok(error instanceof ControlInputError);
    assert.equal(error.code, "invalid_arguments");
    assert.deepEqual(error.details.expected, { type: "integer", minimum: 0, maximum: 15 });
    assert.ok(error.details.hint);
  }
  assert.equal(applied, false);
  assert.throws(() => controlDraft({ chanel: 1 }, { channel: (v) => v }),
    (error: unknown) => error instanceof ControlInputError && error.details.argument === "chanel");
});

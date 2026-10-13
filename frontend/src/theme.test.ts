import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_THEME,
  readTheme,
  saveTheme,
  THEME_STORAGE_KEY,
} from "./theme.ts";
test("theme preference restores across application starts and rejects stale values", () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
  };
  assert.equal(readTheme(storage), DEFAULT_THEME);
  saveTheme("anatomy-light", storage);
  assert.equal(readTheme(storage), "anatomy-light");
  saveTheme("anatomy-dark", storage);
  assert.equal(readTheme(storage), "anatomy-dark");
  values.set(THEME_STORAGE_KEY, "removed-theme");
  assert.equal(readTheme(storage), DEFAULT_THEME);
});
test("denied preference storage does not prevent changing the live appearance", () => {
  const storage = {
    getItem: (): string => {
      throw new Error("Denied");
    },
    setItem: () => {
      throw new Error("Denied");
    },
  };
  assert.equal(readTheme(storage), DEFAULT_THEME);
  assert.doesNotThrow(() => saveTheme("anatomy-light", storage));
});

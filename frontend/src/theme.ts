export const THEMES = [
  {
    id: "hardware-dark",
    name: "Hardware Dark",
    description: "Charcoal panels, tactile keys and orange controls.",
  },
  {
    id: "anatomy-light",
    name: "Anatomy Light",
    description: "Ivory surfaces, engraved lines and machined details.",
  },
  {
    id: "anatomy-dark",
    name: "Anatomy Dark",
    description: "Graphite surfaces with precise, silver linework.",
  },
] as const;
export type ThemeId = (typeof THEMES)[number]["id"];
export const DEFAULT_THEME: ThemeId = "hardware-dark";
export const THEME_STORAGE_KEY = "pulsedeck.theme";
type ThemeStorage = Pick<Storage, "getItem" | "setItem">;
export function isThemeId(value: unknown): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}
function browserStorage(): ThemeStorage | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage;
  } catch {
    return undefined;
  }
}
export function readTheme(storage = browserStorage()): ThemeId {
  try {
    const saved = storage?.getItem(THEME_STORAGE_KEY);
    return isThemeId(saved) ? saved : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}
export function saveTheme(theme: ThemeId, storage = browserStorage()): void {
  try {
    storage?.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    /* Appearance still applies when local storage is unavailable. */
  }
}
export function applyTheme(theme: ThemeId): void {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme =
    theme === "anatomy-light" ? "light" : "dark";
  const color = getComputedStyle(document.documentElement)
    .getPropertyValue("--color-app-bg")
    .trim();
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute("content", color);
}

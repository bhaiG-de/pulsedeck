export const THEMES = [
  {
    id: "aluminium",
    name: "Aluminium",
    description:
      "Bead-blasted grey chassis, black screens and coloured encoders.",
    scheme: "light",
  },
  {
    id: "ivory",
    name: "Ivory",
    description:
      "Warm off-white housing with graphite type and a single orange dot.",
    scheme: "light",
  },
  {
    id: "graphite",
    name: "Graphite",
    description:
      "Anodised black body for dark booths. Same screens, same colours.",
    scheme: "dark",
  },
] as const;
export type ThemeId = (typeof THEMES)[number]["id"];
export const DEFAULT_THEME: ThemeId = "aluminium";
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
    THEMES.find((option) => option.id === theme)?.scheme ?? "light";
  const color = getComputedStyle(document.documentElement)
    .getPropertyValue("--color-app-bg")
    .trim();
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute("content", color);
}

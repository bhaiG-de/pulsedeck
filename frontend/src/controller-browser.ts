/** UI actions emitted by the native FLX4 parser, using its serde names. */
export type ControllerBrowserAction =
  | { type: "browse"; delta: number; shifted: boolean }
  | { type: "load"; deck: number; shifted: boolean; instant_double?: boolean }
  | {
      type: "global_button";
      button: "browse_press" | "browse_back" | "panel_forward";
      pressed: boolean;
      shifted: boolean;
    };
export interface ControllerBrowserHandle {
  handle: (action: ControllerBrowserAction) => void | Promise<void>;
}
export type BrowserPane = "playlists" | "tracks";
export type BrowserLayer = "home" | "footer" | "playlists" | "tracks" | "close";
export interface BrowserNavigation {
  layer: BrowserLayer;
  returning: boolean;
}
export type BrowserNavigationEvent =
  "rotate" | "press" | "back" | "open" | "close" | BrowserPane;
/** Press walks into songs, then walks back through playlists and the close control. */
export function navigateBrowser(
  state: BrowserNavigation,
  event: BrowserNavigationEvent,
): BrowserNavigation {
  if (event === "open" || event === "playlists")
    return { layer: "playlists", returning: false };
  if (event === "tracks") return { layer: "tracks", returning: false };
  if (event === "close") return { layer: "footer", returning: false };
  if (event === "rotate") {
    if (state.layer === "home") return { layer: "footer", returning: false };
    if (state.layer === "close" || state.layer === "playlists")
      return { layer: "playlists", returning: false };
    return state;
  }
  if (event === "back") {
    if (state.layer === "tracks")
      return { layer: "playlists", returning: true };
    if (state.layer === "playlists") return { layer: "close", returning: true };
    return { layer: "footer", returning: false };
  }
  if (state.layer === "home" || state.layer === "footer")
    return { layer: "playlists", returning: false };
  if (state.layer === "tracks") return { layer: "playlists", returning: true };
  if (state.layer === "playlists")
    return {
      layer: state.returning ? "close" : "tracks",
      returning: state.returning,
    };
  return { layer: "footer", returning: false };
}
export function browserPane(
  pane: BrowserPane,
  action: "press" | "back",
): BrowserPane {
  return action === "back" || pane === "tracks" ? "playlists" : "tracks";
}
export function isBrowserAction(
  value: unknown,
): value is ControllerBrowserAction {
  if (!value || typeof value !== "object") return false;
  const a = value as Record<string, unknown>;
  if (a.shifted !== undefined && typeof a.shifted !== "boolean") return false;
  if (a.pressed !== undefined && typeof a.pressed !== "boolean") return false;
  return (
    (a.type === "browse" &&
      typeof a.delta === "number" &&
      Number.isFinite(a.delta) &&
      Number.isInteger(a.delta)) ||
    (a.type === "load" && (a.deck === 0 || a.deck === 1)) ||
    (a.type === "global_button" &&
      (a.button === "browse_press" ||
        a.button === "browse_back" ||
        a.button === "panel_forward"))
  );
}
/** Keep the current row anchored to its identity when lists reorder/filter. */
export function browseIndex<T>(
  items: readonly T[],
  selected: T | null,
  delta: number,
): number {
  if (!items.length) return -1;
  const index = selected === null ? -1 : items.indexOf(selected);
  const step = Math.trunc(Number.isFinite(delta) ? delta : 0);
  const start = index < 0 ? (step > 0 ? -1 : 0) : index;
  return Math.max(0, Math.min(items.length - 1, start + step));
}
export function zoomBeats(current: number, delta: number): number {
  // A shifted browse detent changes one zoom level, regardless of event batching.
  return Math.max(
    8,
    Math.min(128, current * 2 ** Math.max(-4, Math.min(4, -Math.trunc(delta)))),
  );
}
export function scrollBrowserSelection(element: Element | null) {
  requestAnimationFrame(() =>
    element?.scrollIntoView({ block: "nearest", inline: "nearest" }),
  );
}

export interface LoadGesture {
  deck: number;
  time: number;
}
export function isInstantDouble(
  previous: LoadGesture | null,
  deck: number,
  time: number,
): boolean {
  return (
    previous?.deck === deck &&
    time >= previous.time &&
    time - previous.time < 500
  );
}

export interface ControllerSurface {
  tempo_ranges: [number, number];
  pad_modes: [string, string];
  fx_name: [string, string];
  fx_enabled: [boolean, boolean];
  fx_beats: [number, number];
  fx_depth: number;
  fx_target: number;
  smart_cfx_preset: string;
}
export function controllerLabel(name: string): string {
  const labels: Record<string, string> = {
    hot_cue: "HOT CUE",
    pad_fx1: "PAD FX 1",
    pad_fx2: "PAD FX 2",
    beat_jump: "BEAT JUMP",
    beat_loop: "BEAT LOOP",
    key_shift: "KEY SHIFT",
  };
  return labels[name] ?? name.replaceAll("_", " ").toUpperCase();
}

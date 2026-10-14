import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowLeftRight, Music2, RotateCcw, SkipForward } from "lucide-react";
import { isDesktop, native } from "./api";
import type { EngineSnapshot } from "./types";
import SpotifyLogo from "./SpotifyLogo";
import {
  browseIndex,
  scrollBrowserSelection,
  type BrowserLayer,
  type BrowserPane,
  type ControllerBrowserAction,
  type ControllerBrowserHandle,
} from "./controller-browser";
import {
  ENERGY_DIRECTIONS,
  ENERGY_LABELS,
  dragPayload,
  emptyMessage,
  listCounts,
  referenceSummary,
  stepDirection,
  suggestionRequestKey,
  type EnergyDirection,
  type Suggestion,
  type SuggestionList,
} from "./suggestions";
import "./styles/next.css";

/**
 * "What should I play next?": tracks ranked against the deck the crowd hears,
 * for loading into the other deck. The left pane chooses keep, raise or lower
 * energy; the browse knob steers both panes like the playlist browser.
 */
export default function NextPanel({
  engine,
  active,
  libraryRevision,
  controllerLayer,
  registerControllerBrowser,
  onBrowserPaneChange,
  onLoad,
  onError,
}: {
  engine: EngineSnapshot;
  /** Fetch only while the drawer shows this view. */
  active: boolean;
  /** Changes when the library changes, so the list refreshes. */
  libraryRevision: unknown;
  controllerLayer: BrowserLayer;
  registerControllerBrowser: (handle: ControllerBrowserHandle | null) => void;
  onBrowserPaneChange: (pane: BrowserPane) => void;
  onLoad: (deck: number, suggestion: Suggestion) => void | Promise<void>;
  onError: (error: unknown) => void;
}) {
  const [direction, updateDirection] = useState<EnergyDirection>("keep");
  const directionRef = useRef(direction);
  const setDirection = useCallback((value: EnergyDirection) => {
    directionRef.current = value;
    updateDirection(value);
  }, []);
  const [list, updateList] = useState<SuggestionList | null>(null);
  const listRef = useRef<SuggestionList | null>(null);
  const [selection, updateSelection] = useState<string | null>(null);
  const selectionRef = useRef<string | null>(null);
  const setSelection = useCallback((source: string | null) => {
    selectionRef.current = source;
    updateSelection(source);
  }, []);
  const [energyFocus, updateEnergyFocus] = useState(true);
  const energyFocusRef = useRef(true);
  const setEnergyFocus = useCallback((value: boolean) => {
    energyFocusRef.current = value;
    updateEnergyFocus(value);
  }, []);
  const root = useRef<HTMLElement>(null);
  const handler = useRef<
    (action: ControllerBrowserAction) => void | Promise<void>
  >(() => {});
  useEffect(() => {
    registerControllerBrowser({ handle: (action) => handler.current(action) });
    return () => registerControllerBrowser(null);
  }, [registerControllerBrowser]);
  useEffect(() => {
    if (controllerLayer === "playlists" || controllerLayer === "tracks")
      setEnergyFocus(controllerLayer === "playlists");
  }, [controllerLayer, setEnergyFocus]);

  // Refetch whenever an input changes; one request at a time, and the
  // newest inputs win, so the list follows a track change within one round
  // trip without piling requests up while faders move.
  const [refreshes, setRefreshes] = useState(0);
  const requestKey = `${suggestionRequestKey(engine, direction)}#${refreshes}`;
  const wanted = useRef({ key: requestKey, direction });
  wanted.current = { key: requestKey, direction };
  const fetched = useRef<string | null>(null);
  const running = useRef(false);
  const pump = useCallback(async () => {
    if (running.current) return;
    running.current = true;
    try {
      while (fetched.current !== wanted.current.key) {
        const { key, direction } = wanted.current;
        fetched.current = key;
        const result = await native<SuggestionList>("suggest_next", {
          energy_direction: direction,
        });
        // Shown even if the inputs moved on meanwhile; the loop fetches again.
        listRef.current = result;
        updateList(result);
        if (
          selectionRef.current &&
          !result.suggestions.some((s) => s.source === selectionRef.current)
        )
          setSelection(null);
      }
    } catch (error) {
      onError(error);
    } finally {
      running.current = false;
    }
  }, [onError, setSelection]);
  useEffect(() => {
    if (active) setRefreshes((n) => n + 1);
  }, [active, libraryRevision]);
  useEffect(() => {
    if (active && isDesktop) void pump();
  }, [active, requestKey, pump]);
  useEffect(() => {
    if (!active || !list?.pending) return;
    const timer = setTimeout(() => setRefreshes((n) => n + 1), 700);
    return () => clearTimeout(timer);
  }, [active, list]);

  const targetDeck = list?.target_deck ?? null;
  const suggestions = list?.suggestions ?? [];
  const load = useCallback(
    async (deck: number, suggestion: Suggestion) => {
      setSelection(suggestion.source);
      await onLoad(deck, suggestion);
    },
    [onLoad, setSelection],
  );
  handler.current = async (action) => {
    if (action.type === "browse") {
      if (action.shifted) return;
      if (energyFocusRef.current)
        setDirection(stepDirection(directionRef.current, action.delta));
      else {
        const ids = (listRef.current?.suggestions ?? []).map((s) => s.source);
        const index = browseIndex(ids, selectionRef.current, action.delta);
        if (index >= 0) setSelection(ids[index]);
      }
    } else if (action.type === "global_button" && action.pressed) {
      const toEnergy = action.button === "browse_back";
      setEnergyFocus(toEnergy);
      if (!toEnergy && !selectionRef.current)
        setSelection(listRef.current?.suggestions[0]?.source ?? null);
    } else if (action.type === "load" && !action.shifted) {
      const current = listRef.current?.suggestions ?? [];
      const suggestion =
        current.find((s) => s.source === selectionRef.current) ?? current[0];
      if (!suggestion) {
        onError(emptyMessage(listRef.current) ?? "Nothing to load yet.");
        return;
      }
      await load(action.deck, suggestion);
    }
  };
  useEffect(() => {
    if (controllerLayer !== "playlists" && controllerLayer !== "tracks") return;
    const id = energyFocus ? `energy-${direction}` : selection;
    const element = id
      ? [...(root.current?.querySelectorAll("[data-browser-id]") ?? [])].find(
          (row) => row.getAttribute("data-browser-id") === id,
        )
      : root.current?.querySelector('[data-browser-pane="next-suggestions"]');
    scrollBrowserSelection(element ?? null);
    const frame = requestAnimationFrame(() =>
      (element as HTMLElement | undefined)?.focus({ preventScroll: true }),
    );
    return () => cancelAnimationFrame(frame);
  }, [controllerLayer, energyFocus, direction, selection]);

  async function forgetPlayed() {
    try {
      await native("clear_play_history");
      setRefreshes((n) => n + 1);
    } catch (error) {
      onError(error);
    }
  }

  const empty = emptyMessage(list);
  const targetLetter = targetDeck === 1 ? "B" : "A";
  return (
    <section className="library-panel next-panel" ref={root}>
      <div className="library-toolbar">
        <div>
          <h2>
            {list?.reference ? `After ${list.reference.title}` : "Next track"}
          </h2>
          <span>{list ? referenceSummary(list) : ""}</span>
        </div>
        <div className="library-actions">
          {!!list?.played && (
            <button
              onClick={forgetPlayed}
              title="Suggest tracks played this session again"
            >
              <RotateCcw size={13} /> Forget {list.played} played
            </button>
          )}
        </div>
      </div>
      <div className="controller-browser-split">
        <aside
          className={`controller-playlists-pane ${energyFocus && controllerLayer !== "close" ? "pane-active" : ""}`}
          aria-label="Energy"
        >
          <div className="browser-pane-heading">ENERGY</div>
          {ENERGY_DIRECTIONS.map((choice) => (
            <button
              key={choice}
              data-browser-id={`energy-${choice}`}
              aria-pressed={direction === choice}
              className={direction === choice ? "controller-selected" : ""}
              onClick={() => {
                setDirection(choice);
                setEnergyFocus(true);
                onBrowserPaneChange("playlists");
              }}
            >
              {ENERGY_LABELS[choice]}
            </button>
          ))}
          <p className="next-help">
            Ranked by key, tempo and energy against the deck the crowd hears.
            Played tracks and both decks' tracks are left out.
          </p>
        </aside>
        <div
          className={`controller-tracks-pane ${!energyFocus && controllerLayer !== "close" ? "pane-active" : ""}`}
          data-browser-pane="next-suggestions"
          tabIndex={-1}
          onPointerDown={() => {
            if (energyFocusRef.current) {
              setEnergyFocus(false);
              onBrowserPaneChange("tracks");
            }
          }}
        >
          <div className="browser-pane-heading">
            SUGGESTIONS{" "}
            {list?.deck !== null && list?.deck !== undefined && (
              <small>Press LOAD on a deck to load the selected track</small>
            )}
          </div>
          <div className="track-table-wrapper">
            <table className="track-table next-table">
              <thead>
                <tr>
                  <th className="track-number">#</th>
                  <th>TITLE / ARTIST</th>
                  <th>WHY</th>
                  <th>BPM</th>
                  <th>KEY</th>
                  <th title="Energy level, 1 to 10">ENERGY</th>
                  <th>DECK</th>
                </tr>
              </thead>
              <tbody>
                {suggestions.map((s, i) => (
                  <tr
                    key={s.source}
                    data-browser-id={s.source}
                    tabIndex={-1}
                    className={selection === s.source ? "selected" : ""}
                    draggable
                    onDragStart={(e) => {
                      const [type, data] = dragPayload(s);
                      e.dataTransfer.setData(type, data);
                    }}
                    onClick={() => setSelection(s.source)}
                    onDoubleClick={() => {
                      if (targetDeck !== null) void load(targetDeck, s);
                    }}
                  >
                    <td className="track-number">
                      {String(i + 1).padStart(2, "0")}
                    </td>
                    <td>
                      <div className="track-cell">
                        <div className="track-art">
                          {s.spotify ? (
                            <SpotifyLogo size={14} />
                          ) : (
                            <Music2 size={17} />
                          )}
                        </div>
                        <div>
                          <strong>{s.title}</strong>
                          <span>
                            {s.artist ||
                              (s.spotify ? "Spotify" : "Local audio")}
                          </span>
                        </div>
                      </div>
                    </td>
                    <td
                      className={`next-reason ${s.key_clash ? "is-clash" : ""}`}
                      title={s.reason}
                    >
                      {s.reason}
                    </td>
                    <td className="numeric">{s.bpm.toFixed(1)}</td>
                    <td>
                      <span
                        className={`key-tag ${s.key_clash ? "key-clash" : ""}`}
                        title={s.key_clash ? "Key clash" : undefined}
                      >
                        {s.camelot ?? "—"}
                      </span>
                    </td>
                    <td className="numeric">{s.energy_level ?? "—"}</td>
                    <td>
                      {targetDeck !== null && (
                        <div className="load-buttons">
                          <button
                            className={targetDeck === 1 ? "load-b" : "load-a"}
                            title={`Load deck ${targetLetter}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              void load(targetDeck, s);
                            }}
                          >
                            {targetLetter}
                          </button>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {empty && (
              <div className="empty-library" role="status">
                <div className="empty-icon">
                  <SkipForward size={22} />
                </div>
                <p>{empty}</p>
              </div>
            )}
          </div>
          <div className="library-footer">
            <span>{list?.reference ? listCounts(list) : ""}</span>
            <div>
              <span>
                <ArrowLeftRight size={12} />
                Drag to deck
              </span>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

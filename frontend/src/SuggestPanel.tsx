import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import {
  ArrowRight,
  Check,
  Headphones,
  LoaderCircle,
  Play,
  RefreshCw,
  Sparkles,
} from "lucide-react";
import { control, isDesktop } from "./api";
import { listen } from "@tauri-apps/api/event";
import { engineStore } from "./engine-state";
import type { EngineSnapshot } from "./types";
import type {
  BrowserLayer,
  BrowserPane,
  ControllerBrowserHandle,
} from "./controller-browser";
import {
  ENERGY_DIRECTIONS,
  ENERGY_LABELS,
  type EnergyDirection,
} from "./suggestions";
import {
  TECHNIQUE_NAMES,
  moveCountdown,
  mergeMoveFeedback,
  sameMove,
  sourceTime,
  type MixOperation,
  type MoveList,
} from "./next-moves";
import "./styles/suggest.css";
import SuccessorQueuePanel from "./SuccessorQueuePanel";

export default function SuggestPanel({
  engine: initialEngine,
  active,
  libraryRevision,
  controllerLayer,
  registerControllerBrowser,
  onBrowserPaneChange,
  onError,
}: {
  engine: EngineSnapshot;
  active: boolean;
  libraryRevision: unknown;
  controllerLayer: BrowserLayer;
  registerControllerBrowser: (handle: ControllerBrowserHandle | null) => void;
  onBrowserPaneChange: (pane: BrowserPane) => void;
  onError: (error: unknown) => void;
}) {
  const live = useSyncExternalStore(
    engineStore.subscribeControls,
    engineStore.getControls,
  );
  const engine = live.running === initialEngine.running ? live : initialEngine;
  const [direction, setDirection] = useState<EnergyDirection>("keep");
  const [scope, setScope] = useState("all");
  const [technique, setTechnique] = useState("all");
  const [list, setList] = useState<MoveList | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [reviewed, setReviewed] = useState(false);
  const [replacePrepared, setReplacePrepared] = useState(false);
  const [operation, setOperation] = useState<MixOperation | null>(null);
  const [queueActive, setQueueActive] = useState(false);
  const latestOperation = useRef(operation);
  latestOperation.current = operation;
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);
  const [playheads, setPlayheads] = useState(() =>
    engineStore.getTransport().decks.map((d) => ({
      position: d.position_seconds ?? 0,
      rate: d.actual_playback_rate ?? 1,
    })),
  );
  const requestInFlight = useRef(false);
  const handler = useRef<ControllerBrowserHandle["handle"]>(() => {});
  const options =
    list?.options.filter(
      (o) => technique === "all" || o.technique === technique,
    ) ?? [];
  const chosen = options.find((o) => o.id === selected) ?? options[0];
  const audibleDeck = engine.decks.find(
    (d, i) =>
      d.playing &&
      d.track &&
      !d.preparation_pending &&
      (d.channel_fader ?? 1) > 0.001 &&
      (i === 0 ? (engine.crossfader ?? 0) < 1 : (engine.crossfader ?? 0) > -1),
  );
  const targetTrack = chosen ? engine.decks[chosen.target_deck]?.track : null;
  const needsReplacement = !!(
    targetTrack &&
    chosen &&
    targetTrack.path !== chosen.source
  );
  const preparedChoice = sameMove(chosen, operation?.option);
  const armedChoice = preparedChoice && operation?.state === "ready";
  const automaticActive =
    queueActive ||
    (!!operation?.automatic &&
      !operation.terminal &&
      ![
        "completed",
        "recovered",
        "failed",
        "cancelled",
        "yielded",
        "interrupted",
        "invalidated",
      ].includes(operation.state));
  const reference =
    list?.reference_deck == null ? null : engine.decks[list.reference_deck];
  const playhead =
    list?.reference_deck == null ? null : playheads[list.reference_deck];
  const countdown = chosen
    ? moveCountdown(
        chosen,
        playhead?.position ?? reference?.position_seconds ?? 0,
        playhead?.rate ?? reference?.actual_playback_rate ?? 1,
      )
    : 0;
  const sourceKey = `${engine.load_revisions}:${engine.deck_control_revisions}:${engine.decks.map((d) => `${d.position_revision}:${d.grid_revision}`)}`;
  const wanted = useRef("");
  wanted.current = `${scope}:${direction}:${sourceKey}`;
  const refreshOptions = useCallback(async () => {
    if (requestInFlight.current || !isDesktop) return;
    requestInFlight.current = true;
    const requestKey = wanted.current;
    try {
      const result = await control<MoveList>("mix.options", {
        detail: true,
        energy_direction: direction,
        ...(scope === "liked"
          ? { playlist: "spotify:liked", provider: "spotify" }
          : scope === "local"
            ? { provider: "local" }
            : {}),
      });
      if (wanted.current !== requestKey) return;
      setList(result);
      setError("");
    } catch (e) {
      if (wanted.current === requestKey)
        setError(e instanceof Error ? e.message : String(e));
    } finally {
      requestInFlight.current = false;
    }
  }, [direction, scope]);
  useEffect(() => {
    if (active) void refreshOptions();
  }, [active, refreshOptions, sourceKey, refresh, libraryRevision]);
  useEffect(() => {
    if (!active && latestOperation.current?.state === "previewing") {
      void control<MixOperation>("mix.preview", {
        prepared_id: latestOperation.current.prepared_id,
        enabled: false,
      })
        .then(setOperation)
        .catch(onError);
    }
  }, [active, onError]);
  useEffect(
    () => () => {
      const current = latestOperation.current;
      if (current?.state === "previewing") {
        void control("mix.preview", {
          prepared_id: current.prepared_id,
          enabled: false,
        }).catch(onError);
      }
    },
    [onError],
  );
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(
      () =>
        setPlayheads(
          engineStore.getTransport().decks.map((d) => ({
            position: d.position_seconds ?? 0,
            rate: d.actual_playback_rate ?? 1,
          })),
        ),
      250,
    );
    return () => clearInterval(timer);
  }, [active]);
  useEffect(() => {
    setReviewed(false);
    setReplacePrepared(false);
  }, [chosen?.id]);
  useEffect(() => {
    registerControllerBrowser({ handle: (a) => handler.current(a) });
    return () => registerControllerBrowser(null);
  }, [registerControllerBrowser]);
  handler.current = async (action) => {
    if (action.type === "browse" && !action.shifted) {
      const i = options.findIndex((o) => o.id === chosen?.id);
      setSelected(
        options[
          Math.max(0, Math.min(options.length - 1, i + Math.sign(action.delta)))
        ]?.id ?? null,
      );
      onBrowserPaneChange("tracks");
    }
  };
  useEffect(() => {
    if (!active || !isDesktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<Partial<MixOperation>>("mix-updated", ({ payload }) => {
      if (payload.operation_id !== latestOperation.current?.operation_id)
        return;
      setOperation((old) => (old ? mergeMoveFeedback(old, payload) : old));
      if (payload.state === "completed") setRefresh((n) => n + 1);
    })
      .then(async (stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
        // Reconcile once after attaching, including events emitted before the acceptance reply.
        const current = latestOperation.current;
        if (current) {
          try {
            const result = await control<MixOperation>("mix.status", {
              operation_id: current.operation_id,
            });
            if (
              !disposed &&
              latestOperation.current?.operation_id === result.operation_id
            )
              setOperation((old) =>
                old ? mergeMoveFeedback(old, result) : result,
              );
          } catch (e) {
            if (!disposed) setError(String(e));
          }
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active, operation?.operation_id]);
  async function perform(label: string, action: () => Promise<unknown>) {
    setBusy(label);
    setError("");
    try {
      await action();
      setRefresh((n) => n + 1);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      onError(e);
    } finally {
      setBusy(null);
    }
  }
  const prepare = () =>
    chosen &&
    perform("Preparing", async () => {
      const result = await control<MixOperation>("mix.prepare", {
        option_id: chosen.id,
        review_confirmed: reviewed,
        ...(needsReplacement ? { replace_prepared: replacePrepared } : {}),
      });
      setOperation(result);
    });
  return (
    <section className="suggest-panel" aria-label="Next move suggestions">
      <div className="suggest-toolbar">
        <div>
          <h2>
            <Sparkles size={18} /> Next moves
          </h2>
          <p>
            {list?.reference_title
              ? `After ${list.reference_title} · Deck ${list.reference_deck === 1 ? "B" : "A"}`
              : "Load a track to find your next move"}
          </p>
        </div>
        <button
          onClick={() => setRefresh((n) => n + 1)}
          aria-label="Refresh suggestions"
        >
          <RefreshCw size={14} />
        </button>
      </div>
      <div className="suggest-filters">
        <label>
          Collection
          <select
            aria-label="Collection"
            value={scope}
            onChange={(e) => {
              setScope(e.target.value);
              setSelected(null);
              setList(null);
            }}
          >
            <option value="all">All cached music</option>
            <option value="local">Local library</option>
            <option value="liked">Spotify Liked Songs</option>
          </select>
        </label>
        <label>
          Direction
          <select
            aria-label="Direction"
            value={direction}
            onChange={(e) => {
              setDirection(e.target.value as EnergyDirection);
              setList(null);
            }}
          >
            {ENERGY_DIRECTIONS.map((d) => (
              <option key={d} value={d}>
                {ENERGY_LABELS[d]}
              </option>
            ))}
          </select>
        </label>
        <label>
          Technique
          <select
            aria-label="Technique"
            value={technique}
            onChange={(e) => setTechnique(e.target.value)}
          >
            <option value="all">All supported techniques</option>
            {list?.capabilities
              .filter((c) => c.available)
              .map((c) => (
                <option value={c.technique} key={c.technique}>
                  {TECHNIQUE_NAMES[c.technique]}
                </option>
              ))}
          </select>
        </label>
        <span className="suggest-speed">Natural speed · up to ±4%</span>
      </div>
      {list && (
        <div className="suggest-coverage">
          <span>
            {list.coverage.indexed_members} /{" "}
            {list.coverage.total_members ?? "unknown"} indexed ·{" "}
            {list.coverage.analysed_members} analysed ·{" "}
            {list.coverage.needs_analysis_members} need analysis
            {list.considered_tracks !== undefined
              ? ` · ${list.considered_tracks} passage candidates inspected`
              : ""}
            {list.coverage.complete ? "" : " · collection incomplete"}
          </span>
          {scope === "liked" && !list.coverage.complete && (
            <button
              disabled={!!busy}
              onClick={() =>
                void perform("Indexing collection", async () => {
                  await control("mix.index_scope", { scope: "spotify:liked" });
                })
              }
            >
              Index all Liked Songs
            </button>
          )}
        </div>
      )}
      {!!options.length &&
        list?.reasons.map((reason) => (
          <p className="suggest-preview-note" key={reason}>
            {reason}
          </p>
        ))}
      {error && (
        <p className="suggest-error" role="alert">
          {error}
        </p>
      )}
      {busy && (
        <p className="suggest-working" role="status">
          <LoaderCircle size={13} className="spin" /> {busy}…
        </p>
      )}
      <SuccessorQueuePanel
        active={active}
        chosen={chosen}
        direction={direction}
        technique={technique}
        scope={scope}
        carrierPlaying={!!engine.running && !!audibleDeck}
        defaultCollection={
          audibleDeck?.track?.path.startsWith("spotify:") ? "liked" : "local"
        }
        onActiveChange={setQueueActive}
      />
      <div className="suggest-body">
        <div
          className={`suggest-options ${controllerLayer === "tracks" ? "pane-active" : ""}`}
          data-browser-pane="next-suggestions"
          tabIndex={-1}
        >
          {options.map((o, i) => (
            <button
              className={`suggest-option ${chosen?.id === o.id ? "selected" : ""}`}
              key={o.id}
              onClick={() => setSelected(o.id)}
              data-browser-id={o.id}
            >
              <span className="suggest-rank">
                {String(i + 1).padStart(2, "0")}
              </span>
              <span className="suggest-option-main">
                <strong>{o.title}</strong>
                <small>
                  {o.artist || (o.spotify ? "Spotify" : "Local audio")}
                </small>
                <span>
                  {TECHNIQUE_NAMES[o.technique]} · {o.overlap_bars} bars
                </span>
              </span>
              <span className="suggest-option-timing">
                <strong>{o.native_bpm.toFixed(1)} BPM</strong>
                <small>
                  {o.tempo_percent >= 0 ? "+" : ""}
                  {o.tempo_percent.toFixed(2)}%
                </small>
                <span>
                  {sourceTime(o.launch_seconds)} → {sourceTime(o.clear_seconds)}
                </span>
              </span>
            </button>
          ))}
          {!options.length && (
            <div className="suggest-empty">
              <Sparkles size={28} />
              <h3>
                {isDesktop
                  ? "No eligible moves yet"
                  : "Connect the desktop engine"}
              </h3>
              {(list?.reasons.length
                ? list.reasons
                : [
                    "Suggestions use the current decks, analysed passages and source collection.",
                  ]
              ).map((r) => (
                <p key={r}>{r}</p>
              ))}
            </div>
          )}
        </div>
        {chosen && (
          <article className="suggest-detail">
            <div className="suggest-detail-title">
              <span>{TECHNIQUE_NAMES[chosen.technique]}</span>
              <strong>
                Deck {chosen.reference_deck === 1 ? "B" : "A"}
                <ArrowRight size={15} />
                Deck {chosen.target_deck === 1 ? "B" : "A"}
              </strong>
            </div>
            <h3>{chosen.title}</h3>
            {chosen.readiness === "target_requires_isolation" && (
              <p className="suggest-preview-note">
                Close the target deck’s channel fader before preparing a move.
              </p>
            )}
            {chosen.readiness === "target_playing" && (
              <p className="suggest-preview-note">
                The target deck is playing. Prepare after it has been released.
              </p>
            )}
            <div
              className={`suggest-countdown ${countdown < chosen.preparation_budget_seconds ? "urgent" : ""}`}
            >
              <strong>
                {countdown > 0 ? `${Math.ceil(countdown)}s` : "Expired"}
              </strong>
              <span>
                until transition starts
                {!reference?.playing ? " · reference paused" : ""}
                <small>
                  Prepare within{" "}
                  {Math.max(
                    0,
                    Math.floor(countdown - chosen.preparation_budget_seconds),
                  )}
                  s · {chosen.preparation_budget_seconds}s budget
                </small>
              </span>
            </div>
            {chosen.continuation_seconds !== undefined && (
              <p className="suggest-preview-note">
                {Math.floor(chosen.continuation_seconds)}s of music after
                clearance · next move needs at least{" "}
                {Math.ceil(chosen.minimum_continuation_seconds ?? 0)}s runway
              </p>
            )}
            <ol className="suggest-timeline">
              <li>
                <time>{sourceTime(chosen.launch_seconds)}</time>
                <span>
                  Start incoming at {sourceTime(chosen.entry_seconds)}
                  <small>Playback entry · incoming fader closed</small>
                </span>
              </li>
              <li>
                <time>{sourceTime(chosen.audible_seconds)}</time>
                <span>
                  Introduce incoming layers
                  <small>Verify advancing audio before raising its fader</small>
                </span>
              </li>
              <li>
                <time>{sourceTime(chosen.handover_seconds)}</time>
                <span>
                  Foreground handover
                  <small>
                    {chosen.technique === "filter_blend"
                      ? "Filter creates space while the new layers rise"
                      : "Shape EQ and balance the overlapping layers"}
                  </small>
                </span>
              </li>
              <li>
                <time>{sourceTime(chosen.clear_seconds)}</time>
                <span>
                  Clear outgoing
                  <small>
                    Incoming continues at {chosen.target_bpm.toFixed(1)} BPM
                  </small>
                </span>
              </li>
            </ol>
            <details open>
              <summary>Evidence & review</summary>
              <ul>
                {chosen.evidence.map((e) => (
                  <li key={e}>{e}</li>
                ))}
              </ul>
              <div className="suggest-warnings">
                {chosen.warnings.map((w) => (
                  <p key={w}>{w}</p>
                ))}
              </div>
            </details>
            <label className="suggest-review">
              <input
                type="checkbox"
                checked={reviewed}
                onChange={(e) => setReviewed(e.target.checked)}
              />
              I checked the passages and their foreground / vocal overlap
            </label>
            {needsReplacement && (
              <label className="suggest-review">
                <input
                  type="checkbox"
                  checked={replacePrepared}
                  onChange={(e) => setReplacePrepared(e.target.checked)}
                />
                Replace the paused track “{targetTrack?.title}” on Deck{" "}
                {chosen.target_deck === 1 ? "B" : "A"}
              </label>
            )}
            <div className="suggest-execution">
              <button
                disabled={
                  !!busy ||
                  automaticActive ||
                  !reviewed ||
                  (needsReplacement && !replacePrepared) ||
                  countdown <= 4 ||
                  !reference?.playing
                }
                onClick={() =>
                  void perform("Executing move", async () => {
                    setOperation(
                      await control<MixOperation>("mix.execute", {
                        option_id: chosen.id,
                        review_confirmed: reviewed,
                        ...(needsReplacement
                          ? { replace_prepared: replacePrepared }
                          : {}),
                      }),
                    );
                  })
                }
              >
                <Play size={14} /> Execute move
              </button>
              <button
                disabled={
                  !!busy ||
                  automaticActive ||
                  (needsReplacement && !replacePrepared) ||
                  countdown <= chosen.preparation_budget_seconds ||
                  operation?.state === "previewing"
                }
                onClick={() => void prepare()}
              >
                <Check size={14} /> Prepare
              </button>
              <button
                disabled={
                  !!busy ||
                  !preparedChoice ||
                  !["ready", "previewing"].includes(operation?.state ?? "")
                }
                onClick={() =>
                  void perform(
                    operation?.state === "previewing"
                      ? "Stopping preview"
                      : "Starting headphone preview",
                    async () =>
                      setOperation(
                        await control<MixOperation>("mix.preview", {
                          prepared_id: operation!.prepared_id,
                          enabled: operation?.state !== "previewing",
                        }),
                      ),
                  )
                }
              >
                <Headphones size={14} />
                {operation?.state === "previewing"
                  ? "Stop preview"
                  : "Headphone preview"}
              </button>
              <button
                disabled={!!busy || !armedChoice || !reviewed}
                onClick={() =>
                  void perform("Arming transition", async () =>
                    setOperation(
                      await control<MixOperation>("mix.arm", {
                        prepared_id: operation!.prepared_id,
                        review_confirmed: reviewed,
                      }),
                    ),
                  )
                }
              >
                <Play size={14} /> Arm transition
              </button>
              {operation &&
                !["completed", "cancelled", "failed"].includes(
                  operation.state,
                ) && (
                  <button
                    disabled={!!busy}
                    onClick={() =>
                      void perform("Cancelling", async () =>
                        setOperation(
                          await control<MixOperation>("mix.cancel", {
                            operation_id: operation.operation_id,
                          }),
                        ),
                      )
                    }
                  >
                    Cancel
                  </button>
                )}
            </div>
            {operation && (
              <p className="suggest-operation" role="status">
                Transition: {operation.state}
                {operation.option && !sameMove(operation.option, chosen)
                  ? ` · ${operation.option.title} (${TECHNIQUE_NAMES[operation.option.technique]})`
                  : ""}
                {operation.error ? ` · ${operation.error.message}` : ""}
              </p>
            )}
            <p className="suggest-preview-note">
              <Headphones size={13} /> Prepare, then preview the incoming
              passage in headphones when needed. After reviewing the evidence,
              Execute move handles preparation, timing and a short fallback
              automatically. Prepare and Arm remain available for an auditioned
              move.
            </p>
          </article>
        )}
      </div>
      {list && (
        <details className="suggest-analysis">
          <summary>
            {list.coverage.needs_analysis_members} tracks need analysis ·
            technique capabilities
          </summary>
          {!!list.coverage.needs_analysis.length && (
            <div className="suggest-analysis-members">
              {list.coverage.needs_analysis.slice(0, 30).map((m) => (
                <div key={m.source}>
                  <span>
                    {m.title || m.source}
                    <small>{m.artist}</small>
                  </span>
                  {m.source.startsWith("spotify:") && (
                    <button
                      disabled={!!busy}
                      onClick={() =>
                        void perform("Queuing analysis", async () => {
                          await control("spotify.analysis.start", {
                            uris: [m.source],
                          });
                        })
                      }
                    >
                      Analyse
                    </button>
                  )}
                </div>
              ))}
              {list.coverage.needs_analysis_members > 30 && (
                <p>
                  Showing first 30; collection indexing and analysis are
                  separate.
                </p>
              )}
            </div>
          )}
          <div className="suggest-capabilities">
            {list.capabilities.map((c) => (
              <div key={c.technique}>
                <strong>
                  {TECHNIQUE_NAMES[c.technique]}{" "}
                  <small>{c.available ? "Available" : "Unavailable"}</small>
                </strong>
                <span>{c.reason}</span>
              </div>
            ))}
          </div>
        </details>
      )}
    </section>
  );
}

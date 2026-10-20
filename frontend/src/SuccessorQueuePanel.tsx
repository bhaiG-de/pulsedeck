import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { control, isDesktop } from "./api";
import type { NextMove } from "./next-moves";
import { TECHNIQUE_NAMES } from "./next-moves";
import type { EnergyDirection } from "./suggestions";
import {
  mergeQueue,
  reorderQueue,
  type QueueTrack,
  type SuccessorQueue,
} from "./successor-queue";

export default function SuccessorQueuePanel({
  active,
  chosen,
  direction,
  technique,
  scope,
  onActiveChange,
}: {
  active: boolean;
  chosen?: NextMove;
  direction: EnergyDirection;
  technique: string;
  scope: string;
  onActiveChange: (active: boolean) => void;
}) {
  const [queue, setQueue] = useState<SuccessorQueue | null>(null);
  const [draft, setDraft] = useState<QueueTrack[]>([]);
  const [mode, setMode] = useState<"ordered" | "collection">("ordered");
  const [collection, setCollection] = useState(
    scope === "liked" ? "liked" : "local",
  );
  const [minutes, setMinutes] = useState("");
  const [stretch, setStretch] = useState("100");
  const [replace, setReplace] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const running = !!queue?.queue_id && !queue.terminal;
  const policyDirection = queue?.queue_id
    ? (queue?.policy?.energy_direction ?? direction)
    : direction;
  const policyTechniques = queue?.queue_id
    ? queue?.policy?.techniques
    : technique === "all"
      ? null
      : [technique];
  const techniqueLabel = policyTechniques?.length
    ? policyTechniques.map((t) => TECHNIQUE_NAMES[t] ?? t).join(", ")
    : "supported techniques";
  const items = running || queue?.queue_id ? (queue?.items ?? []) : draft;
  const pending = running ? items.filter((i) => i.state === "pending") : draft;
  useEffect(() => {
    onActiveChange(running);
  }, [onActiveChange, running]);
  useEffect(() => {
    if (!active || !isDesktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<SuccessorQueue>("mix-queue-updated", ({ payload }) => {
      if (!disposed) setQueue((old) => mergeQueue(old, payload));
    })
      .then(async (stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
        const result = await control<SuccessorQueue>("mix.queue.status", {});
        if (!disposed) setQueue((old) => mergeQueue(old, result));
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active]);
  async function perform(action: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      const latest = await control<SuccessorQueue>(
        "mix.queue.status",
        {},
      ).catch(() => null);
      if (latest) setQueue((old) => mergeQueue(old, latest));
    } finally {
      setBusy(false);
    }
  }
  async function edit(tracks: QueueTrack[]) {
    if (!running || !queue?.queue_id) {
      setDraft(tracks);
      setQueue(null);
      return;
    }
    await perform(async () => {
      const result = await control<SuccessorQueue>("mix.queue.edit", {
        queue_id: queue.queue_id,
        expected_revision: queue.revision,
        sources: tracks.map((t) => t.source),
      });
      setQueue((old) => mergeQueue(old, result));
    });
  }
  function move(index: number, delta: number) {
    const order = reorderQueue(
      pending.map((i) => i.source),
      index,
      delta,
    );
    void edit(order.map((source) => pending.find((i) => i.source === source)!));
  }
  const alreadyQueued =
    !!chosen && pending.some((i) => i.source === chosen.source);
  return (
    <details className="suggest-successor-queue">
      <summary>
        Successor queue{" "}
        <span>
          {queue?.queue_id
            ? queue.readiness === "preparing"
              ? "Preparing next track"
              : queue.state
            : `${draft.length} planned`}
        </span>
      </summary>
      <div className="suggest-queue-content">
        <p>
          Queue owns preparation and native transitions. Musical policy:{" "}
          {policyDirection} energy · {techniqueLabel}. Start a playing carrier
          first.
        </p>
        {queue?.coverage?.indexed_members != null && (
          <p>
            {queue.coverage.indexed_members} tracks indexed ·{" "}
            {queue.coverage.analysed_members} analysed
            {queue.coverage.needs_analysis_members > 0 &&
              " · New candidates are prepared as needed"}
          </p>
        )}
        {!running && !queue?.queue_id && (
          <div className="suggest-queue-policy">
            <label>
              Repertoire
              <select
                aria-label="Queue repertoire"
                value={mode}
                onChange={(e) => {
                  const value = e.target.value as "ordered" | "collection";
                  setMode(value);
                  if (value === "collection" && !minutes) setMinutes("60");
                }}
              >
                <option value="ordered">Ordered tracks</option>
                <option value="collection">Choose from collection</option>
              </select>
            </label>
            {mode === "collection" && (
              <label>
                Collection
                <select
                  aria-label="Queue collection"
                  value={collection}
                  onChange={(e) => setCollection(e.target.value)}
                >
                  <option value="liked">Spotify Liked Songs</option>
                  <option value="local">Local library</option>
                </select>
              </label>
            )}
            <label>
              End after minutes (optional)
              <input
                aria-label="Queue duration minutes"
                type="number"
                min="0.017"
                max="1440"
                step="1"
                value={minutes}
                onChange={(e) => setMinutes(e.target.value)}
                placeholder="Until ordered queue ends"
              />
            </label>
            <label>
              Maximum tempo change %
              <input
                aria-label="Queue maximum stretch"
                type="number"
                min="0"
                max="100"
                step="0.25"
                value={stretch}
                onChange={(e) => setStretch(e.target.value)}
              />
            </label>
          </div>
        )}
        {(!queue?.queue_id || running) &&
          (!running || queue?.mode === "ordered") &&
          (running || mode === "ordered") && (
            <button
              type="button"
              disabled={!chosen || busy || alreadyQueued}
              onClick={() =>
                chosen &&
                void edit([
                  ...pending,
                  {
                    source: chosen.source,
                    title: chosen.title,
                    artist: chosen.artist,
                    state: "pending",
                    availability: "analysis_cached",
                  },
                ])
              }
            >
              Add selected track to queue
            </button>
          )}
        {!!items.length && (
          <ol
            className="suggest-queue-items"
            aria-label="Successor queue tracks"
          >
            {items.map((item, index) => {
              const pendingIndex = pending.findIndex(
                (i) => i.source === item.source,
              );
              const editable =
                (!running && !queue?.queue_id) ||
                (running && item.state === "pending");
              return (
                <li key={item.id ?? `${item.source}-${index}`}>
                  <div>
                    <strong>{item.title}</strong>
                    <small>
                      {item.artist} ·{" "}
                      {(item.state ?? "pending").replaceAll("_", " ")} ·{" "}
                      {(item.availability ?? "unchecked").replaceAll("_", " ")}
                    </small>
                    {item.reason && (
                      <small className="suggest-error">{item.reason}</small>
                    )}
                  </div>
                  {editable && (
                    <div className="suggest-queue-edit">
                      <button
                        type="button"
                        aria-label={`Move ${item.title} earlier`}
                        disabled={busy || pendingIndex <= 0}
                        onClick={() => move(pendingIndex, -1)}
                      >
                        ↑
                      </button>
                      <button
                        type="button"
                        aria-label={`Move ${item.title} later`}
                        disabled={busy || pendingIndex >= pending.length - 1}
                        onClick={() => move(pendingIndex, 1)}
                      >
                        ↓
                      </button>
                      <button
                        type="button"
                        aria-label={`Remove ${item.title} from queue`}
                        disabled={busy}
                        onClick={() =>
                          void edit(
                            pending.filter((_, i) => i !== pendingIndex),
                          )
                        }
                      >
                        Remove
                      </button>
                    </div>
                  )}
                </li>
              );
            })}
          </ol>
        )}
        {queue?.queue_id && (
          <p role="status">
            Queue: {queue.state.replaceAll("_", " ")} ·{" "}
            {queue.completed_moves ?? 0} handovers completed
            {queue.reason ? ` · ${queue.reason.replaceAll("_", " ")}` : ""}
          </p>
        )}
        {!running && !queue?.queue_id && (
          <>
            <label className="suggest-review">
              <input
                type="checkbox"
                checked={replace}
                onChange={(e) => setReplace(e.target.checked)}
              />
              Allow replacing a paused prepared deck
            </label>
            <button
              type="button"
              disabled={
                !isDesktop ||
                busy ||
                (mode === "ordered" && !draft.length) ||
                !Number.isFinite(Number(stretch)) ||
                (minutes !== "" && Number(minutes) <= 0)
              }
              onClick={() =>
                void perform(async () => {
                  const result = await control<SuccessorQueue>(
                    "mix.queue.start",
                    {
                      ...(mode === "ordered"
                        ? {
                            sources: draft.map((t) => t.source),
                          }
                        : collection === "liked"
                          ? { playlist: "spotify:liked", provider: "spotify" }
                          : { provider: "local" }),
                      energy_direction: direction,
                      max_stretch_percent: Number(stretch),
                      ...(technique !== "all"
                        ? { techniques: [technique] }
                        : {}),
                      replace_prepared: replace,
                      max_skips: 3,
                      ...(minutes
                        ? { duration_seconds: Number(minutes) * 60 }
                        : {}),
                    },
                  );
                  setQueue(result);
                })
              }
            >
              Start successor queue
            </button>
          </>
        )}
        {running && (
          <button
            type="button"
            className="danger"
            disabled={busy}
            onClick={() =>
              void perform(async () => {
                const result = await control<SuccessorQueue>("mix.queue.stop", {
                  queue_id: queue?.queue_id,
                });
                setQueue((old) => mergeQueue(old, result));
              })
            }
          >
            Stop successor queue
          </button>
        )}
        {!running && !!queue?.queue_id && (
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              setQueue(null);
              setDraft([]);
            }}
          >
            Plan a new queue
          </button>
        )}
        {error && (
          <p className="suggest-error" role="alert">
            {error}
          </p>
        )}
        <p className="suggest-preview-note">
          A duration endpoint stops playback at its deadline. Exhausted or
          failed queues ask for attention and preserve the current music; Stop
          cancels automation without pausing ordinary playback.
        </p>
      </div>
    </details>
  );
}

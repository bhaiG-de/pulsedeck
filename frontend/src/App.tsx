import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Circle,
  Square,
  LockKeyhole,
  AudioLines,
  ArrowLeftRight,
  ChevronDown,
  ChevronRight,
  Disc3,
  FolderPlus,
  Headphones,
  Library,
  Link2,
  LoaderCircle,
  Minus,
  Music2,
  Pause,
  Play,
  Plus,
  Radio,
  Search,
  Settings2,
  SlidersHorizontal,
  Volume2,
  X,
  Zap,
  Check,
  AlertCircle,
  SkipBack,
  Repeat2,
} from "lucide-react";
import { isDesktop, native, control } from "./api";
import TrackTags from "./TrackTags";
import { useControl } from "./control-bridge";
import {
  controlString,
  controlBoolean,
  controlNumber,
  controlEnum,
  controlInteger,
  controlDraft,
} from "./control-state";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import {
  AppState,
  AudioDeviceInfo,
  DeckSnapshot,
  EngineSnapshot,
  MidiState,
  Playlist,
  TrackRecord,
  RecordingStatus,
  emptyEngine,
} from "./types";
import Waveform from "./Waveform";
import AnalysisInfo from "./AnalysisInfo";
import TrackDrawer from "./TrackDrawer";
import { beatPosition, beatPhaseDelta, snapToBeat } from "./mixing-view";
import {
  withTrackMetadata,
  type EngineTransport,
  type EngineMetadata,
} from "./engine-state";
import { BAND_COLORS, HOT_CUE_COLORS } from "./waveform-data";
import AnatomyDial from "./AnatomyDial";
import SpotifyPanel, { type SpotifyTrack } from "./SpotifyPanel";
import {
  THEMES,
  applyTheme,
  readTheme,
  saveTheme,
  type ThemeId,
} from "./theme";
import {
  browseIndex,
  isBrowserAction,
  isInstantDouble,
  navigateBrowser,
  type BrowserNavigation,
  type BrowserNavigationEvent,
  controllerLabel,
  type ControllerSurface,
  zoomBeats,
  scrollBrowserSelection,
  type ControllerBrowserAction,
  type ControllerBrowserHandle,
} from "./controller-browser";
const fmt = (seconds: number) => {
  const value = Math.max(0, Math.floor(seconds || 0));
  return `${Math.floor(value / 60)
    .toString()
    .padStart(2, "0")}:${(value % 60).toString().padStart(2, "0")}`;
};
const val = (n: unknown, fallback = 0) =>
  typeof n === "number" && Number.isFinite(n) ? n : fallback;
type Command = Record<string, unknown>;
type Notice = { text: string; error?: boolean };
function Range({
  label,
  value,
  onChange,
  min = 0,
  max = 1,
  step = 0.01,
  vertical = false,
  display,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
  vertical?: boolean;
  display?: string;
}) {
  return (
    <label className={`range-control ${vertical ? "vertical" : ""}`}>
      <span>{label}</span>
      <input
        aria-label={label}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(+e.target.value)}
      />
      {display && <output>{display}</output>}
    </label>
  );
}
function Knob({
  label,
  value,
  min = 0,
  max = 2,
  onChange,
  reset = 1,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
  reset?: number;
}) {
  return (
    <label className="knob-control">
      <span
        className="knob"
        style={
          {
            "--rotation": `${-135 + ((value - min) / (max - min)) * 270}deg`,
          } as React.CSSProperties
        }
      >
        <input
          aria-label={label}
          title={`${label}: ${value.toFixed(2)}. Double click to reset.`}
          type="range"
          min={min}
          max={max}
          step={0.01}
          value={value}
          onChange={(e) => onChange(+e.target.value)}
          onDoubleClick={() => onChange(reset)}
        />
        <AnatomyDial />
        <i />
      </span>
      <span>{label}</span>
    </label>
  );
}
function Meter({ value = 0 }: { value?: number }) {
  return (
    <div className="meter" aria-label={`Level ${Math.round(value * 100)}%`}>
      {Array.from({ length: 18 }, (_, i) => (
        <i
          key={i}
          className={`${i > 14 ? "peak" : ""} ${value > i / 18 ? "lit" : ""}`}
        />
      ))}
    </div>
  );
}
function DeckWaveform({
  id,
  deck,
  library,
  command,
  visibleBeats,
  snapSeek,
  waveformCommand,
  snapshotTime,
}: {
  id: number;
  deck: DeckSnapshot;
  library: TrackRecord[];
  command: (c: Command) => void;
  waveformCommand: (c: Command) => Promise<void>;
  snapshotTime: number;
  visibleBeats: number;
  snapSeek: boolean;
}) {
  const track =
    deck.track ||
    library.find((t) => t.id === deck.track_id || t.path === deck.track_path);
  const duration = val(
    track?.duration_seconds,
    val(deck.duration_seconds, val(deck.duration)),
  );
  const position = val(deck.position_seconds, val(deck.position));
  const baseBpm = val(deck.bpm, val(track?.bpm, val(deck.original_bpm)));
  const gridOffset = val(deck.beatgrid_offset_seconds);
  const heldPosition = useRef<number | null>(null);
  const currentSource = useRef(track?.path);
  currentSource.current = track?.path;
  const color = id === 0 ? "#f4b65b" : "#62cbd5";
  return (
    <section
      className="stacked-wave-row"
      aria-label={`Deck ${id === 0 ? "A" : "B"} scrolling waveform`}
      style={{ "--deck-color": color } as React.CSSProperties}
    >
      <span className="scrolling-deck-label">{id === 0 ? "A" : "B"}</span>
      <Waveform
        samples={track?.waveform || deck.waveform || []}
        bands={track?.waveform_bands}
        position={position}
        duration={duration}
        bpm={baseBpm}
        spanBeats={visibleBeats}
        color={color}
        hotcues={deck.hot_cues || []}
        memoryCues={deck.memory_cues || []}
        cuePoint={deck.cue_seconds}
        loopStart={deck.loop_start_seconds}
        loopEnd={deck.loop_end_seconds}
        loopActive={deck.loop_active}
        beatgridOffset={gridOffset}
        onSeek={(seconds) =>
          command({
            type: "seek",
            deck: id,
            seconds: snapSeek
              ? snapToBeat(seconds, baseBpm, gridOffset, duration)
              : seconds,
          })
        }
        playing={!!deck.playing}
        playbackRate={val(
          deck.playback_rate,
          1 + val(deck.tempo_percent) / 100,
        )}
        jogTouch={!!deck.jog_touch}
        positionRevision={deck.position_revision}
        sourceId={track?.path}
        snapshotTime={snapshotTime}
        onScrubStart={async (grabbed) => {
          heldPosition.current = grabbed;
          await waveformCommand({ type: "jog_touch", deck: id, touched: true });
          if (currentSource.current === track?.path)
            await waveformCommand({ type: "seek", deck: id, seconds: grabbed });
        }}
        onScrub={async (delta, elapsedMs) => {
          if (currentSource.current !== track?.path) return;
          const target = Math.max(
            0,
            Math.min(duration, (heldPosition.current ?? position) + delta),
          );
          heldPosition.current = target;
          await waveformCommand({
            type: "scrub_to",
            deck: id,
            seconds: target,
            duration_ms: Math.min(100, elapsedMs + 8),
          });
        }}
        onScrubEnd={async () => {
          const target = heldPosition.current;
          heldPosition.current = null;
          if (target != null && currentSource.current === track?.path) {
            try {
              await waveformCommand({
                type: "scrub_end",
                deck: id,
                seconds: target,
              });
              return;
            } catch {
              // Failed commit must still release a held platter.
            }
          }
          await waveformCommand({
            type: "jog_touch",
            deck: id,
            touched: false,
          });
        }}
      />
    </section>
  );
}
function WaveformAlignment({ decks }: { decks: DeckSnapshot[] }) {
  const phases = decks.map((d) =>
    d.track
      ? beatPosition(
          val(d.position_seconds),
          val(d.bpm, val(d.track.bpm)),
          val(d.beatgrid_offset_seconds),
        )
      : null,
  );
  const delta = beatPhaseDelta(phases[0] ?? null, phases[1] ?? null);
  const bpms = decks.map(
    (d) => val(d.bpm, val(d.track?.bpm)) * (1 + val(d.tempo_percent) / 100),
  );
  return (
    <div
      className="waveform-alignment"
      title="Beat phase from the decks' current beatgrids. Check the first-beat alignment before relying on it."
    >
      <span>BEAT ALIGN</span>
      <div className="phase-gauge">
        <i className="phase-center" />
        <i
          className={`phase-dot ${delta !== null && Math.abs(delta) < 0.025 ? "aligned" : ""}`}
          style={{
            left: `${50 + (delta ?? 0) * 80}%`,
            visibility: delta === null ? "hidden" : "visible",
          }}
        />
      </div>
      <strong>
        {delta === null
          ? "—"
          : `${delta >= 0 ? "+" : ""}${delta.toFixed(2)} beat`}
      </strong>
      <span className="tempo-difference">
        {decks.every((d) => !!d.track && val(d.bpm, val(d.track.bpm)) > 0)
          ? `Δ ${(bpms[1] - bpms[0]).toFixed(2)} BPM`
          : "Load both decks"}
      </span>
    </div>
  );
}
function Deck({
  id,
  deck,
  library,
  command,
  waveformCommand,
  load,
  demo,
  reanalyze,
  loadSpotify,
  visibleBeats,
  snapSeek,
  snapshotTime,
  hardwareRange,
  hardwarePadMode,
  onTempoRange,
}: {
  id: number;
  deck: DeckSnapshot;
  library: TrackRecord[];
  command: (c: Command) => void;
  waveformCommand: (c: Command) => Promise<void>;
  load: (deck: number, path: string) => void;
  demo: (deck: number) => void;
  reanalyze: (deck: number) => Promise<void>;
  loadSpotify: (deck: number, track: SpotifyTrack) => Promise<void>;
  visibleBeats: number;
  snapSeek: boolean;
  snapshotTime: number;
  hardwareRange?: number;
  hardwarePadMode?: string;
  onTempoRange: (deck: number, range: number) => void;
}) {
  const track =
    deck.track ||
    library.find((t) => t.id === deck.track_id || t.path === deck.track_path);
  const title = track?.title || deck.track_title || deck.title;
  const duration = val(
    track?.duration_seconds,
    val(deck.duration_seconds, val(deck.duration)),
  );
  const position = val(deck.position_seconds, val(deck.position));
  const baseBpm = val(deck.bpm, val(track?.bpm, val(deck.original_bpm)));
  const tempo = val(deck.tempo_percent);
  const bpm = baseBpm * (1 + tempo / 100);
  const hotcues = deck.hot_cues || Array(8).fill(null);
  const [editGrid, setEditGrid] = useState(false);
  const [gridBpm, setGridBpm] = useState("120");
  const [gridOffset, setGridOffset] = useState("0");
  useEffect(() => {
    setEditGrid(false);
  }, [track?.id, track?.path]);
  const [range, setRange] = useState(6);
  useEffect(() => {
    if (hardwareRange !== undefined) setRange(hardwareRange);
  }, [hardwareRange]);
  const activeLoopBeats =
    deck.loop_start_seconds != null && deck.loop_end_seconds != null
      ? ((deck.loop_end_seconds - deck.loop_start_seconds) * baseBpm) / 60
      : 0;
  const [padMode, setPadMode] = useState<"hotcue" | "loop" | "memory">(
    "hotcue",
  );
  const [menu, setMenu] = useState(false);
  const [scratch, setScratch] = useState(false);
  const [dropTarget, setDropTarget] = useState(false);
  const currentJogSource = useRef(track?.path);
  currentJogSource.current = track?.path;
  const jogGesture = useRef<{
    source: string | undefined;
    target: number;
    x: number;
    lastAt: number;
    pending: boolean;
  } | null>(null);
  const jogFrame = useRef<number | null>(null);
  const jogQueue = useRef(Promise.resolve());
  const enqueueJog = (
    c: Command,
    source: string | undefined,
    release = false,
  ) => {
    jogQueue.current = jogQueue.current
      .then(async () => {
        if (currentJogSource.current === source)
          await waveformCommand({ ...c, deck: id });
        else if (release)
          await waveformCommand({
            type: "jog_touch",
            deck: id,
            touched: false,
          });
      })
      .catch(() => {});
  };
  const flushJog = () => {
    const gesture = jogGesture.current;
    if (!gesture?.pending) return;
    const now = performance.now();
    const elapsed = Math.max(8, Math.min(100, now - gesture.lastAt + 8));
    gesture.pending = false;
    gesture.lastAt = now;
    enqueueJog(
      { type: "scrub_to", seconds: gesture.target, duration_ms: elapsed },
      gesture.source,
    );
  };
  const finishJog = () => {
    if (jogFrame.current !== null) cancelAnimationFrame(jogFrame.current);
    jogFrame.current = null;
    flushJog();
    const gesture = jogGesture.current;
    jogGesture.current = null;
    setScratch(false);
    if (gesture)
      enqueueJog(
        { type: "scrub_end", seconds: gesture.target },
        gesture.source,
        true,
      );
  };
  useEffect(
    () => () => {
      if (jogFrame.current !== null) cancelAnimationFrame(jogFrame.current);
      jogFrame.current = null;
      if (jogGesture.current) {
        jogGesture.current = null;
        setScratch(false);
        jogQueue.current = jogQueue.current
          .then(() =>
            waveformCommand({ type: "jog_touch", deck: id, touched: false }),
          )
          .catch(() => {});
      }
    },
    [track?.path, id, waveformCommand],
  );
  const color = id === 0 ? "#f4b65b" : "#62cbd5";
  const send = (type: string, data: Command = {}) =>
    command({ type, deck: id, ...data });
  useControl(
    `deck-${id === 0 ? "A" : "B"}`,
    {
      menu: (v) => setMenu(controlBoolean(v)),
      pads: (v) => setPadMode(controlEnum(v, ["hotcue", "loop", "memory"])),
      "tempo-range": (v) => {
        const n = controlNumber(v, 6, 100);
        if (![6, 10, 16, 100].includes(n))
          throw new Error("Range must be 6/10/16/100");
        setRange(n);
        onTempoRange(id, n);
      },
      "grid-editor": (v) => {
        const open = controlBoolean(v);
        if (open) {
          setGridBpm(String(baseBpm || 120));
          setGridOffset(String(val(deck.beatgrid_offset_seconds)));
        }
        setEditGrid(open);
      },
      "grid-draft": (v) => {
        const draft = controlDraft(v, {
          bpm: (v) => controlNumber(v, 30, 300),
          offset: (v) => controlNumber(v, 0, duration),
        });
        if (draft.bpm !== undefined)
          setGridBpm(String(controlNumber(draft.bpm, 30, 300)));
        if (draft.offset !== undefined)
          setGridOffset(String(controlNumber(draft.offset, 0, duration)));
      },
      "grid-half": () =>
        setGridBpm(String(controlNumber(+gridBpm / 2, 30, 300))),
      "grid-double": () =>
        setGridBpm(String(controlNumber(+gridBpm * 2, 30, 300))),
      "grid-apply": async () => {
        const bpm = controlNumber(+gridBpm, 30, 300);
        const offset = controlNumber(+gridOffset, 0, duration);
        await waveformCommand({ type: "set_bpm", deck: id, bpm });
        await waveformCommand({
          type: "set_beatgrid",
          deck: id,
          offset_seconds: offset,
        });
        setEditGrid(false);
      },
      "jog-cancel": finishJog,
    },
    {
      menu,
      pads: padMode,
      tempo_range: range,
      grid_editor: editGrid,
      grid_draft: { bpm: gridBpm, offset: gridOffset },
      scratch,
    },
  );
  return (
    <section
      className={`deck deck-${id} ${dropTarget ? "drop-target" : ""}`}
      style={{ "--deck-color": color } as React.CSSProperties}
      onDragOver={(e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
        setDropTarget(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null))
          setDropTarget(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDropTarget(false);
        const path = e.dataTransfer.getData("application/pulsedeck-track");
        if (path) {
          load(id, path);
          return;
        }
        const spotify = e.dataTransfer.getData(
          "application/pulsedeck-spotify-track",
        );
        if (spotify) {
          try {
            const track = JSON.parse(spotify) as SpotifyTrack;
            if (
              typeof track.uri === "string" &&
              /^spotify:track:[A-Za-z0-9]{22}$/.test(track.uri)
            )
              void loadSpotify(id, track);
          } catch {
            /* Ignore unrelated drag payloads. */
          }
        }
      }}
    >
      <div className="deck-heading">
        <span className="deck-badge">{id === 0 ? "A" : "B"}</span>
        <div className="track-heading">
          <strong>{title || "Ready for your next track"}</strong>
          <span>
            {track?.artist ||
              (title
                ? "Local audio · native playback"
                : "Drag a track here or choose from the library")}
          </span>
        </div>
        <div className="load-menu">
          <button className="subtle" onClick={() => setMenu(!menu)}>
            Load <ChevronDown size={13} />
          </button>
          {menu && (
            <div className="dropdown">
              <button
                onClick={() => {
                  demo(id);
                  setMenu(false);
                }}
              >
                <AudioLines size={14} />
                Load test tone <small>Generated audio</small>
              </button>
              {library.slice(0, 12).map((t) => (
                <button
                  key={t.id}
                  onClick={() => {
                    load(id, t.path);
                    setMenu(false);
                  }}
                >
                  {t.title}
                </button>
              ))}
              {!library.length && (
                <div className="muted">Import audio to populate this list</div>
              )}
            </div>
          )}
        </div>
      </div>
      <div className="deck-info">
        <div className="tempo-readout">
          <strong>{bpm ? bpm.toFixed(2) : "—"}</strong>
          <span>BPM</span>
          <em>
            {tempo >= 0 ? "+" : ""}
            {tempo.toFixed(2)}%{" "}
            <button
              className="edit-grid"
              disabled={!title}
              onClick={() => {
                setGridBpm(String(baseBpm || 120));
                setGridOffset(String(val(deck.beatgrid_offset_seconds)));
                setEditGrid(!editGrid);
              }}
            >
              GRID
            </button>
          </em>
        </div>
        <div className="key-readout">
          <strong
            title={track?.key ? `Estimated key: ${track.key}` : undefined}
          >
            {track?.camelot || track?.key || deck.key || "—"}
          </strong>
          <span>KEY</span>
        </div>
        <div className="time-readout">
          <strong>{fmt(position)}</strong>
          <span>−{fmt(duration - position)} remaining</span>
        </div>
      </div>
      {track && (
        <AnalysisInfo
          controlTarget={`analysis-${id === 0 ? "A" : "B"}`}
          key={`${track.id}-${track.path}`}
          track={track}
          onReanalyze={() => reanalyze(id)}
          baseBpm={baseBpm}
          onChooseBpm={(bpm) => {
            setGridBpm(bpm.toFixed(2));
            setGridOffset(String(val(deck.beatgrid_offset_seconds)));
            setEditGrid(true);
          }}
        />
      )}
      {editGrid && (
        <form
          className="grid-editor"
          onSubmit={(e) => {
            e.preventDefault();
            const bpm = Number(gridBpm);
            const offset = Number(gridOffset);
            if (bpm >= 30 && bpm <= 300 && Number.isFinite(offset)) {
              send("set_bpm", { bpm });
              send("set_beatgrid", { offset_seconds: offset });
              setEditGrid(false);
            }
          }}
        >
          <label>
            BPM
            <input
              aria-label={`Deck ${id + 1} beatgrid BPM`}
              type="number"
              min={30}
              max={300}
              step={0.01}
              value={gridBpm}
              onChange={(e) => setGridBpm(e.target.value)}
            />
          </label>
          <div className="grid-tempo-tools">
            <button
              type="button"
              disabled={!Number.isFinite(+gridBpm) || +gridBpm / 2 < 30}
              onClick={() => setGridBpm((+gridBpm / 2).toFixed(2))}
            >
              ½ tempo
            </button>
            <button
              type="button"
              disabled={!Number.isFinite(+gridBpm) || +gridBpm * 2 > 300}
              onClick={() => setGridBpm((+gridBpm * 2).toFixed(2))}
            >
              2× tempo
            </button>
          </div>
          <label>
            FIRST BEAT (s)
            <input
              aria-label={`Deck ${id + 1} first beat offset`}
              type="number"
              min={0}
              step={0.01}
              value={gridOffset}
              onChange={(e) => setGridOffset(e.target.value)}
            />
          </label>
          <button>
            <Check size={12} />
            Apply grid
          </button>
        </form>
      )}
      <div className="deck-track-preview">
        <Waveform
          samples={track?.waveform || deck.waveform || []}
          bands={track?.waveform_bands}
          position={position}
          duration={duration}
          bpm={baseBpm}
          spanBeats={visibleBeats}
          color={color}
          overview
          playing={!!deck.playing}
          playbackRate={val(
            deck.playback_rate,
            1 + val(deck.tempo_percent) / 100,
          )}
          jogTouch={!!deck.jog_touch}
          positionRevision={deck.position_revision}
          sourceId={track?.path}
          snapshotTime={snapshotTime}
          hotcues={hotcues}
          memoryCues={deck.memory_cues || []}
          cuePoint={deck.cue_seconds}
          loopStart={deck.loop_start_seconds}
          loopEnd={deck.loop_end_seconds}
          loopActive={deck.loop_active}
          beatgridOffset={val(deck.beatgrid_offset_seconds)}
          onSeek={(seconds) =>
            send("seek", {
              seconds: snapSeek
                ? snapToBeat(
                    seconds,
                    baseBpm,
                    val(deck.beatgrid_offset_seconds),
                    duration,
                  )
                : seconds,
            })
          }
        />
        <div className="preview-caption">
          <span>FULL TRACK</span>
          <span>
            {track?.waveform_bands ? (
              <>
                <i style={{ background: BAND_COLORS.low }} />
                LOW <i style={{ background: BAND_COLORS.mid }} />
                MID <i style={{ background: BAND_COLORS.high }} />
                HIGH
              </>
            ) : (
              "Load a track to preview"
            )}
          </span>
        </div>
      </div>
      <div className="deck-controls">
        <div
          className="jog"
          onPointerDown={(e) => {
            if (!title || e.button !== 0) return;
            e.preventDefault();
            e.currentTarget.setPointerCapture(e.pointerId);
            jogGesture.current = {
              source: track?.path,
              target: position,
              x: e.clientX,
              lastAt: performance.now(),
              pending: false,
            };
            setScratch(true);
            enqueueJog({ type: "jog_touch", touched: true }, track?.path);
          }}
          onPointerMove={(e) => {
            const gesture = jogGesture.current;
            if (!gesture || !e.currentTarget.hasPointerCapture(e.pointerId))
              return;
            if (currentJogSource.current !== gesture.source) {
              finishJog();
              return;
            }
            gesture.target = Math.max(
              0,
              Math.min(
                duration,
                gesture.target + (e.clientX - gesture.x) * 0.012,
              ),
            );
            gesture.x = e.clientX;
            gesture.pending = true;
            if (jogFrame.current === null)
              jogFrame.current = requestAnimationFrame(() => {
                jogFrame.current = null;
                flushJog();
              });
          }}
          onPointerUp={(e) => {
            finishJog();
            if (e.currentTarget.hasPointerCapture(e.pointerId))
              e.currentTarget.releasePointerCapture(e.pointerId);
          }}
          onPointerCancel={finishJog}
          onLostPointerCapture={finishJog}
          role="slider"
          aria-label={`Deck ${id + 1} jog wheel`}
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
              e.preventDefault();
              send("jog", {
                delta_seconds: e.key === "ArrowLeft" ? -0.05 : 0.05,
                scratch: false,
              });
            }
          }}
        >
          <div className={`jog-surface ${scratch ? "touching" : ""}`}>
            <AnatomyDial platter />
            <div className="jog-center">
              <strong>{bpm ? bpm.toFixed(2) : "—"}</strong>
              <span>BPM</span>
              <small>
                {scratch
                  ? "SCRATCH"
                  : `${tempo >= 0 ? "+" : ""}${tempo.toFixed(2)}%`}
              </small>
            </div>
            <i style={{ transform: `rotate(${(position * 90) % 360}deg)` }} />
          </div>
        </div>
        <div className="transport-column">
          <div className="sync-row">
            <button
              disabled={!title}
              className={`mini ${deck.sync_enabled ? "sync-on" : ""}`}
              onClick={() => send("set_sync", { enabled: !deck.sync_enabled })}
            >
              <Link2 size={12} /> SYNC
            </button>
            <button
              className={`master-tag ${deck.is_master ? "master-on" : ""}`}
              disabled={!title}
              onClick={() => send("set_master")}
            >
              MASTER
            </button>
          </div>
          <div className="transport">
            <button
              title="Hold to preview from cue point"
              className="cue"
              disabled={!title}
              onPointerDown={(e) => {
                e.currentTarget.setPointerCapture(e.pointerId);
                send("cue_press", { pressed: true });
              }}
              onPointerUp={() => send("cue_press", { pressed: false })}
              onPointerCancel={() => send("cue_press", { pressed: false })}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  if (!e.repeat) send("cue_press", { pressed: true });
                }
              }}
              onKeyUp={(e) => {
                if (e.key === "Enter" || e.key === " ")
                  send("cue_press", { pressed: false });
              }}
            >
              CUE
            </button>
            <button
              className={`play ${deck.playing ? "active" : ""}`}
              aria-label={deck.playing ? "Pause" : "Play"}
              disabled={!title}
              onClick={() => send("toggle_play")}
            >
              {deck.playing ? (
                <Pause size={20} fill="currentColor" />
              ) : (
                <Play size={20} fill="currentColor" />
              )}
            </button>
          </div>
          <button
            className="set-cue"
            disabled={!title}
            onClick={() => send("set_cue")}
          >
            SET CUE AT {fmt(position)}
          </button>
          <div className="beat-jump">
            <span>BEAT JUMP</span>
            <button
              disabled={!title}
              title="Back 4 beats"
              onClick={() => send("beat_jump", { beats: -4 })}
            >
              <SkipBack size={12} />4
            </button>
            <button
              disabled={!title}
              title="Forward 4 beats"
              onClick={() => send("beat_jump", { beats: 4 })}
            >
              4<ChevronRight size={14} />
            </button>
          </div>
        </div>
        <div className="tempo-slider">
          <Range
            label="TEMPO"
            min={Math.max(-90, Math.min(-range, tempo))}
            max={Math.max(range, tempo)}
            step={0.01}
            value={tempo}
            onChange={(percent) => send("set_tempo", { percent })}
            vertical
          />
          <button
            className="range-pill"
            title="Tempo range · Shift + SYNC on FLX4 cycles this range"
            onClick={() => {
              const next =
                range === 6 ? 10 : range === 10 ? 16 : range === 16 ? 100 : 6;
              setRange(next);
              onTempoRange(id, next);
            }}
          >
            {range === 100 ? "WIDE" : `±${range}%`}
          </button>
          <button
            className="tempo-reset"
            onClick={() => send("set_tempo", { percent: 0 })}
          >
            RESET
          </button>
        </div>
      </div>
      {hardwarePadMode && (
        <div className="controller-pad-mode">
          FLX4 PADS · {controllerLabel(hardwarePadMode)}
        </div>
      )}
      <div className="pads-heading">
        <div className="tabs">
          <button
            className={padMode === "hotcue" ? "selected" : ""}
            onClick={() => setPadMode("hotcue")}
          >
            HOT CUE
          </button>
          <button
            className={padMode === "loop" ? "selected" : ""}
            onClick={() => setPadMode("loop")}
          >
            BEAT LOOP
          </button>
          <button
            className={padMode === "memory" ? "selected" : ""}
            onClick={() => setPadMode("memory")}
          >
            MEMORY
          </button>
        </div>
        <span>
          {padMode === "hotcue"
            ? "Shift + click to clear"
            : padMode === "loop"
              ? "Tap a length to loop"
              : `${deck.memory_cues?.filter((c) => c !== null).length || 0} / 10 saved`}
        </span>
      </div>
      {padMode === "memory" ? (
        <div className="memory-controls">
          <button
            disabled={!title}
            title="Previous memory cue"
            onClick={() => send("memory_cue_previous")}
          >
            <SkipBack size={13} />
            PREV
          </button>
          <button disabled={!title} onClick={() => send("memory_cue_save")}>
            <Plus size={13} />
            SAVE
          </button>
          <button
            disabled={!title}
            title="Delete nearby memory cue"
            onClick={() => send("memory_cue_delete")}
          >
            <X size={13} />
            DELETE
          </button>
          <button
            disabled={!title}
            title="Next memory cue"
            onClick={() => send("memory_cue_next")}
          >
            NEXT
            <ChevronRight size={13} />
          </button>
        </div>
      ) : (
        <div className="pads">
          {Array.from({ length: 8 }, (_, i) => {
            const beats = [0.25, 0.5, 1, 2, 4, 8, 16, 32][i];
            const cue = hotcues[i];
            const active =
              padMode === "hotcue"
                ? cue !== null && cue !== undefined
                : Math.abs(activeLoopBeats - beats) < beats * 0.025;
            return (
              <button
                key={i}
                disabled={!title}
                className={active ? "set" : ""}
                style={
                  padMode === "hotcue"
                    ? ({
                        "--cue-color": HOT_CUE_COLORS[i],
                      } as React.CSSProperties)
                    : undefined
                }
                onClick={(e) => {
                  if (padMode === "hotcue" && e.shiftKey) {
                    send("clear_hot_cue", { index: i });
                    return;
                  }
                  if (padMode === "hotcue")
                    send(cue != null ? "trigger_hot_cue" : "set_hot_cue", {
                      index: i,
                    });
                  else {
                    send("set_loop", {
                      beats:
                        Math.abs(activeLoopBeats - beats) < beats * 0.025
                          ? null
                          : beats,
                    });
                  }
                }}
              >
                <span>
                  {padMode === "hotcue"
                    ? String.fromCharCode(65 + i)
                    : `${beats}`}
                </span>
                <small>
                  {padMode === "hotcue"
                    ? cue != null
                      ? fmt(cue)
                      : "EMPTY"
                    : "BEATS"}
                </small>
              </button>
            );
          })}
        </div>
      )}
      <div className="deck-footer">
        <span>
          <span className={`tiny-dot ${title ? "live" : ""}`} />
          {title ? "TRACK LOADED" : "STANDBY"}
        </span>
        <div className="manual-loop">
          <button
            disabled={!title}
            title="Set loop in point"
            onClick={() => send("loop_in")}
          >
            IN
          </button>
          <button
            disabled={!title}
            title="Set loop out point"
            onClick={() => send("loop_out")}
          >
            OUT
          </button>
          <button
            disabled={deck.loop_start_seconds == null}
            className={deck.loop_start_seconds != null ? "loop-on" : ""}
            onClick={() => send("set_loop", { beats: null })}
          >
            <Repeat2 size={12} />{" "}
            {deck.loop_start_seconds != null ? "EXIT" : "LOOP"}
          </button>
        </div>
        <div className="pitch-tools">
          <button
            disabled={!title || deck.preparation_pending}
            className={deck.keylock ? "keylock enabled" : "keylock"}
            title="Preserve pitch when changing tempo"
            onClick={() => send("set_keylock", { enabled: !deck.keylock })}
          >
            <LockKeyhole size={10} />
            {deck.preparation_pending ? "PREPARING…" : "KEY LOCK"}
          </button>
          <select
            aria-label={`Deck ${id + 1} pitch shift`}
            disabled={!title || deck.preparation_pending}
            value={val(deck.pitch_semitones)}
            onChange={(e) => send("set_pitch", { semitones: +e.target.value })}
          >
            {Array.from({ length: 25 }, (_, i) => i - 12).map((n) => (
              <option value={n} key={n}>
                {n > 0 ? "+" : ""}
                {n} st
              </option>
            ))}
          </select>
        </div>
      </div>
    </section>
  );
}
function Mixer({
  engine,
  command,
  controllerSurface,
}: {
  engine: EngineSnapshot;
  command: (c: Command) => void;
  controllerSurface?: ControllerSurface;
}) {
  const [fxType, setFxType] = useState<"echo" | "reverb">("echo");
  useControl(
    "mixer",
    { "fx-view": (v) => setFxType(controlEnum(v, ["echo", "reverb"])) },
    { fx_view: fxType },
  );
  return (
    <section className="mixer">
      <div className="mixer-title">
        <SlidersHorizontal size={13} /> MIXER <span>2 CHANNEL</span>
      </div>
      {controllerSurface && (
        <div className="controller-fx-status" aria-label="FLX4 effects status">
          <small>
            FLX4 FX · TARGET {controllerSurface.fx_target === 0 ? "A" : "B"}
          </small>
          {controllerSurface.fx_name.map((name, i) => (
            <span
              key={i}
              className={controllerSurface.fx_enabled[i] ? "enabled" : ""}
            >
              <b>{i === 0 ? "A" : "B"}</b> {controllerLabel(name)} ·{" "}
              {controllerSurface.fx_beats[i]} beat ·{" "}
              {controllerSurface.fx_enabled[i] ? "ON" : "OFF"}
            </span>
          ))}
          <small>
            FX depth {Math.round(controllerSurface.fx_depth * 100)}% · CFX{" "}
            {controllerLabel(controllerSurface.smart_cfx_preset)}
          </small>
        </div>
      )}
      <div className="channel-pair">
        {engine.decks.map((deck, i) => (
          <div className={`channel channel-${i}`} key={i}>
            <span className="channel-label">{i === 0 ? "A" : "B"}</span>
            <Knob
              label="TRIM"
              value={val(deck.gain, 1)}
              onChange={(gain) =>
                command({ type: "set_deck_gain", deck: i, gain })
              }
            />
            <Knob
              label="HIGH"
              value={val(deck.eq_high, 1)}
              onChange={(high) =>
                command({
                  type: "set_eq",
                  deck: i,
                  low: val(deck.eq_low, 1),
                  mid: val(deck.eq_mid, 1),
                  high,
                })
              }
            />
            <Knob
              label="MID"
              value={val(deck.eq_mid, 1)}
              onChange={(mid) =>
                command({
                  type: "set_eq",
                  deck: i,
                  low: val(deck.eq_low, 1),
                  mid,
                  high: val(deck.eq_high, 1),
                })
              }
            />
            <Knob
              label="LOW"
              value={val(deck.eq_low, 1)}
              onChange={(low) =>
                command({
                  type: "set_eq",
                  deck: i,
                  low,
                  mid: val(deck.eq_mid, 1),
                  high: val(deck.eq_high, 1),
                })
              }
            />
            <Knob
              label="FILTER"
              min={-1}
              max={1}
              reset={0}
              value={val(deck.filter)}
              onChange={(amount) =>
                command({ type: "set_filter", deck: i, amount })
              }
            />
            <button
              className={`headphone-button ${deck.cue_enabled ? "enabled" : ""}`}
              aria-label={`Headphone cue deck ${i + 1}`}
              onClick={() =>
                command({
                  type: "set_headphone_cue",
                  deck: i,
                  enabled: !deck.cue_enabled,
                })
              }
            >
              <Headphones size={15} /> CUE
            </button>
            <div className="fader-row">
              <Range
                label="LEVEL"
                min={0}
                max={1}
                value={val(deck.channel_fader, 1)}
                onChange={(value) =>
                  command({ type: "set_channel_fader", deck: i, value })
                }
                vertical
              />
              <Meter value={Math.max(...(deck.peak || [0, 0]))} />
            </div>
          </div>
        ))}
      </div>
      <div className="crossfader">
        <div>
          <span>A</span>
          <span>CROSSFADER</span>
          <span>B</span>
        </div>
        <input
          aria-label="Crossfader"
          type="range"
          min={-1}
          max={1}
          step={0.01}
          value={val(engine.crossfader)}
          onChange={(e) =>
            command({ type: "set_crossfader", position: +e.target.value })
          }
          onDoubleClick={() => command({ type: "set_crossfader", position: 0 })}
        />
      </div>
      <div className="master-controls">
        <Knob
          label="MASTER"
          max={2}
          value={val(engine.master_gain, 1)}
          onChange={(gain) => command({ type: "set_master_gain", gain })}
        />
        <Knob
          label="PHONES"
          max={2}
          value={val(engine.cue_gain, 1)}
          onChange={(gain) => command({ type: "set_cue_gain", gain })}
        />
        <Knob
          label="CUE / MIX"
          max={1}
          reset={0}
          value={val(engine.cue_mix)}
          onChange={(mix) => command({ type: "set_cue_mix", mix })}
        />
      </div>
      <div className="fx-row">
        <button
          className="fx-selector"
          onClick={() => setFxType(fxType === "echo" ? "reverb" : "echo")}
        >
          <Zap size={12} />
          {fxType.toUpperCase()}
          <ChevronDown size={10} />
        </button>
        {engine.decks.map((d, i) => (
          <input
            key={i}
            aria-label={`Deck ${i + 1} ${fxType} amount`}
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={val(d[fxType])}
            onChange={(e) =>
              command({
                type: "set_fx",
                deck: i,
                echo: fxType === "echo" ? +e.target.value : val(d.echo),
                reverb: fxType === "reverb" ? +e.target.value : val(d.reverb),
              })
            }
          />
        ))}
      </div>
    </section>
  );
}
export default function App() {
  const [theme, setTheme] = useState<ThemeId>(readTheme);
  useEffect(() => {
    applyTheme(theme);
    saveTheme(theme);
  }, [theme]);
  const [smartModes, setSmartModes] = useState<AppState["smart_modes"]>();
  const [recording, setRecording] = useState<RecordingStatus>({});
  const [sourceJobs, setSourceJobs] = useState<
    NonNullable<AppState["spotify_decks"]>
  >([]);
  const [librarySource, setLibrarySource] = useState<"local" | "spotify">(
    "local",
  );
  const [drawerOpen, updateDrawerOpen] = useState(false);
  const drawerOpenRef = useRef(false);
  const [browserNavigation, updateBrowserNavigation] =
    useState<BrowserNavigation>({ layer: "home", returning: false });
  const browserNavigationRef = useRef(browserNavigation);
  const navigate = useCallback((event: BrowserNavigationEvent) => {
    const next = navigateBrowser(browserNavigationRef.current, event);
    browserNavigationRef.current = next;
    updateBrowserNavigation(next);
    return next;
  }, []);
  const setDrawerOpen = useCallback(
    (value: boolean | ((open: boolean) => boolean)) => {
      const open =
        typeof value === "function" ? value(drawerOpenRef.current) : value;
      drawerOpenRef.current = open;
      updateDrawerOpen(open);
      navigate(open ? "open" : "close");
    },
    [navigate],
  );
  const [visibleBeats, setVisibleBeats] = useState(32);
  const [snapSeek, setSnapSeek] = useState(false);
  const [showSampler, setShowSampler] = useState(false);
  const [samplerBank, setSamplerBank] = useState(0);
  const [sampleNames, setSampleNames] = useState<string[]>([
    "Kick",
    "Snare",
    "Closed hat",
    "Clap",
    "Bass",
    "Percussion",
    "Rim",
    "Crash",
    "Sample 9",
    "Sample 10",
    "Sample 11",
    "Sample 12",
    "Sample 13",
    "Sample 14",
    "Sample 15",
    "Sample 16",
  ]);
  const [analysis, setAnalysis] = useState<AppState["analysis"]>();
  const [engine, setEngine] = useState<EngineSnapshot>(emptyEngine);
  const [snapshotTime, setSnapshotTime] = useState(0);
  const trackMetadata = useRef<(TrackRecord | null)[]>([null, null]);
  const metadataRevision = useRef(-1);

  const [library, setLibrary] = useState<TrackRecord[]>([]);
  const [playlists, setPlaylists] = useState<Playlist[]>([]);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [midi, setMidi] = useState<MidiState>({});
  const [controllerSurface, setControllerSurface] =
    useState<ControllerSurface>();
  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    const stops: UnlistenFn[] = [];
    const register = (stop: UnlistenFn) => {
      if (disposed) stop();
      else stops.push(stop);
    };
    listen<ControllerSurface>("controller_surface", (event) => {
      if (!disposed) setControllerSurface(event.payload);
    })
      .then(register)
      .catch(() => {});
    listen<{ deck: number; range: number }>(
      "controller_tempo_range",
      (event) => {
        const { deck, range } = event.payload;
        if (deck !== 0 && deck !== 1) return;
        setControllerSurface((old) => {
          if (!old) return old;
          const tempo_ranges: [number, number] = [...old.tempo_ranges];
          tempo_ranges[deck] = range;
          return { ...old, tempo_ranges };
        });
      },
    ).then(register);
    native<ControllerSurface>("get_controller_surface")
      .then((surface) => {
        if (!disposed) setControllerSurface(surface);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, []);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [selection, updateSelection] = useState<string | number | null>(null);
  const selectedLocalRow = useRef<string | number | null>(null);
  const setSelection = useCallback((id: string | number | null) => {
    selectedLocalRow.current = id;
    updateSelection(id);
  }, []);
  const [playlist, updatePlaylist] = useState<string | number | null>(null);
  const selectedLocalPlaylist = useRef<string | number | null>(null);
  const setPlaylist = useCallback((id: string | number | null) => {
    selectedLocalPlaylist.current = id;
    updatePlaylist(id);
  }, []);
  const [localCollectionFocus, updateLocalCollectionFocus] = useState(true);
  const localCollectionFocusRef = useRef(true);
  const setLocalCollectionFocus = useCallback(
    (value: boolean, followNavigation = true) => {
      localCollectionFocusRef.current = value;
      updateLocalCollectionFocus(value);
      if (followNavigation) navigate(value ? "playlists" : "tracks");
    },
    [navigate],
  );
  const spotifyBrowser = useRef<ControllerBrowserHandle | null>(null);
  const controllerHandler = useRef<
    (action: ControllerBrowserAction) => void | Promise<void>
  >(() => {});
  const deckLoadRequest = useRef([0, 0]);
  const lastControllerLoad = useRef<{ deck: number; time: number } | null>(
    null,
  );
  const registerSpotifyBrowser = useCallback(
    (handle: ControllerBrowserHandle | null) => {
      spotifyBrowser.current = handle;
    },
    [],
  );
  const [settings, setSettings] = useState(false);
  const [settingsTab, setSettingsTab] = useState("audio");
  const [device, setDevice] = useState("");
  const [buffer, setBuffer] = useState(256);
  const [masterL, setMasterL] = useState(0);
  const [masterR, setMasterR] = useState(1);
  const [cueL, setCueL] = useState(2);
  const [cueR, setCueR] = useState(3);
  const [cueRoute, setCueRoute] = useState(false);
  const audioSettingsHydrated = useRef(false);
  const audioSettingsTouched = useRef(false);
  const audioChangePending = useRef(false);
  const [audioChanging, setAudioChanging] = useState(false);
  const [midiInput, setMidiInput] = useState("");
  const [midiOutput, setMidiOutput] = useState("");
  const [midiInputs, setMidiInputs] = useState<string[]>([]);
  const [midiOutputs, setMidiOutputs] = useState<string[]>([]);
  const [newPlaylist, setNewPlaylist] = useState(false);
  const [playlistName, setPlaylistName] = useState("");
  const [sort, setSort] = useState<"title" | "bpm" | "duration">("title");
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const fail = useCallback((error: unknown) => {
    setNotice({
      text: String(error instanceof Error ? error.message : error),
      error: true,
    });
  }, []);
  const announce = useCallback((text: string) => {
    setNotice({ text });
    clearTimeout(noticeTimer.current);
    noticeTimer.current = setTimeout(() => setNotice(null), 5000);
  }, []);
  const hydrateAudio = useCallback((audio: AppState["audio_status"]) => {
    if (audioSettingsHydrated.current || !audio) return;
    audioSettingsHydrated.current = true;
    if (audioSettingsTouched.current) return;
    setDevice(audio.device_name);
    setBuffer(audio.buffer_frames);
    setMasterL(audio.routing.master_left);
    setMasterR(audio.routing.master_right);
    setCueL(audio.routing.cue_left ?? 2);
    setCueR(audio.routing.cue_right ?? 3);
    setCueRoute(
      audio.routing.cue_left !== null && audio.routing.cue_right !== null,
    );
  }, []);
  const refresh = useCallback(async () => {
    if (!isDesktop) return;
    try {
      const s = await native<AppState>("get_app_state");
      trackMetadata.current = s.engine.decks.map((d) => d.track ?? null);
      metadataRevision.current = -1;
      setSnapshotTime(performance.now());
      setEngine(s.engine);
      setLibrary(s.library);
      setPlaylists(s.playlists);
      setDevices(s.audio_devices);
      setMidi(s.midi);
      setSmartModes(s.smart_modes);
      setSourceJobs(s.spotify_decks || []);
      setAnalysis(s.analysis);
      hydrateAudio(s.audio_status);
      setDevice(
        (d) =>
          d ||
          s.audio_status?.device_name ||
          s.audio_devices.find((d) => d.is_default)?.name ||
          s.audio_devices[0]?.name ||
          "",
      );
    } catch (e) {
      fail(e);
    }
  }, [fail, hydrateAudio]);
  useEffect(() => {
    refresh();
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        if (isDesktop && !document.hidden) {
          const s = await native<EngineTransport>("get_engine_transport");
          const receivedAt = performance.now();
          if (s.metadata_revision !== metadataRevision.current) {
            const metadata = await native<EngineMetadata>(
              "get_engine_metadata",
            );
            if (cancelled) return;
            trackMetadata.current = metadata.tracks;
            if (metadata.samples)
              setSampleNames(
                metadata.samples.map(
                  (sample, i) => sample?.title || `Sample ${i + 1}`,
                ),
              );
            metadataRevision.current = metadata.revision;
          }
          if (!cancelled) {
            setSnapshotTime(receivedAt);
            setEngine(withTrackMetadata(s.engine, trackMetadata.current));
          }
        }
      } catch (e) {
        if (!cancelled) {
          fail(e);
          timer = setTimeout(poll, 1000);
        }
        return;
      }
      if (!cancelled) timer = setTimeout(poll, 33);
    };
    poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [refresh, fail]);
  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    const timer = setInterval(() => {
      native<
        Pick<
          AppState,
          | "midi"
          | "analysis"
          | "smart_modes"
          | "spotify_decks"
          | "audio_status"
          | "audio_devices"
        >
      >("get_activity_state")
        .then((activity) => {
          if (disposed) return;
          setMidi(activity.midi);
          setAnalysis(activity.analysis);
          setSmartModes(activity.smart_modes);
          setSourceJobs(activity.spotify_decks || []);
          hydrateAudio(activity.audio_status);
          setDevices(activity.audio_devices);
        })
        .catch(fail);
    }, 1500);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, [fail]);
  const waveformCommand = useCallback(
    (c: Command) =>
      native<void>("engine_command", { command: c }).catch((error) => {
        fail(error);
        throw error;
      }),
    [fail],
  );
  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let stop: UnlistenFn | undefined;
    listen("library-updated", () => {
      void refresh();
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(fail);
    return () => {
      disposed = true;
      stop?.();
    };
  }, [refresh, fail]);
  const command = useCallback(
    (c: Command) => {
      native("engine_command", { command: c }).catch(fail);
    },
    [fail],
  );
  const setControllerTempoRange = useCallback(
    (deck: number, range: number) => {
      if (isDesktop)
        native("set_controller_tempo_range", { deck, range }).catch(fail);
    },
    [fail],
  );
  const loadSpotify = useCallback(
    async (deck: number, track: SpotifyTrack) => {
      const request = ++deckLoadRequest.current[deck];
      setSourceJobs((old) =>
        [0, 1].map((i) =>
          i === deck
            ? { preparing: true, uri: track.uri }
            : old[i] || { preparing: false },
        ),
      );
      try {
        await native("load_spotify_track", { deck, uri: track.uri });
        if (deckLoadRequest.current[deck] !== request) return;
        await refresh();
        announce(`Loaded ${track.title} on deck ${deck === 0 ? "A" : "B"}`);
      } catch (error) {
        if (deckLoadRequest.current[deck] !== request) return;
        await refresh();
        throw error;
      }
    },
    [refresh, announce],
  );
  const importTracks = useCallback(async () => {
    setBusy(true);
    try {
      const paths = await native<string[]>("choose_tracks");
      if (paths.length) {
        const tracks = await native<TrackRecord[]>("import_tracks", { paths });
        await refresh();
        announce(
          `Imported ${tracks.length} track${tracks.length === 1 ? "" : "s"}${tracks.length < paths.length ? `; ${paths.length - tracks.length} file(s) failed — see Library settings` : ""}`,
        );
      }
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }, [refresh, announce, fail]);
  const load = useCallback(
    async (deck: number, path: string) => {
      try {
        await native("load_track", { deck, path });
        await refresh();
        announce(`Track loaded to deck ${deck === 0 ? "A" : "B"}`);
      } catch (e) {
        fail(e);
      }
    },
    [refresh, announce, fail],
  );
  const demo = useCallback(
    async (deck: number) => {
      try {
        await native("load_demo", { deck });
        await refresh();
        announce("Generated test tone loaded");
      } catch (e) {
        fail(e);
      }
    },
    [refresh, announce, fail],
  );
  const reanalyze = useCallback(
    async (deck: number) => {
      await native("reanalyze_loaded_track", { deck });
      await refresh();
      announce("Beat, key and waveform analysis updated");
    },
    [refresh, announce],
  );
  const recordingActive = !!(recording.recording || recording.active);
  useEffect(() => {
    if (!isDesktop) return;
    native<RecordingStatus>("get_recording_status")
      .then(setRecording)
      .catch(() => {});
    if (!recordingActive) return;
    const timer = setInterval(async () => {
      try {
        setRecording(await native<RecordingStatus>("get_recording_status"));
      } catch {}
    }, 1000);
    return () => clearInterval(timer);
  }, [recordingActive]);
  async function toggleRecording() {
    try {
      const status = await native<RecordingStatus | null>(
        recordingActive ? "stop_recording" : "start_recording",
      );
      if (status) {
        setRecording(status);
        if (status.error) {
          fail(status.error);
          return;
        }
        if (recordingActive && status.dropped_frames) {
          fail(
            `Recording saved, but ${status.dropped_frames} audio frames were dropped.`,
          );
          return;
        }
        announce(recordingActive ? "Recording saved" : "Recording started");
      }
    } catch (e) {
      fail(e);
    }
  }
  const visible = useMemo(() => {
    const p = playlists.find((p) => p.id === playlist);
    const q = search.toLowerCase();
    return library
      .filter(
        (t) =>
          (!p || p.track_ids.includes(t.id)) &&
          `${t.title} ${t.artist || ""} ${t.bpm || ""} ${t.key || ""} ${(t.tags || []).join(" ")}`
            .toLowerCase()
            .includes(q),
      )
      .sort((a, b) =>
        sort === "title"
          ? a.title.localeCompare(b.title)
          : sort === "bpm"
            ? val(a.bpm) - val(b.bpm)
            : val(a.duration_seconds) - val(b.duration_seconds),
      );
  }, [library, playlists, playlist, search, sort]);
  useEffect(() => {
    if (!isDesktop) return;
    let canceled = false;
    let unlisten: UnlistenFn | undefined;
    listen<number>("controller_selection", (event) => {
      setSelection(event.payload);
      requestAnimationFrame(() =>
        document
          .querySelector(`[data-track-id="${event.payload}"]`)
          ?.scrollIntoView({ block: "nearest" }),
      );
    })
      .then((fn) => {
        if (canceled) fn();
        else unlisten = fn;
      })
      .catch(fail);
    return () => {
      canceled = true;
      unlisten?.();
    };
  }, [fail]);
  controllerHandler.current = async (action) => {
    if (action.type === "global_button" && !action.pressed) return;
    if (action.type !== "load" || action.shifted)
      lastControllerLoad.current = null;
    if (action.type === "global_button" && action.button === "panel_forward") {
      setDrawerOpen((open) => !open);
      return;
    }
    if (action.type === "browse" && action.shifted) {
      setVisibleBeats((old) => zoomBeats(old, action.delta));
      return;
    }
    if (action.type === "load" && action.shifted && action.deck === 1) {
      setDrawerOpen((open) => !open);
      return;
    }
    if (action.type === "browse" && !drawerOpenRef.current) {
      const wasHome = browserNavigationRef.current.layer === "home";
      navigate("rotate");
      if (!wasHome)
        setLibrarySource((source) => {
          const sources = ["spotify", "local"] as const;
          return sources[browseIndex(sources, source, action.delta)];
        });
      return;
    }
    if (action.type === "global_button") {
      const next = navigate(
        action.button === "browse_back" || action.shifted ? "back" : "press",
      );
      const open = next.layer !== "footer" && next.layer !== "home";
      drawerOpenRef.current = open;
      updateDrawerOpen(open);
      if (!open || next.layer === "close") return;
      setLocalCollectionFocus(next.layer === "playlists", false);
      if (librarySource === "spotify") {
        await spotifyBrowser.current?.handle({
          ...action,
          button: next.layer === "playlists" ? "browse_back" : "browse_press",
          shifted: false,
        });
      } else if (
        next.layer === "tracks" &&
        !visible.some((t) => t.id === selectedLocalRow.current)
      ) {
        setSelection(visible[0]?.id ?? null);
      }
      return;
    }
    if (!drawerOpenRef.current) setDrawerOpen(true);
    if (action.type === "browse") {
      const wasClose = browserNavigationRef.current.layer === "close";
      navigate("rotate");
      if (wasClose) {
        setLocalCollectionFocus(true, false);
        await spotifyBrowser.current?.handle({
          type: "global_button",
          button: "browse_back",
          pressed: true,
          shifted: false,
        });
      }
    }
    if (action.type === "load" && !action.shifted) {
      const now = performance.now();
      const previous = lastControllerLoad.current;
      lastControllerLoad.current = { deck: action.deck, time: now };
      if (
        action.instant_double ||
        isInstantDouble(previous, action.deck, now)
      ) {
        deckLoadRequest.current[action.deck]++;
        await native("instant_double", { deck: action.deck });
        await refresh();
        announce(`Instant double on deck ${action.deck === 0 ? "A" : "B"}`);
        return;
      }
      if (engine.decks[action.deck]?.playing) {
        announce(
          `Deck ${action.deck === 0 ? "A" : "B"} is playing. Pause before loading, or double-press LOAD for instant doubles.`,
        );
        return;
      }
    }
    if (librarySource === "spotify") {
      if (spotifyBrowser.current) await spotifyBrowser.current.handle(action);
      else
        announce(
          "Opening the Spotify browser. Select a playlist once the library loads.",
        );
      return;
    }
    if (action.type === "browse") {
      if (localCollectionFocusRef.current) {
        const ids = ["all", ...playlists.map((p) => p.id)];
        const index = browseIndex(
          ids,
          selectedLocalPlaylist.current ?? "all",
          action.delta,
        );
        if (index >= 0) {
          setPlaylist(ids[index] === "all" ? null : ids[index]);
          setSelection(null);
        }
      } else {
        const ids = visible.map((track) => track.id);
        const index = browseIndex(ids, selectedLocalRow.current, action.delta);
        if (index >= 0) {
          setSelection(ids[index]);
          await native("set_controller_selection", { track_id: ids[index] });
        }
      }
    } else if (action.type === "load") {
      const track =
        visible.find((t) => t.id === selectedLocalRow.current) ?? visible[0];
      if (!track) {
        announce("Import local tracks or switch the music source to Spotify.");
        return;
      }
      if (action.shifted) {
        setSearch(track.artist || track.title);
        setPlaylist(null);
        setLocalCollectionFocus(false);
        announce(`Related tracks: ${track.artist || track.title}`);
      } else {
        setSelection(track.id);
        await load(action.deck, track.path);
      }
    }
  };
  useEffect(() => {
    if (browserNavigation.layer === "playlists" && librarySource === "local")
      setLocalCollectionFocus(true, false);
    const frame = requestAnimationFrame(() => {
      if (browserNavigation.layer === "footer")
        document
          .querySelector<HTMLButtonElement>(
            `[data-browser-source="${librarySource}"]`,
          )
          ?.focus({ preventScroll: true });
      else if (
        browserNavigation.layer === "playlists" &&
        librarySource === "local"
      ) {
        document
          .querySelector<HTMLButtonElement>(
            `[data-local-collection="${playlist ?? "all"}"]`,
          )
          ?.focus({ preventScroll: true });
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [
    browserNavigation.layer,
    librarySource,
    playlist,
    setLocalCollectionFocus,
  ]);
  const browserNavigationKey = (event: React.KeyboardEvent) => {
    if (
      !(event.target as HTMLElement).matches(
        "[data-browser-source], [data-local-collection], [data-browser-id], [data-controller-close], [data-browser-pane]",
      )
    )
      return;
    let action: ControllerBrowserAction;
    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key))
      action = {
        type: "browse",
        delta: event.key === "ArrowUp" || event.key === "ArrowLeft" ? -1 : 1,
        shifted: false,
      };
    else if (event.key === "Enter" || event.key === "Backspace")
      action = {
        type: "global_button",
        button: event.key === "Backspace" ? "browse_back" : "browse_press",
        pressed: true,
        shifted: event.shiftKey,
      };
    else return;
    event.preventDefault();
    event.stopPropagation();
    void Promise.resolve(controllerHandler.current(action)).catch(fail);
  };
  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let stop: UnlistenFn | undefined;
    listen<unknown>("controller_action", (event) => {
      if (event.payload && typeof event.payload === "object") {
        const action = event.payload as {
          type?: string;
          mode?: string;
          deck?: number;
          pressed?: boolean;
        };
        if (
          (action.type === "pad_mode" ||
            (action.type === "pad" && action.pressed)) &&
          action.mode === "sampler" &&
          (action.deck === 0 || action.deck === 1)
        ) {
          setSamplerBank(action.deck);
          setShowSampler(true);
        }
      }
      if (isBrowserAction(event.payload))
        void Promise.resolve(controllerHandler.current(event.payload)).catch(
          fail,
        );
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(fail);
    return () => {
      disposed = true;
      stop?.();
    };
  }, [fail]);
  useEffect(() => {
    if (librarySource !== "local" || !drawerOpen || localCollectionFocus)
      return;
    const row = document.querySelector<HTMLElement>(
      `[data-track-id="${selection}"]`,
    );
    scrollBrowserSelection(row);
    if (browserNavigation.layer === "tracks")
      (
        row ??
        document.querySelector<HTMLElement>(
          '[data-browser-pane="local-tracks"]',
        )
      )?.focus({ preventScroll: true });
  }, [
    selection,
    drawerOpen,
    librarySource,
    localCollectionFocus,
    browserNavigation.layer,
  ]);
  useEffect(() => {
    if (librarySource === "local" && drawerOpen && localCollectionFocus)
      scrollBrowserSelection(
        document.querySelector(
          `[data-local-collection="${playlist ?? "all"}"]`,
        ),
      );
  }, [playlist, drawerOpen, librarySource, localCollectionFocus]);
  const selected = library.find((t) => t.id === selection);
  const audioRunning = !!(engine.running || engine.audio_running);
  const currentDevice = devices.find((d) => d.name === device);
  const channels =
    currentDevice?.output_channels || currentDevice?.channels || 2;
  const selectTrack = useCallback(
    (id: string | number) => {
      setSelection(id);
      setLocalCollectionFocus(false);
      native("set_controller_selection", { track_id: id }).catch(fail);
    },
    [fail],
  );
  useEffect(() => {
    if (isDesktop)
      native("set_controller_browse_context", {
        track_ids: visible.map((t) => t.id),
      }).catch(fail);
  }, [visible, fail]);
  const saveTags = useCallback(async (id: string | number, tags: string[]) => {
    const updated = await control<TrackRecord>("library.set_tags", {
      id,
      tags,
    });
    setLibrary((tracks) =>
      tracks.map((t) => (t.id === id ? { ...t, tags: updated.tags } : t)),
    );
  }, []);
  const trackRows = useMemo(
    () =>
      visible.map((track, i) => (
        <tr
          data-track-id={track.id}
          data-browser-id={String(track.id)}
          tabIndex={-1}
          key={track.id}
          className={selection === track.id ? "selected" : ""}
          onClick={() => selectTrack(track.id)}
          draggable
          onDragStart={(e) =>
            e.dataTransfer.setData("application/pulsedeck-track", track.path)
          }
          onDoubleClick={() =>
            load(engine.decks[0]?.playing ? 1 : 0, track.path)
          }
        >
          <td className="track-number">{String(i + 1).padStart(2, "0")}</td>
          <td>
            <div className="track-cell">
              <div className="track-art">
                <Music2 size={17} />
              </div>
              <div>
                <strong>{track.title}</strong>
                <span>
                  {track.artist ||
                    track.path.split("/").slice(-2, -1)[0] ||
                    "Local audio"}
                </span>
              </div>
            </div>
          </td>
          <td className="numeric">{track.bpm ? track.bpm.toFixed(1) : "—"}</td>
          <td>
            <span className="key-tag">{track.key || "—"}</span>
          </td>
          <td className="numeric">{fmt(val(track.duration_seconds))}</td>
          <td>
            <div className="load-buttons">
              <button
                className="load-a"
                title="Load deck A"
                onClick={(e) => {
                  e.stopPropagation();
                  load(0, track.path);
                }}
              >
                A
              </button>
              <button
                className="load-b"
                title="Load deck B"
                onClick={(e) => {
                  e.stopPropagation();
                  load(1, track.path);
                }}
              >
                B
              </button>
            </div>
          </td>
          <td>
            <TrackTags
              title={track.title}
              tags={track.tags || []}
              save={(tags) => saveTags(track.id, tags)}
            />
          </td>
        </tr>
      )),
    [visible, selection, load, selectTrack, saveTags],
  );
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.code === "Escape") {
        setSettings(false);
        setDrawerOpen(false);
        return;
      }
      if (
        (e.target as HTMLElement).matches(
          "input,textarea,select,button,summary,a",
        ) ||
        e.defaultPrevented ||
        settings
      )
        return;
      if (
        (drawerOpenRef.current ||
          browserNavigationRef.current.layer === "footer") &&
        (e.code === "Enter" || e.code === "Backspace")
      ) {
        e.preventDefault();
        void Promise.resolve(
          controllerHandler.current({
            type: "global_button",
            button: e.code === "Backspace" ? "browse_back" : "browse_press",
            pressed: true,
            shifted: e.shiftKey,
          }),
        ).catch(fail);
        return;
      }
      if (e.code === "Space") {
        e.preventDefault();
        command({ type: "toggle_play", deck: 0 });
      }
      if (e.code === "Enter") {
        e.preventDefault();
        command({ type: "toggle_play", deck: 1 });
      }
      if (e.code === "KeyI" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        importTracks();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [command, settings, importTracks, librarySource, fail]);
  async function refreshPorts() {
    try {
      const ports = await native<{ inputs: string[]; outputs: string[] }>(
        "list_midi_ports",
      );
      setMidiInputs(ports.inputs);
      setMidiOutputs(ports.outputs);
      setMidiInput((v) => v || ports.inputs[0] || "");
      setMidiOutput((v) => v || ports.outputs[0] || "");
    } catch (e) {
      fail(e);
    }
  }
  async function openSettings() {
    setSettings(true);
    const results = await Promise.allSettled([
      native<AudioDeviceInfo[]>("list_audio_devices").then(setDevices),
      refreshPorts(),
    ]);
    for (const result of results)
      if (result.status === "rejected" && isDesktop) fail(result.reason);
  }
  async function startAudio() {
    if (audioChangePending.current) return;
    audioChangePending.current = true;
    setAudioChanging(true);
    try {
      await native("start_audio", {
        device_name: device || null,
        buffer_frames: buffer,
        routing: {
          master_left: masterL,
          master_right: masterR,
          cue_left: cueRoute ? cueL : null,
          cue_right: cueRoute ? cueR : null,
        },
      });
      await refresh();
      announce("Audio engine started");
    } catch (e) {
      fail(e);
    } finally {
      audioChangePending.current = false;
      setAudioChanging(false);
    }
  }
  useControl(
    "app",
    {
      theme: (v) =>
        setTheme(
          controlEnum(v, ["hardware-dark", "anatomy-light", "anatomy-dark"]),
        ),
      source: (v) => setLibrarySource(controlEnum(v, ["local", "spotify"])),
      browser: (v) => setDrawerOpen(controlBoolean(v)),
      "browser-toggle": () => setDrawerOpen((v) => !v),
      search: (v) => setSearch(controlString(v)),
      sort: (v) => setSort(controlEnum(v, ["title", "bpm", "duration"])),
      selection: async (v) => {
        const raw =
          typeof v === "number"
            ? v
            : Number(controlString(v).replace(/^local:/, ""));
        const track = visible.find((t) => t.id === raw);
        if (!track) throw new Error("Track is not in the visible browser");
        setSelection(track.id);
        setLocalCollectionFocus(false);
        await native("set_controller_selection", { track_id: track.id });
      },
      collection: (v) => {
        if (v === null || v === "all") {
          setPlaylist(null);
          return;
        }
        const raw =
          typeof v === "number"
            ? v
            : Number(controlString(v).replace(/^local:/, ""));
        if (!playlists.some((p) => p.id === raw))
          throw new Error("Playlist not found");
        setPlaylist(raw);
        setLocalCollectionFocus(true);
      },
      navigation: (v) =>
        navigate(
          controlEnum(v, [
            "rotate",
            "press",
            "back",
            "open",
            "close",
            "playlists",
            "tracks",
          ]),
        ),
      "controller-browser": (v) => {
        if (!isBrowserAction(v)) throw new Error("Invalid browser action");
        return controllerHandler.current(v);
      },
      zoom: (v) => setVisibleBeats(controlNumber(v, 8, 128)),
      snap: (v) => setSnapSeek(controlBoolean(v)),
      sampler: (v) => setShowSampler(controlBoolean(v)),
      "sampler-bank": (v) => {
        const n = controlNumber(v, 1, 2);
        if (!Number.isInteger(n)) throw new Error("Bank must be 1 or 2");
        setSamplerBank(n - 1);
      },
      "notice-dismiss": () => setNotice(null),
      "playlist-form": (v) => setNewPlaylist(controlBoolean(v)),
      "playlist-name": (v) => setPlaylistName(controlString(v)),
      "playlist-submit": async () => {
        await native("create_playlist", { name: playlistName });
        setNewPlaylist(false);
        setPlaylistName("");
        await refresh();
      },
      settings: async (v) => {
        const open = controlBoolean(v);
        if (open) await openSettings();
        else setSettings(false);
      },
      "settings-tab": (v) =>
        setSettingsTab(
          controlEnum(v, [
            "appearance",
            "audio",
            "midi",
            "library",
            "spotify",
            "roadmap",
          ]),
        ),
      "audio-draft": (v) => {
        const d = controlDraft(v, {
          device: controlString,
          buffer: (v) => controlInteger(v, 16, 8192),
          master_left: (v) => controlInteger(v, 0, 63),
          master_right: (v) => controlInteger(v, 0, 63),
          cue_left: (v) => controlInteger(v, 0, 63),
          cue_right: (v) => controlInteger(v, 0, 63),
          cue_route: controlBoolean,
        });
        audioSettingsTouched.current = true;
        if (d.device !== undefined) setDevice(controlString(d.device));
        if (d.buffer !== undefined)
          setBuffer(controlNumber(d.buffer, 16, 8192));
        if (d.master_left !== undefined)
          setMasterL(controlNumber(d.master_left, 0, 63));
        if (d.master_right !== undefined)
          setMasterR(controlNumber(d.master_right, 0, 63));
        if (d.cue_left !== undefined) setCueL(controlNumber(d.cue_left, 0, 63));
        if (d.cue_right !== undefined)
          setCueR(controlNumber(d.cue_right, 0, 63));
        if (d.cue_route !== undefined) setCueRoute(controlBoolean(d.cue_route));
      },
      "audio-apply": async () => {
        await native("start_audio", {
          device_name: device || null,
          buffer_frames: buffer,
          routing: {
            master_left: masterL,
            master_right: masterR,
            cue_left: cueRoute ? cueL : null,
            cue_right: cueRoute ? cueR : null,
          },
        });
        await refresh();
      },
      "midi-draft": (v) => {
        const d = controlDraft(v, {
          input: controlString,
          output: controlString,
        });
        if (d.input !== undefined) setMidiInput(controlString(d.input));
        if (d.output !== undefined) setMidiOutput(controlString(d.output));
      },
      "midi-apply": async () => {
        setMidi(
          await native("connect_midi", {
            input_name: midiInput,
            output_name: midiOutput || null,
          }),
        );
      },
      "ports-refresh": refreshPorts,
      refresh,
      "import-dialog": importTracks,
      "recording-dialog": toggleRecording,
    },
    {
      theme,
      source: librarySource,
      browser: drawerOpen,
      navigation: browserNavigation,
      search,
      sort,
      selection,
      collection: playlist,
      visible_track_ids: visible.map((t) => t.id),
      zoom: visibleBeats,
      snap: snapSeek,
      sampler: showSampler,
      sampler_bank: samplerBank + 1,
      settings,
      settings_tab: settingsTab,
      playlist_form: newPlaylist,
      playlist_name: playlistName,
      notice,
      audio_draft: {
        device,
        buffer,
        master_left: masterL,
        master_right: masterR,
        cue_left: cueL,
        cue_right: cueR,
        cue_route: cueRoute,
      },
      midi_draft: { input: midiInput, output: midiOutput },
    },
  );
  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">
            <AudioLines size={22} />
          </div>
          <strong>PulseDeck</strong>
          <span className="version">0.1</span>
        </div>
        <div className="topbar-center">
          <span className="workspace-label">PERFORMANCE</span>
          <span className="native-tag">RUST AUDIO ENGINE</span>
        </div>
        <div className="top-actions">
          <button
            className={`record-button ${recordingActive ? "recording" : ""}`}
            disabled={
              (!audioRunning ||
                engine.decks.some(
                  (d) => d.track?.recording_allowed === false,
                )) &&
              !recordingActive
            }
            onClick={toggleRecording}
            title={
              engine.decks.some((d) => d.track?.recording_allowed === false)
                ? "Recording is unavailable while a Spotify source is loaded"
                : "Record the native deck and sampler mix as WAV"
            }
          >
            {recordingActive ? (
              <Square size={10} fill="currentColor" />
            ) : (
              <Circle size={10} fill="currentColor" />
            )}
            {recordingActive ? "STOP REC" : "REC"}
          </button>
          <button
            className={`sampler-toggle ${showSampler ? "selected" : ""}`}
            onClick={() => setShowSampler(!showSampler)}
          >
            SAMPLER
          </button>
          <span
            className={`connection-chip ${audioRunning ? "connected" : ""}`}
          >
            <span className="tiny-dot" />
            {audioRunning ? "Audio online" : "Audio stopped"}
          </span>
          <button
            className={`controller-chip ${midi.connected ? "connected" : ""}`}
            onClick={() => {
              openSettings();
              setSettingsTab("midi");
            }}
          >
            <Radio size={13} />
            {midi.connected ? "MIDI connected" : "Controller offline"}
          </button>
          <button
            className="icon-button"
            aria-label="Settings"
            onClick={openSettings}
          >
            <Settings2 size={18} />
          </button>
          <label className="source-picker">
            <span>SOURCE</span>
            <select
              aria-label="Music source"
              value={librarySource}
              onChange={(e) => {
                setLibrarySource(e.target.value as "local" | "spotify");
                setDrawerOpen(true);
              }}
            >
              <option value="local">Local tracks</option>
              <option value="spotify">Spotify</option>
            </select>
          </label>
          <button
            className="import-button"
            onClick={importTracks}
            disabled={busy}
          >
            {busy ? (
              <LoaderCircle size={15} className="spin" />
            ) : (
              <Plus size={16} />
            )}{" "}
            {busy
              ? `Analyzing${analysis?.active ? ` ${analysis.completed}/${analysis.total}` : "…"}`
              : "Import tracks"}
          </button>
        </div>
      </header>
      <div className="workspace">
        <main>
          <section
            className="stacked-waveforms"
            aria-label="Stacked deck waveforms"
          >
            <div className="waveform-tools">
              <div className="linked-zoom">
                <button
                  aria-label="Zoom both waveforms out"
                  disabled={visibleBeats >= 128}
                  onClick={() => setVisibleBeats((n) => Math.min(128, n * 2))}
                >
                  <Minus size={13} />
                </button>
                <span>{visibleBeats / 4} bars</span>
                <button
                  aria-label="Zoom both waveforms in"
                  disabled={visibleBeats <= 8}
                  onClick={() => setVisibleBeats((n) => Math.max(8, n / 2))}
                >
                  <Plus size={13} />
                </button>
                <button
                  className="reset-wave-zoom"
                  onClick={() => setVisibleBeats(32)}
                >
                  RST
                </button>
              </div>
              <WaveformAlignment decks={engine.decks} />
              <button
                className={`snap-seek ${snapSeek ? "enabled" : ""}`}
                title="Snap waveform seeks to the nearest beat"
                onClick={() => setSnapSeek((v) => !v)}
              >
                SNAP SEEK
              </button>
            </div>
            <DeckWaveform
              id={0}
              deck={engine.decks[0] || {}}
              library={library}
              command={command}
              visibleBeats={visibleBeats}
              snapSeek={snapSeek}
              waveformCommand={waveformCommand}
              snapshotTime={snapshotTime}
            />
            <DeckWaveform
              id={1}
              deck={engine.decks[1] || {}}
              library={library}
              command={command}
              visibleBeats={visibleBeats}
              snapSeek={snapSeek}
              waveformCommand={waveformCommand}
              snapshotTime={snapshotTime}
            />
          </section>
          {
            <div className="performance-area">
              <Deck
                id={0}
                hardwareRange={controllerSurface?.tempo_ranges[0]}
                onTempoRange={setControllerTempoRange}
                hardwarePadMode={
                  midi.connected ? controllerSurface?.pad_modes[0] : undefined
                }
                deck={engine.decks[0] || {}}
                library={library}
                command={command}
                waveformCommand={waveformCommand}
                visibleBeats={visibleBeats}
                snapSeek={snapSeek}
                snapshotTime={snapshotTime}
                load={load}
                demo={demo}
                reanalyze={reanalyze}
                loadSpotify={(deck, track) =>
                  loadSpotify(deck, track).catch(fail)
                }
              />
              <Mixer
                engine={engine}
                command={command}
                controllerSurface={
                  midi.connected ? controllerSurface : undefined
                }
              />
              <Deck
                id={1}
                hardwareRange={controllerSurface?.tempo_ranges[1]}
                onTempoRange={setControllerTempoRange}
                hardwarePadMode={
                  midi.connected ? controllerSurface?.pad_modes[1] : undefined
                }
                deck={engine.decks[1] || {}}
                library={library}
                command={command}
                waveformCommand={waveformCommand}
                visibleBeats={visibleBeats}
                snapSeek={snapSeek}
                snapshotTime={snapshotTime}
                load={load}
                demo={demo}
                reanalyze={reanalyze}
                loadSpotify={(deck, track) =>
                  loadSpotify(deck, track).catch(fail)
                }
              />
            </div>
          }
          {showSampler && (
            <section className="sampler-strip">
              <div className="sampler-label">
                <Zap size={14} />
                <span>
                  SAMPLER<small>Click to play · Shift to stop</small>
                  <select
                    aria-label="Sampler bank"
                    value={samplerBank}
                    onChange={(e) => setSamplerBank(+e.target.value)}
                  >
                    <option value={0}>Bank A · 1–8</option>
                    <option value={1}>Bank B · 9–16</option>
                  </select>
                </span>
              </div>
              <div className="sample-pads">
                {sampleNames
                  .slice(samplerBank * 8, samplerBank * 8 + 8)
                  .map((name, index) => {
                    const slot = samplerBank * 8 + index;
                    return (
                      <div className="sample-slot" key={slot}>
                        <button
                          onClick={(e) =>
                            command({
                              type: e.shiftKey
                                ? "stop_sample"
                                : "trigger_sample",
                              slot,
                            })
                          }
                        >
                          <span>{slot + 1}</span>
                          {name}
                        </button>
                        <button
                          aria-label={`Load sample ${slot + 1}`}
                          title="Load an audio sample"
                          onClick={async () => {
                            try {
                              const sample = await native<unknown>(
                                "choose_sample",
                                { slot },
                              );
                              if (sample) {
                                const name =
                                  typeof sample === "string"
                                    ? sample.split("/").pop()!
                                    : typeof sample === "object" &&
                                        sample &&
                                        "title" in sample
                                      ? String(sample.title)
                                      : `Sample ${slot + 1}`;
                                setSampleNames((names) =>
                                  names.map((old, i) =>
                                    i === slot ? name : old,
                                  ),
                                );
                              }
                            } catch (e) {
                              fail(e);
                            }
                          }}
                        >
                          <Plus size={11} />
                        </button>
                      </div>
                    );
                  })}
              </div>
              <Range
                label="LEVEL"
                value={val(engine.sampler_gain, 1)}
                max={2}
                onChange={(gain) => command({ type: "set_sampler_gain", gain })}
              />
            </section>
          )}
        </main>
      </div>
      <TrackDrawer
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        controllerCloseFocused={browserNavigation.layer === "close"}
        controllerPaneFocused={
          browserNavigation.layer === "playlists" ||
          browserNavigation.layer === "tracks"
        }
        onNavigationKey={browserNavigationKey}
        title={librarySource === "spotify" ? "Spotify tracks" : "Local tracks"}
      >
        {librarySource === "spotify" ? (
          <>
            {sourceJobs.map((job, deck) =>
              job.preparing ? (
                <div className="spotify-boundary" key={deck} role="status">
                  Preparing deck {deck === 0 ? "A" : "B"} in memory…{" "}
                  <button
                    onClick={() =>
                      native("cancel_spotify_load", { deck })
                        .then(refresh)
                        .catch(fail)
                    }
                  >
                    Cancel
                  </button>
                </div>
              ) : job.error ? (
                <div className="spotify-error" key={deck} role="alert">
                  Deck {deck === 0 ? "A" : "B"}: {job.error}
                </div>
              ) : null,
            )}
            <SpotifyPanel
              compact
              onLoadDeck={loadSpotify}
              deckJobs={sourceJobs}
              registerControllerBrowser={registerSpotifyBrowser}
              controllerLayer={browserNavigation.layer}
              onBrowserPaneChange={navigate}
            />
          </>
        ) : (
          <section className="library-panel">
            <div className="library-toolbar">
              <div>
                <h2>
                  {playlists.find((p) => p.id === playlist)?.name ||
                    "Your collection"}
                </h2>
                <span>
                  {visible.length} tracks
                  {library.length > 0
                    ? ` · ${fmt(library.reduce((n, t) => n + val(t.duration_seconds), 0))} total`
                    : ""}
                </span>
              </div>
              <div className="library-actions">
                <button
                  onClick={() => setNewPlaylist(!newPlaylist)}
                  aria-label="Create playlist"
                >
                  <Plus size={14} /> Playlist
                </button>

                <label className="search">
                  <Search size={15} />
                  <input
                    placeholder="Search tracks, artist, BPM, tags…"
                    aria-label="Search library"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                  />
                  <kbd>⌘ I</kbd>
                </label>
                <select
                  aria-label="Sort tracks"
                  value={sort}
                  onChange={(e) => setSort(e.target.value as typeof sort)}
                >
                  <option value="title">Title ↑</option>
                  <option value="bpm">BPM ↑</option>
                  <option value="duration">Duration ↑</option>
                </select>
              </div>
            </div>
            <div className="controller-browser-split">
              <aside
                className={`controller-playlists-pane ${localCollectionFocus && browserNavigation.layer !== "close" ? "pane-active" : ""}`}
                aria-label="Local playlists"
              >
                <div className="browser-pane-heading">PLAYLISTS</div>
                {[{ id: null, name: "All tracks" }, ...playlists].map(
                  (collection) => (
                    <button
                      key={collection.id ?? "all"}
                      data-local-collection={collection.id ?? "all"}
                      className={
                        playlist === collection.id ? "controller-selected" : ""
                      }
                      onClick={() => {
                        setPlaylist(collection.id);
                        setSelection(null);
                        setLocalCollectionFocus(true);
                      }}
                    >
                      {collection.name}
                    </button>
                  ),
                )}
              </aside>
              <div
                className={`controller-tracks-pane ${!localCollectionFocus && browserNavigation.layer !== "close" ? "pane-active" : ""}`}
                data-browser-pane="local-tracks"
                tabIndex={-1}
                onPointerDown={() => setLocalCollectionFocus(false)}
              >
                <div className="browser-pane-heading">
                  TRACKS <small>Press BROWSE to return to playlists</small>
                </div>
                {newPlaylist && (
                  <form
                    className="drawer-playlist-form"
                    onSubmit={async (e) => {
                      e.preventDefault();
                      try {
                        await native("create_playlist", {
                          name: playlistName.trim(),
                        });
                        setNewPlaylist(false);
                        setPlaylistName("");
                        await refresh();
                      } catch (e) {
                        fail(e);
                      }
                    }}
                  >
                    <input
                      autoFocus
                      aria-label="Playlist name"
                      placeholder="Playlist name"
                      value={playlistName}
                      onChange={(e) => setPlaylistName(e.target.value)}
                    />
                    <button disabled={!playlistName.trim()}>
                      <Check size={14} /> Create
                    </button>
                  </form>
                )}
                <div className="track-table-wrapper">
                  <table className="track-table local-track-table">
                    <thead>
                      <tr>
                        <th className="track-number">#</th>
                        <th>TITLE / ARTIST</th>
                        <th>BPM</th>
                        <th>KEY</th>
                        <th>TIME</th>
                        <th>DECK</th>
                        <th>Tags</th>
                      </tr>
                    </thead>
                    <tbody>{trackRows}</tbody>
                  </table>
                  {!visible.length && (
                    <div className="empty-library">
                      <div className="empty-icon">
                        <Music2 size={26} />
                      </div>
                      <h3>
                        {search
                          ? "No matching tracks"
                          : "A fresh start for your next set"}
                      </h3>
                      <p>
                        {search
                          ? "Try a different title, artist, BPM or tag."
                          : "Import local audio, then drag a track onto either deck."}
                      </p>
                      {!search && (
                        <button
                          className="import-button"
                          onClick={importTracks}
                          disabled={busy}
                        >
                          <FolderPlus size={15} />
                          Import your music
                        </button>
                      )}
                      <span>
                        {isDesktop
                          ? "WAV · AIFF · MP3 · FLAC · AAC · OGG"
                          : "Desktop preview · native audio is available in the app"}
                      </span>
                    </div>
                  )}
                </div>
                <div className="library-footer">
                  <span>
                    {selected
                      ? selected.title
                      : "Select a track to inspect or load"}
                  </span>
                  <div>
                    {selected && (
                      <>
                        <select
                          aria-label="Add selected track to playlist"
                          value=""
                          onChange={async (e) => {
                            try {
                              await native("add_to_playlist", {
                                playlist_id: Number(e.target.value),
                                track_id: selected.id,
                              });
                              await refresh();
                              announce("Track added to playlist");
                            } catch (err) {
                              fail(err);
                            }
                          }}
                        >
                          <option value="">Add to playlist…</option>
                          {playlists.map((p) => (
                            <option key={p.id} value={p.id}>
                              {p.name}
                            </option>
                          ))}
                        </select>
                        <button
                          onClick={async () => {
                            try {
                              await native("delete_track", { id: selected.id });
                              setSelection(null);
                              await refresh();
                            } catch (e) {
                              fail(e);
                            }
                          }}
                        >
                          Remove from collection
                        </button>
                      </>
                    )}
                    <span>
                      <ArrowLeftRight size={12} />
                      Drag to deck
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </section>
        )}
      </TrackDrawer>
      <footer
        className="session-footer"
        aria-label="Music library and session status"
        onKeyDownCapture={browserNavigationKey}
      >
        <div className="browser-dock">
          <div className="dock-actions">
            <button
              data-browser-source="spotify"
              className={`${drawerOpen && librarySource === "spotify" ? "active" : ""} ${browserNavigation.layer === "footer" && librarySource === "spotify" ? "controller-focused" : ""}`}
              onClick={() => {
                setLibrarySource("spotify");
                setDrawerOpen(!(drawerOpen && librarySource === "spotify"));
              }}
            >
              <Radio size={15} />{" "}
              {drawerOpen && librarySource === "spotify"
                ? "Hide Spotify"
                : "Open Spotify"}
            </button>
            <button
              data-browser-source="local"
              className={`${drawerOpen && librarySource === "local" ? "active" : ""} ${browserNavigation.layer === "footer" && librarySource === "local" ? "controller-focused" : ""}`}
              onClick={() => {
                setLibrarySource("local");
                setDrawerOpen(!(drawerOpen && librarySource === "local"));
              }}
            >
              <Library size={15} /> Local tracks <span>{library.length}</span>
            </button>
          </div>
          <span>
            {sourceJobs.some((job) => job.preparing)
              ? "Preparing track in memory…"
              : "Browse your music without leaving the decks."}
          </span>
          <button className="dock-audio" onClick={openSettings}>
            {engine.sample_rate
              ? `${(engine.sample_rate / 1000).toFixed(1)} kHz`
              : "Audio settings"}{" "}
            · {engine.buffer_frames || buffer} frames{" "}
            <SlidersHorizontal size={13} />
          </button>
        </div>
        <div className="statusbar">
          <div>
            <span className={`tiny-dot ${isDesktop ? "live" : ""}`} />
            {isDesktop
              ? "Native desktop session"
              : "Browser preview — open the desktop app for playback"}
          </div>
          <div>
            <span>
              {analysis?.active
                ? `Analyzing ${analysis.completed}/${analysis.total} tracks`
                : audioRunning
                  ? `${engine.output_channels || 2} output channels`
                  : "Configure audio to begin"}
            </span>
            {recordingActive && (
              <span className="recording-status">
                ● RECORDING{" "}
                {recording.dropped_frames
                  ? `· ${recording.dropped_frames} dropped frames`
                  : ""}
              </span>
            )}
            <span>STREAM ERRORS {engine.underruns || 0}</span>
            <span>SPACE: DECK A · ENTER: DECK B</span>
          </div>
        </div>
      </footer>
      {notice && (
        <div
          className={`toast ${notice.error ? "error" : ""}`}
          role={notice.error ? "alert" : "status"}
        >
          {notice.error ? <AlertCircle size={17} /> : <Check size={17} />}
          <span>{notice.text}</span>
          <button
            aria-label="Dismiss notification"
            onClick={() => setNotice(null)}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {settings && (
        <div className="modal-backdrop" onClick={() => setSettings(false)}>
          <section
            className="settings-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="settings-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-heading">
              <div>
                <h2 id="settings-title">Set up your session</h2>
                <p>
                  Native audio, controller connections, and library migration.
                </p>
              </div>
              <button
                className="icon-button"
                aria-label="Close settings"
                onClick={() => setSettings(false)}
              >
                <X size={20} />
              </button>
            </div>
            <div className="settings-tabs">
              {[
                "appearance",
                "audio",
                "midi",
                "library",
                "spotify",
                "roadmap",
              ].map((t) => (
                <button
                  key={t}
                  className={settingsTab === t ? "selected" : ""}
                  onClick={() => setSettingsTab(t)}
                >
                  {t === "midi"
                    ? "Controller"
                    : t[0].toUpperCase() + t.slice(1)}
                </button>
              ))}
            </div>
            <div className="settings-body">
              {settingsTab === "appearance" && (
                <section
                  className="appearance-settings"
                  aria-label="Appearance"
                >
                  <div className="settings-section-title">
                    <Disc3 size={18} />
                    <div>
                      <h3>Instrument theme</h3>
                      <p>
                        Changes apply immediately and restore when you reopen
                        PulseDeck.
                      </p>
                    </div>
                  </div>
                  <div
                    className="theme-options"
                    role="radiogroup"
                    aria-label="Application theme"
                  >
                    {THEMES.map((option) => (
                      <button
                        key={option.id}
                        role="radio"
                        aria-checked={theme === option.id}
                        tabIndex={theme === option.id ? 0 : -1}
                        onKeyDown={(event) => {
                          const direction = [
                            "ArrowRight",
                            "ArrowDown",
                          ].includes(event.key)
                            ? 1
                            : ["ArrowLeft", "ArrowUp"].includes(event.key)
                              ? -1
                              : 0;
                          if (
                            !direction &&
                            event.key !== "Home" &&
                            event.key !== "End"
                          )
                            return;
                          event.preventDefault();
                          const index = THEMES.findIndex(
                            (item) => item.id === option.id,
                          );
                          const next =
                            event.key === "Home"
                              ? THEMES[0]
                              : event.key === "End"
                                ? THEMES[THEMES.length - 1]
                                : THEMES[
                                    (index + direction + THEMES.length) %
                                      THEMES.length
                                  ];
                          setTheme(next.id);
                          event.currentTarget.parentElement
                            ?.querySelector<HTMLButtonElement>(
                              `[data-theme="${next.id}"]`,
                            )
                            ?.focus();
                        }}
                        data-theme={option.id}
                        className={`theme-option ${theme === option.id ? "is-selected" : ""}`}
                        onClick={() => setTheme(option.id)}
                      >
                        <span className="theme-preview" aria-hidden="true">
                          <i />
                          <i />
                          <i />
                          <b />
                        </span>
                        <span>
                          <strong>{option.name}</strong>
                          <small>{option.description}</small>
                        </span>
                        {theme === option.id && <Check size={16} />}
                      </button>
                    ))}
                  </div>
                  <p className="help-text">
                    Anatomy uses a palette of shaded surfaces and engraved
                    lines. Frequency bands and cue colors stay consistent across
                    themes.
                  </p>
                </section>
              )}
              {settingsTab === "audio" && (
                <>
                  <div className="settings-section-title">
                    <Volume2 size={18} />
                    <div>
                      <h3>Audio output</h3>
                      <p>Changes apply when you start the audio engine.</p>
                    </div>
                  </div>
                  <label className="field">
                    Output device
                    <select
                      value={device}
                      onChange={(e) => {
                        audioSettingsTouched.current = true;
                        setDevice(e.target.value);
                        setCueRoute(false);
                        setMasterL(0);
                        setMasterR(1);
                      }}
                    >
                      {devices.length ? (
                        devices.map((d) => (
                          <option key={d.name}>{d.name}</option>
                        ))
                      ) : (
                        <option value="">No output devices available</option>
                      )}
                    </select>
                  </label>
                  <div className="field-grid">
                    <label className="field">
                      Buffer size
                      <select
                        value={buffer}
                        onChange={(e) => {
                          audioSettingsTouched.current = true;
                          setBuffer(+e.target.value);
                        }}
                      >
                        {[64, 128, 256, 512, 1024].map((n) => (
                          <option value={n} key={n}>
                            {n} frames
                          </option>
                        ))}
                      </select>
                    </label>
                    <div className="field">
                      <span>Device channels</span>
                      <div className="readout">{channels} outputs</div>
                    </div>
                  </div>
                  <div className="routing-block">
                    <h4>Master output</h4>
                    <div className="field-grid">
                      <label className="field">
                        Left channel
                        <select
                          value={masterL}
                          onChange={(e) => {
                            audioSettingsTouched.current = true;
                            setMasterL(+e.target.value);
                          }}
                        >
                          {Array.from({ length: channels }, (_, i) => (
                            <option value={i} key={i}>
                              Output {i + 1}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="field">
                        Right channel
                        <select
                          value={masterR}
                          onChange={(e) => {
                            audioSettingsTouched.current = true;
                            setMasterR(+e.target.value);
                          }}
                        >
                          {Array.from({ length: channels }, (_, i) => (
                            <option value={i} key={i}>
                              Output {i + 1}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>
                    <label className="checkbox">
                      <input
                        type="checkbox"
                        checked={cueRoute}
                        disabled={channels < 4}
                        onChange={(e) => {
                          audioSettingsTouched.current = true;
                          setCueRoute(e.target.checked);
                        }}
                      />
                      Separate headphone output
                    </label>
                    {channels < 4 && (
                      <p className="help-text">
                        Independent headphone cue needs an audio device with at
                        least four output channels, such as the DDJ-FLX4.
                      </p>
                    )}
                    {cueRoute && (
                      <div className="field-grid">
                        <label className="field">
                          Cue left
                          <select
                            value={cueL}
                            onChange={(e) => setCueL(+e.target.value)}
                          >
                            {Array.from({ length: channels }, (_, i) => (
                              <option value={i} key={i}>
                                Output {i + 1}
                              </option>
                            ))}
                          </select>
                        </label>
                        <label className="field">
                          Cue right
                          <select
                            value={cueR}
                            onChange={(e) => setCueR(+e.target.value)}
                          >
                            {Array.from({ length: channels }, (_, i) => (
                              <option value={i} key={i}>
                                Output {i + 1}
                              </option>
                            ))}
                          </select>
                        </label>
                      </div>
                    )}
                  </div>
                  {smartModes && (
                    <div className="transition-presets">
                      <h4>PulseDeck transition presets</h4>
                      <label className="checkbox">
                        <input
                          type="checkbox"
                          checked={smartModes.fader}
                          onChange={async (e) => {
                            try {
                              await native("set_smart_modes", {
                                fader: e.target.checked,
                                cfx: smartModes.cfx,
                              });
                              await refresh();
                            } catch (error) {
                              fail(error);
                            }
                          }}
                        />
                        Tempo blend
                      </label>
                      <p className="help-text">
                        Blend the decks' tempos as you move the crossfader.
                      </p>
                      <label className="checkbox">
                        <input
                          type="checkbox"
                          checked={smartModes.cfx}
                          onChange={async (e) => {
                            try {
                              await native("set_smart_modes", {
                                fader: smartModes.fader,
                                cfx: e.target.checked,
                              });
                              await refresh();
                            } catch (error) {
                              fail(error);
                            }
                          }}
                        />
                        Filter echo
                      </label>
                      <p className="help-text">
                        Add echo and reverb as you turn the channel filter.
                      </p>
                    </div>
                  )}
                  <div className="settings-bottom">
                    <button
                      className="secondary"
                      onClick={async () => {
                        try {
                          setDevices(await native("list_audio_devices"));
                        } catch (e) {
                          fail(e);
                        }
                      }}
                    >
                      Refresh devices
                    </button>
                    <div>
                      {audioRunning && (
                        <button
                          className="secondary"
                          disabled={audioChanging}
                          onClick={async () => {
                            if (audioChangePending.current) return;
                            audioChangePending.current = true;
                            setAudioChanging(true);
                            try {
                              await native("stop_audio");
                              await refresh();
                            } catch (e) {
                              fail(e);
                            } finally {
                              audioChangePending.current = false;
                              setAudioChanging(false);
                            }
                          }}
                        >
                          Stop audio
                        </button>
                      )}
                      <button
                        className="import-button"
                        onClick={startAudio}
                        disabled={!devices.length || audioChanging}
                      >
                        {audioChanging
                          ? "Changing output…"
                          : audioRunning
                            ? "Restart audio"
                            : "Start audio"}
                      </button>
                    </div>
                  </div>
                </>
              )}
              {settingsTab === "midi" && (
                <>
                  <div className="settings-section-title">
                    <Radio size={18} />
                    <div>
                      <h3>DDJ-FLX4 / MIDI</h3>
                      <p>
                        Connect a macOS MIDI endpoint to control the two decks.
                      </p>
                    </div>
                  </div>
                  <div
                    className={`device-status ${midi.connected ? "connected" : ""}`}
                  >
                    <span className="tiny-dot" />
                    <div>
                      <strong>
                        {midi.connected
                          ? midi.input_name || "Controller connected"
                          : "No controller connected"}
                      </strong>
                      <span>
                        {midi.connected
                          ? `${midi.messages_received || 0} MIDI messages received · LED feedback ${midi.feedback_enabled ? "on" : "off"}`
                          : "Audio and library functions work without a controller"}
                      </span>
                    </div>
                  </div>
                  {midi.last_error && (
                    <p className="help-text midi-error">{midi.last_error}</p>
                  )}
                  <label className="field">
                    MIDI input
                    <select
                      value={midiInput}
                      onChange={(e) => setMidiInput(e.target.value)}
                    >
                      <option value="">Choose input…</option>
                      {midiInputs.map((p) => (
                        <option key={p}>{p}</option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    MIDI output / LED feedback
                    <select
                      value={midiOutput}
                      onChange={(e) => setMidiOutput(e.target.value)}
                    >
                      <option value="">No LED output</option>
                      {midiOutputs.map((p) => (
                        <option key={p}>{p}</option>
                      ))}
                    </select>
                  </label>
                  {!midiInputs.length && (
                    <p className="help-text">
                      macOS currently exposes no MIDI input. Reconnect the
                      controller and refresh when it appears.
                    </p>
                  )}
                  <div className="settings-bottom">
                    <button className="secondary" onClick={refreshPorts}>
                      Refresh MIDI ports
                    </button>
                    <button
                      className="import-button"
                      disabled={!midiInput && !midi.connected}
                      onClick={async () => {
                        try {
                          await native(
                            midi.connected ? "disconnect_midi" : "connect_midi",
                            midi.connected
                              ? undefined
                              : {
                                  input_name: midiInput,
                                  output_name: midiOutput || null,
                                },
                          );
                          await refresh();
                          announce(
                            midi.connected
                              ? "Controller disconnected"
                              : "Controller connected",
                          );
                        } catch (e) {
                          fail(e);
                        }
                      }}
                    >
                      {midi.connected ? "Disconnect" : "Connect controller"}
                    </button>
                  </div>
                </>
              )}
              {settingsTab === "library" && (
                <>
                  <div className="settings-section-title">
                    <Library size={18} />
                    <div>
                      <h3>Library migration</h3>
                      <p>
                        Move track metadata and playlists through rekordbox XML.
                      </p>
                    </div>
                  </div>
                  <div className="migration-card">
                    <h4>Import a rekordbox collection</h4>
                    <p>
                      Export your collection as XML from rekordbox, then choose
                      it here. Audio files stay in their original locations.
                    </p>
                    <button
                      className="secondary"
                      onClick={async () => {
                        setBusy(true);
                        try {
                          await native("import_rekordbox_xml");
                          await refresh();
                          announce("XML collection imported");
                        } catch (e) {
                          fail(e);
                        } finally {
                          setBusy(false);
                        }
                      }}
                      disabled={busy}
                    >
                      Import rekordbox XML
                    </button>
                  </div>
                  <div className="migration-card">
                    <h4>Export your PulseDeck library</h4>
                    <p>
                      Create a portable XML collection for playlists, track
                      metadata, and cue points.
                    </p>
                    <button
                      className="secondary"
                      onClick={async () => {
                        try {
                          await native("export_rekordbox_xml");
                          announce("Library export finished");
                        } catch (e) {
                          fail(e);
                        }
                      }}
                    >
                      Export library XML
                    </button>
                  </div>
                  {!!analysis?.errors.length && (
                    <div className="import-errors">
                      <h4>Import errors ({analysis.errors.length})</h4>
                      {analysis.errors.map((error, i) => (
                        <p key={i}>{error}</p>
                      ))}
                    </div>
                  )}
                  <p className="help-text">
                    Only the collection record is removed when you remove a
                    track. Your audio file stays on disk.
                  </p>
                </>
              )}
              {settingsTab === "spotify" && <SpotifyPanel settingsOnly />}
              {settingsTab === "roadmap" && (
                <>
                  <div className="settings-section-title">
                    <AudioLines size={18} />
                    <div>
                      <h3>Build capabilities</h3>
                      <p>
                        This is the first native two-deck performance
                        implementation.
                      </p>
                    </div>
                  </div>
                  <div className="capability">
                    <Check size={15} />
                    <div>
                      <strong>Native Rust playback & mixing</strong>
                      <span>
                        Two decks, cue points, loops, EQ, filters, effects,
                        routing and MIDI.
                      </span>
                    </div>
                  </div>
                  <div className="capability">
                    <Check size={15} />
                    <div>
                      <strong>Pitch lock, sampler and WAV recording</strong>
                      <span>
                        Pitch-preserving tempo changes, eight sample slots,
                        master mix recording.
                      </span>
                    </div>
                  </div>
                  <div className="capability">
                    <Check size={15} />
                    <div>
                      <strong>Local collection</strong>
                      <span>
                        File imports, waveform analysis, BPM / key estimates,
                        playlists, memory cues and XML exchange.
                      </span>
                    </div>
                  </div>
                  {[
                    "Stem separation, streaming in DJ decks and cloud sync",
                    "DVS, video and lighting",
                  ].map((t) => (
                    <div className="capability pending" key={t}>
                      <span className="roadmap-dot" />
                      <div>
                        <strong>{t}</strong>
                        <span>Planned · not available in this build</span>
                      </div>
                    </div>
                  ))}
                  <p className="help-text">
                    Enable Key Lock to preserve pitch during tempo changes. The
                    deck shows Preparing while a new stretch is computed.
                    Performance parity and FLX4 hardware behavior require
                    measurements on a connected device.
                  </p>
                </>
              )}
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

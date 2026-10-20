import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
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
  ListMusic,
  Radio,
  Search,
  Settings2,
  Volume2,
  X,
  Zap,
  Check,
  AlertCircle,
  SkipBack,
  SkipForward,
  Repeat2,
} from "lucide-react";
import { isDesktop, control } from "./api";
import {
  transportTimings,
  metadataTimings,
  performanceSnapshot,
} from "./performance";
import TrackTags from "./TrackTags";
import LibraryTools, { type LibraryFilters } from "./LibraryTools";
import TrackArtwork from "./TrackArtwork";
import VirtualTrackRows from "./VirtualTrackRows";
import { useLocalLibrary } from "./useLocalLibrary";
import TempoSlider from "./TempoSlider";
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
} from "./types";
import Waveform from "./Waveform";
import AnalysisInfo from "./AnalysisInfo";
import TrackDrawer from "./TrackDrawer";
import {
  deckBeatGrid,
  snapToGridBeat,
  gridStatus,
  gridBeatPosition,
  gridBeatTime,
} from "./beat-grid";
import {
  withTrackMetadata,
  engineStore,
  stabilizeTrack,
  retainUnchanged,
  type EngineTransport,
  type EngineMetadata,
} from "./engine-state";
import { BAND_COLORS, HOT_CUE_COLORS, displayCuePoints } from "./waveform-data";
import SpotifyPanel, { type SpotifyTrack } from "./SpotifyPanel";
import YouTubeMusicPanel from "./YouTubeMusicPanel";
import SpotifyLogo from "./SpotifyLogo";
import SuggestPanel from "./SuggestPanel";
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
/* Brand mark: a record drawn as a circle of dots, orange at the spindle. */
const BRAND_DOTS = Array.from({ length: 25 }, (_, i) => [
  2.2 + (i % 5) * 4.4,
  2.2 + Math.floor(i / 5) * 4.4,
]).filter(([x, y]) => Math.hypot(x - 11, y - 11) < 10.5);
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
  tone,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
  reset?: number;
  tone?: "trim" | "high" | "mid" | "low" | "filter" | "master" | "aux";
}) {
  return (
    <label className={`knob-control ${tone ? `tone-${tone}` : ""}`}>
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
        <i />
      </span>
      <span>{label}</span>
    </label>
  );
}
function Meter({ deckId }: { deckId: number }) {
  const value = useSyncExternalStore(
    engineStore.subscribeTransport,
    () =>
      Math.ceil(
        Math.max(
          ...(engineStore.getTransport().decks[deckId]?.peak || [0, 0]),
        ) * 18,
      ) / 18,
  );
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
function PerformanceDiagnostics() {
  const [snapshot, setSnapshot] = useState(performanceSnapshot);
  useEffect(() => {
    const timer = setInterval(() => setSnapshot(performanceSnapshot()), 1000);
    return () => clearInterval(timer);
  }, []);
  useControl(
    "performance",
    {},
    { ...snapshot, store: engineStore.diagnostics },
  );
  return null;
}
function OutputProtection() {
  const summary = useSyncExternalStore(engineStore.subscribeTransport, () => {
    const q = engineStore.getTransport().audio_quality as
      | {
          headroom_db?: number;
          master_gain_reduction_db?: number;
          cue_gain_reduction_db?: number;
          master_input_peak?: number[];
          cue_input_peak?: number[];
        }
      | undefined;
    return q
      ? JSON.stringify([
          q.headroom_db ?? 0,
          Math.round((q.master_gain_reduction_db ?? 0) * 10) / 10,
          Math.round((q.cue_gain_reduction_db ?? 0) * 10) / 10,
        ])
      : "[]";
  });
  const [headroom, master, cue] = JSON.parse(summary) as number[];
  if (!headroom) return null;
  return (
    <div
      className={`output-protection ${master > 0.1 || cue > 0.1 ? "limiting" : ""}`}
      title="Stereo-linked output protection. Lower trim when gain reduction persists. The meters show protected output; source clipping cannot be repaired."
    >
      <span>HEADROOM {headroom} dB</span>
      <span>
        LIMIT {master.toFixed(1)} dB · CUE {cue.toFixed(1)} dB
      </span>
    </div>
  );
}
function livePosition(id: number) {
  const deck = engineStore.getTransport().decks[id];
  return val(deck?.position_seconds, val(deck?.position));
}
function LivePosition({ id }: { id: number }) {
  const seconds = useSyncExternalStore(engineStore.subscribeTransport, () =>
    Math.floor(livePosition(id)),
  );
  return <>{fmt(seconds)}</>;
}
function ActualBpm({ id }: { id: number }) {
  const text = useSyncExternalStore(engineStore.subscribeTransport, () => {
    const bpm = engineStore.getTransport().decks[id]?.actual_playback_bpm;
    return bpm != null && Number.isFinite(bpm) ? bpm.toFixed(1) : "—";
  });
  return (
    <span title="Estimated current musical tempo, including Sync and pitch bend">
      ACTUAL {text} BPM
    </span>
  );
}
function DeckTime({ id, duration }: { id: number; duration: number }) {
  const seconds = useSyncExternalStore(engineStore.subscribeTransport, () =>
    Math.floor(livePosition(id)),
  );
  return (
    <div className="time-readout">
      <strong>{fmt(seconds)}</strong>
      <span>−{fmt(duration - seconds)} remaining</span>
    </div>
  );
}
function PlatterNeedle({ id, duration }: { id: number; duration: number }) {
  const needle = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const element = needle.current!,
      jog = element.closest<HTMLElement>(".jog");
    // The compositor interpolates rotation between authoritative native updates.
    const rotation = element.animate(
      [{ transform: "rotate(0deg)" }, { transform: "rotate(360deg)" }],
      { duration: 4000, iterations: Infinity },
    );
    rotation.pause();
    let identity = "",
      progress = -1,
      stale: ReturnType<typeof setTimeout>;
    const update = () => {
      const engine = engineStore.getTransport(),
        deck = engine.decks[id];
      const rate =
        deck.actual_playback_rate ??
        deck.playback_rate ??
        1 + (deck.tempo_percent ?? 0) / 100;
      const playing = !!deck.playing && !deck.jog_touch;
      const latency =
        (engine.audio_quality as { output_latency_frames?: number } | undefined)
          ?.output_latency_frames ?? 0;
      const elapsed = playing
        ? Math.min(
            120,
            Math.max(0, performance.now() - engineStore.receivedAt()),
          ) / 1000
        : 0;
      const position = Math.max(
        0,
        Math.min(
          duration,
          livePosition(id) +
            (elapsed -
              (playing ? latency / (engine.sample_rate || 48000) : 0)) *
              rate,
        ),
      );
      const next = `${deck.track?.path}@${engine.load_revisions?.[id]}@${deck.position_revision}`;
      if (!playing) rotation.pause();
      if (
        next !== identity ||
        rotation.playState !== "running" ||
        Math.abs(Number(rotation.currentTime) - position * 1000) > 40
      )
        rotation.currentTime = position * 1000;
      identity = next;
      rotation.playbackRate = rate;
      if (playing && rotation.playState !== "running") rotation.play();
      const tick =
        duration > 0 ? Math.min(60, Math.floor((position / duration) * 60)) : 0;
      if (tick !== progress) {
        progress = tick;
        jog?.style.setProperty("--progress", String(tick / 60));
      }
      clearTimeout(stale);
      if (playing) stale = setTimeout(() => rotation.pause(), 120);
    };
    const unsubscribe = engineStore.subscribeTransport(update);
    update();
    return () => {
      unsubscribe();
      clearTimeout(stale);
      rotation.cancel();
    };
  }, [id, duration]);
  return <i ref={needle} />;
}
type LoadingTrack = {
  source: string;
  title: string;
  artist?: string;
  cancellable: boolean;
};

function LoadingWaveform({ title }: { title: string }) {
  return (
    <div className="loading-waveform" aria-hidden="true">
      <div className="loading-waveform-bars">
        {Array.from({ length: 48 }, (_, i) => (
          <i key={i} style={{ height: `${18 + ((i * 17) % 61)}%` }} />
        ))}
      </div>
      <span>
        <LoaderCircle size={14} className="spin" /> Loading {title}…
      </span>
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
  loading,
}: {
  id: number;
  deck: DeckSnapshot;
  library: TrackRecord[];
  command: (c: Command, revision?: number) => void;
  waveformCommand: (c: Command, revision?: number) => Promise<void>;
  snapshotTime: number;
  visibleBeats: number;
  snapSeek: boolean;
  loading?: LoadingTrack | null;
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
  const cueMarkers = useMemo(
    () => displayCuePoints(track?.cue_points, duration),
    [track?.cue_points, duration],
  );
  const beatGrid = useMemo(
    () => deckBeatGrid(deck, track),
    [
      track,
      deck.bpm,
      deck.beatgrid_offset_seconds,
      deck.beatgrid_uniform,
      deck.beatgrid_valid,
      deck.beatgrid_downbeat,
      deck.beatgrid_manual,
      deck.beatgrid_anchor_index,
    ],
  );
  const heldPosition = useRef<number | null>(null);
  const loadRevision = engineStore.getTransport().load_revisions?.[id];
  const sourceToken = `${track?.path ?? ""}@${loadRevision}`;
  const currentSource = useRef(sourceToken);
  currentSource.current = sourceToken;
  const guardedWaveform = (c: Command) => waveformCommand(c, loadRevision);
  const color = id === 0 ? "#f5d90a" : "#86cfee";
  if (loading)
    return (
      <section
        className="stacked-wave-row"
        aria-busy="true"
        aria-label={`Deck ${id === 0 ? "A" : "B"} waveform loading`}
        style={{ "--deck-color": color } as React.CSSProperties}
      >
        <span className="scrolling-deck-label">{id === 0 ? "A" : "B"}</span>
        <LoadingWaveform title={loading.title} />
      </section>
    );
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
        deckId={id}
        position={position}
        duration={duration}
        bpm={baseBpm}
        spanBeats={visibleBeats}
        color={color}
        hotcues={deck.hot_cues || []}
        cueMarkers={cueMarkers}
        memoryCues={deck.memory_cues || []}
        cuePoint={deck.cue_seconds}
        loopStart={deck.loop_start_seconds}
        loopEnd={deck.loop_end_seconds}
        loopActive={deck.loop_active}
        beatGrid={beatGrid}
        beatgridOffset={gridOffset}
        beatgridValid={!!deck.beatgrid_valid}
        beatgridDownbeat={!!deck.beatgrid_downbeat}
        onSeek={(seconds) =>
          command(
            {
              type: "seek",
              deck: id,
              seconds:
                snapSeek && deck.beatgrid_valid
                  ? snapToGridBeat(beatGrid, seconds, duration)
                  : seconds,
            },
            loadRevision,
          )
        }
        playing={!!deck.playing}
        playbackRate={val(
          deck.actual_playback_rate ?? deck.playback_rate,
          1 + val(deck.tempo_percent) / 100,
        )}
        jogTouch={!!deck.jog_touch}
        positionRevision={deck.position_revision}
        sourceId={track?.path}
        loadRevision={loadRevision}
        snapshotTime={snapshotTime}
        onScrubStart={async (grabbed) => {
          heldPosition.current = grabbed;
          await guardedWaveform({ type: "jog_touch", deck: id, touched: true });
          if (currentSource.current === sourceToken)
            await guardedWaveform({ type: "seek", deck: id, seconds: grabbed });
        }}
        onScrub={async (delta, elapsedMs) => {
          if (currentSource.current !== sourceToken) return;
          const target = Math.max(
            0,
            Math.min(duration, (heldPosition.current ?? position) + delta),
          );
          heldPosition.current = target;
          await guardedWaveform({
            type: "scrub_to",
            deck: id,
            seconds: target,
            duration_ms: Math.min(100, elapsedMs + 8),
          });
        }}
        onScrubEnd={async () => {
          const target = heldPosition.current;
          heldPosition.current = null;
          if (currentSource.current !== sourceToken) return;
          if (target != null && currentSource.current === sourceToken) {
            try {
              await guardedWaveform({
                type: "scrub_end",
                deck: id,
                seconds: target,
              });
              return;
            } catch {
              // Failed commit must still release a held platter.
            }
          }
          await guardedWaveform({
            type: "jog_touch",
            deck: id,
            touched: false,
          });
        }}
      />
    </section>
  );
}
function Deck({
  id,
  deck: initialDeck,
  library,
  command,
  waveformCommand: rawWaveformCommand,
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
  loading,
  loadError,
  cancelLoad,
}: {
  id: number;
  deck: DeckSnapshot;
  library: TrackRecord[];
  command: (c: Command, revision?: number) => void;
  waveformCommand: (c: Command, revision?: number) => Promise<void>;
  load: (deck: number, path: string) => void;
  demo: (deck: number) => void;
  reanalyze: (deck: number, revision?: number) => Promise<void>;
  loadSpotify: (deck: number, track: SpotifyTrack) => Promise<void>;
  visibleBeats: number;
  snapSeek: boolean;
  snapshotTime: number;
  hardwareRange?: number;
  hardwarePadMode?: string;
  onTempoRange: (deck: number, range: number) => void;
  loading?: LoadingTrack | null;
  loadError?: string | null;
  cancelLoad: (deck: number) => void;
}) {
  const controls = useSyncExternalStore(engineStore.subscribeControls, () =>
    engineStore.getDeckView(id),
  );
  const deck =
    controls?.track?.path === initialDeck.track?.path ? controls : initialDeck;
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
  const beatGrid = useMemo(
    () => deckBeatGrid(deck, track),
    [
      track,
      deck.bpm,
      deck.beatgrid_offset_seconds,
      deck.beatgrid_uniform,
      deck.beatgrid_valid,
      deck.beatgrid_downbeat,
      deck.beatgrid_manual,
      deck.beatgrid_anchor_index,
    ],
  );
  const tempo = val(deck.tempo_percent);
  const bpm = val(deck.target_playback_bpm, baseBpm * (1 + tempo / 100));
  const hotcues = deck.hot_cues || Array(8).fill(null);
  const cueMarkers = useMemo(
    () => displayCuePoints(track?.cue_points, duration),
    [track?.cue_points, duration],
  );
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
  const loadRevision = engineStore.getTransport().load_revisions?.[id];
  const waveformCommand = useCallback(
    (c: Command, revision = loadRevision) => rawWaveformCommand(c, revision),
    [rawWaveformCommand, loadRevision],
  );
  const sourceToken = `${track?.path ?? ""}@${loadRevision}`;
  const currentJogSource = useRef<string | undefined>(sourceToken);
  currentJogSource.current = loading ? undefined : sourceToken;
  const jogGesture = useRef<{
    source: string | undefined;
    target: number;
    x: number;
    lastAt: number;
    pending: boolean;
  } | null>(null);
  const jogFrame = useRef<number | null>(null);
  const jogQueue = useRef(Promise.resolve());
  const enqueueJog = (c: Command, source: string | undefined) => {
    jogQueue.current = jogQueue.current
      .then(async () => {
        if (currentJogSource.current === source)
          await waveformCommand({ ...c, deck: id }, loadRevision);
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
            waveformCommand(
              { type: "jog_touch", deck: id, touched: false },
              loadRevision,
            ),
          )
          .catch(() => {});
      }
    },
    [track?.path, loadRevision, loading?.source, id, waveformCommand],
  );
  const color = id === 0 ? "#f5d90a" : "#86cfee";
  const send = (type: string, data: Command = {}) =>
    command({ type, deck: id, ...data }, loadRevision);
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
        if (Math.abs(bpm - baseBpm) > 0.01)
          await waveformCommand({ type: "set_bpm", deck: id, bpm });
        await waveformCommand({
          type: "set_beatgrid",
          deck: id,
          offset_seconds: offset,
        });
        setEditGrid(false);
      },
      "grid-align-here": async () => {
        await waveformCommand({
          type: "set_beatgrid",
          deck: id,
          offset_seconds: livePosition(id),
        });
        setGridOffset(livePosition(id).toFixed(3));
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
      loading: loading ?? null,
    },
  );
  if (loading)
    return (
      <section
        className={`deck deck-${id} deck-loading`}
        aria-busy="true"
        aria-label={`Deck ${id === 0 ? "A" : "B"} loading`}
        style={{ "--deck-color": color } as React.CSSProperties}
      >
        <div className="deck-heading">
          <span className="deck-badge">{id === 0 ? "A" : "B"}</span>
          <div className="track-heading">
            <strong>{loading.title}</strong>
            <span>{loading.artist || "Loading track…"}</span>
          </div>
          {loading.cancellable && (
            <button className="subtle" onClick={() => cancelLoad(id)}>
              Cancel
            </button>
          )}
        </div>
        <div className="deck-track-preview">
          <LoadingWaveform title={loading.title} />
        </div>
        <div className="deck-loading-body" role="status">
          <div className="loading-disc">
            <Disc3 size={72} strokeWidth={1} />
          </div>
          <span>
            <LoaderCircle size={16} className="spin" /> Loading deck{" "}
            {id === 0 ? "A" : "B"}…
          </span>
          <small>Waveform and controls will appear when ready</small>
        </div>
      </section>
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
          {(track?.artist || !title) && (
            <span>
              {track?.artist || "Drag a track here or choose from the library"}
            </span>
          )}
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
      {loadError && (
        <div className="deck-load-error" role="alert">
          {loadError}
        </div>
      )}
      <div className="deck-info">
        <div
          className="tempo-readout"
          title="Requested BPM at the last tempo adjustment. Actual BPM can change with the music; Sync keeps the decks aligned."
        >
          <strong>{bpm ? bpm.toFixed(2) : "—"}</strong>
          <span>BPM TARGET</span>
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
            <ActualBpm id={id} />
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
        <DeckTime id={id} duration={duration} />
      </div>
      {editGrid && (
        <form
          className="grid-editor"
          onSubmit={(e) => {
            e.preventDefault();
            const bpm = Number(gridBpm);
            const offset = Number(gridOffset);
            if (bpm >= 30 && bpm <= 300 && Number.isFinite(offset)) {
              if (Math.abs(bpm - baseBpm) > 0.01) send("set_bpm", { bpm });
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
            BEAT 1 (s)
            <input
              aria-label={`Deck ${id + 1} bar one origin`}
              type="number"
              min={0}
              step={0.001}
              value={gridOffset}
              onChange={(e) => setGridOffset(e.target.value)}
            />
          </label>
          <div className="grid-tempo-tools">
            {[-1, 1].map((step) => (
              <button
                key={step}
                type="button"
                disabled={
                  !(+gridBpm >= 30 && +gridBpm <= 300) ||
                  !Number.isFinite(+gridOffset)
                }
                title={`Move beat 1 one beat ${step < 0 ? "earlier" : "later"}`}
                onClick={() =>
                  setGridOffset(
                    Math.max(
                      0,
                      Math.min(
                        duration,
                        gridBeatTime(
                          beatGrid,
                          Math.round(
                            gridBeatPosition(beatGrid, +gridOffset) ?? 0,
                          ) + step,
                        ) ?? +gridOffset + (step * 60) / +gridBpm,
                      ),
                    ).toFixed(3),
                  )
                }
              >
                {step < 0 ? "◀ beat 1" : "beat 1 ▶"}
              </button>
            ))}
          </div>
          <button>
            <Check size={12} />
            Apply grid
          </button>
          <small title={track?.rhythm_error ?? undefined}>
            {gridStatus(deck, track)}
          </small>
        </form>
      )}
      <div className="deck-track-preview">
        <Waveform
          samples={track?.waveform || deck.waveform || []}
          bands={track?.waveform_bands}
          deckId={id}
          position={position}
          duration={duration}
          bpm={baseBpm}
          spanBeats={visibleBeats}
          color={color}
          overview
          playing={!!deck.playing}
          playbackRate={val(
            deck.actual_playback_rate ?? deck.playback_rate,
            1 + val(deck.tempo_percent) / 100,
          )}
          jogTouch={!!deck.jog_touch}
          positionRevision={deck.position_revision}
          sourceId={track?.path}
          loadRevision={loadRevision}
          snapshotTime={snapshotTime}
          hotcues={hotcues}
          cueMarkers={cueMarkers}
          memoryCues={deck.memory_cues || []}
          cuePoint={deck.cue_seconds}
          loopStart={deck.loop_start_seconds}
          loopEnd={deck.loop_end_seconds}
          loopActive={deck.loop_active}
          beatGrid={beatGrid}
          beatgridOffset={val(deck.beatgrid_offset_seconds)}
          beatgridValid={!!deck.beatgrid_valid}
          beatgridDownbeat={!!deck.beatgrid_downbeat}
          onSeek={(seconds) =>
            send("seek", {
              seconds:
                snapSeek && deck.beatgrid_valid
                  ? snapToGridBeat(beatGrid, seconds, duration)
                  : seconds,
            })
          }
        />
        <div className="preview-caption">
          {track ? (
            <AnalysisInfo
              controlTarget={`analysis-${id === 0 ? "A" : "B"}`}
              key={`${track.id}-${track.path}-${loadRevision}`}
              track={track}
              deck={deck}
              deckId={id}
              expectedLoadRevision={loadRevision}
              onReanalyze={() => reanalyze(id, loadRevision)}
              baseBpm={baseBpm}
              onChooseBpm={(bpm) => {
                setGridBpm(bpm.toFixed(2));
                setGridOffset(String(val(deck.beatgrid_offset_seconds)));
                setEditGrid(true);
              }}
            />
          ) : (
            <span>FULL TRACK</span>
          )}
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
          className={`jog ${deck.playing ? "is-playing" : ""}`}
          onPointerDown={(e) => {
            if (!title || e.button !== 0) return;
            e.preventDefault();
            e.currentTarget.setPointerCapture(e.pointerId);
            jogGesture.current = {
              source: sourceToken,
              target: livePosition(id),
              x: e.clientX,
              lastAt: performance.now(),
              pending: false,
            };
            setScratch(true);
            enqueueJog({ type: "jog_touch", touched: true }, sourceToken);
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
            const rotation = e.currentTarget
              .querySelector(".jog-surface > i")
              ?.getAnimations()[0];
            if (rotation) rotation.currentTime = gesture.target * 1000;
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
            <div
              className="jog-center"
              title="Requested BPM at the last tempo adjustment. Actual BPM can change with the music; Sync keeps the decks aligned."
            >
              <strong>{bpm ? bpm.toFixed(2) : "—"}</strong>
              <span>BPM</span>
              <small>
                {scratch
                  ? "SCRATCH"
                  : `${tempo >= 0 ? "+" : ""}${tempo.toFixed(2)}%`}
              </small>
            </div>
            <PlatterNeedle id={id} duration={duration} />
          </div>
        </div>
        <div className="deck-keys">
          <div className="transport-column">
            <div className="sync-row">
              <button
                disabled={!title || (!!deck.is_master && !deck.sync_enabled)}
                className={`mini ${deck.sync_enabled ? "sync-on" : ""}`}
                title={
                  deck.is_master
                    ? "Choose the other playing deck as master before enabling follower Sync"
                    : "Follow the master deck's tempo and beat phase"
                }
                onClick={() =>
                  send("set_sync", { enabled: !deck.sync_enabled })
                }
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
                disabled={!title || deck.preparation_pending}
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
                className={`play ${deck.playing || deck.launch_pending ? "active" : ""}`}
                aria-label={
                  deck.launch_pending
                    ? "Cancel queued launch"
                    : deck.playing
                      ? "Pause"
                      : "Play"
                }
                title={
                  deck.launch_pending
                    ? "Queued for the next master beat; click to cancel"
                    : "Play / pause"
                }
                disabled={!title || deck.preparation_pending}
                onClick={() => send("toggle_play")}
              >
                {deck.preparation_pending ? (
                  <LoaderCircle size={20} className="spin" />
                ) : deck.launch_pending ? (
                  "QUEUED"
                ) : deck.playing ? (
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
              SET CUE AT <LivePosition id={id} />
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
            {padMode === "memory" && (
              <span>
                {deck.memory_cues?.filter((c) => c !== null).length || 0} / 10
                saved
              </span>
            )}
          </div>
        </div>
        <div className="tempo-slider">
          <TempoSlider
            key={sourceToken}
            range={range}
            value={tempo}
            write={(percent) =>
              waveformCommand(
                { type: "set_tempo", deck: id, percent },
                loadRevision,
              )
            }
          >
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
          </TempoSlider>
        </div>
      </div>
      {hardwarePadMode && (
        <div className="controller-pad-mode">
          FLX4 PADS · {controllerLabel(hardwarePadMode)}
        </div>
      )}
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
                title={
                  padMode === "hotcue"
                    ? cue != null
                      ? "Jump to cue · Shift-click to clear"
                      : "Set hot cue here"
                    : `Loop ${beats} beat${beats === 1 ? "" : "s"}`
                }
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
        <div className="manual-loop tray">
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
        <div className="pitch-tools tray">
          <button
            disabled={!title}
            className={
              deck.normalization_enabled !== false
                ? "keylock enabled"
                : "keylock"
            }
            title={`Programme loudness normalization toward -18 LUFS; measured source true peak capped at -1 dBTP. Applied ${(deck.normalization_gain_db ?? 0).toFixed(1)} dB, separate from trim.`}
            aria-pressed={deck.normalization_enabled !== false}
            onClick={() =>
              send("set_normalization", {
                enabled: deck.normalization_enabled === false,
              })
            }
          >
            LEVEL {(deck.normalization_gain_db ?? 0).toFixed(1)} dB
          </button>
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
  engine: initialEngine,
  command,
  controllerSurface,
}: {
  engine: EngineSnapshot;
  command: (c: Command, revision?: number) => void;
  controllerSurface?: ControllerSurface;
}) {
  const controls = useSyncExternalStore(
    engineStore.subscribeControls,
    engineStore.getControls,
  );
  const engine =
    controls.running === initialEngine.running ? controls : initialEngine;
  const [fxType, setFxType] = useState<"echo" | "reverb">("echo");
  useControl(
    "mixer",
    { "fx-view": (v) => setFxType(controlEnum(v, ["echo", "reverb"])) },
    { fx_view: fxType },
  );
  return (
    <div className="mixer-column">
      <section className="mixer">
        {controllerSurface && (
          <div
            className="controller-fx-status"
            aria-label="FLX4 effects status"
          >
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
                tone="trim"
                value={val(deck.gain, 1)}
                onChange={(gain) =>
                  command({ type: "set_deck_gain", deck: i, gain })
                }
              />
              <Knob
                label="HIGH"
                tone="high"
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
                tone="mid"
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
                tone="low"
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
                tone="filter"
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
                <Meter deckId={i} />
              </div>
            </div>
          ))}
        </div>
        <div className="master-controls">
          <Knob
            label="MASTER"
            tone="master"
            max={2}
            value={val(engine.master_gain, 1)}
            onChange={(gain) => command({ type: "set_master_gain", gain })}
          />
          <Knob
            label="PHONES"
            tone="aux"
            max={2}
            value={val(engine.cue_gain, 1)}
            onChange={(gain) => command({ type: "set_cue_gain", gain })}
          />
          <Knob
            label="CUE / MIX"
            tone="aux"
            max={1}
            reset={0}
            value={val(engine.cue_mix)}
            onChange={(mix) => command({ type: "set_cue_mix", mix })}
          />
        </div>
        <OutputProtection />
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
    </div>
  );
}
export default function App() {
  engineStore.diagnostics.app_renders++;
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
  const [pendingLoads, setPendingLoads] = useState<(LoadingTrack | null)[]>([
    null,
    null,
  ]);
  const [librarySource, setLibrarySource] = useState<
    "local" | "spotify" | "ytmusic" | "next"
  >("local");
  const [spotifyCollectionHost, setSpotifyCollectionHost] =
    useState<HTMLDivElement | null>(null);
  const activateSpotify = useCallback(() => setLibrarySource("spotify"), []);
  const [ytmusicCollectionHost, setYtmusicCollectionHost] =
    useState<HTMLDivElement | null>(null);
  const activateYtmusic = useCallback(() => setLibrarySource("ytmusic"), []);
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
  const [appMenu, setAppMenu] = useState(false);
  const appMenuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!appMenu) return;
    const close = (e: PointerEvent) => {
      if (!appMenuRef.current?.contains(e.target as Node)) setAppMenu(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [appMenu]);
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
  const engine = useSyncExternalStore(
    engineStore.subscribeStructural,
    engineStore.getStructural,
  );
  const snapshotTime = engineStore.receivedAt();
  const metadataLoads = useRef<number[] | undefined>(undefined);
  const trackMetadata = useRef<(TrackRecord | null)[]>([null, null]);
  const metadataRevision = useRef(-1);
  const metadataHydrated = useRef(false);

  const deckLoading = engine.decks.map(
    (_, deck) =>
      pendingLoads[deck] ||
      (sourceJobs[deck]?.preparing
        ? {
            source: sourceJobs[deck].uri || "",
            title: "Spotify track",
            cancellable: true,
          }
        : null),
  );

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
    control<ControllerSurface>("controller.state")
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
  const [queueNotice, setQueueNotice] = useState<{
    queue_id: string;
    state: string;
    reason?: string;
    decision_required?: boolean;
  } | null>(null);
  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    const save = (payload: {
      queue_id: string;
      state: string;
      reason?: string;
      decision_required?: boolean;
    }) => {
      if (!disposed) setQueueNotice(payload);
    };
    listen<{
      queue_id: string;
      state: string;
      reason?: string;
      decision_required?: boolean;
    }>("mix-queue-notice", ({ payload }) => save(payload))
      .then(async (unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        stop = unlisten;
        const q = await control<{
          queue_id: string;
          state: string;
          reason?: string;
          decision_required?: boolean;
        }>("mix.queue.status", {});
        if (q.decision_required) save(q);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);
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
  const nextBrowser = useRef<ControllerBrowserHandle | null>(null);
  const ytmusicBrowser = useRef<ControllerBrowserHandle | null>(null);
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
  const registerNextBrowser = useCallback(
    (handle: ControllerBrowserHandle | null) => {
      nextBrowser.current = handle;
    },
    [],
  );
  const registerYtmusicBrowser = useCallback(
    (handle: ControllerBrowserHandle | null) => {
      ytmusicBrowser.current = handle;
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
  const [libraryFilters, setLibraryFilters] = useState<LibraryFilters>({});
  const localLibrary = useLocalLibrary(
    search,
    sort,
    playlist,
    fail,
    libraryFilters,
  );
  const {
    tracks: library,
    setTracks: setLibrary,
    reload: reloadLibrary,
    loadMore: loadMoreLibrary,
  } = localLibrary;
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
      const s = await control<AppState>("app.state", { library: false });
      const latest = engineStore.getTransport();
      if (
        !metadataHydrated.current &&
        (!latest.load_revisions ||
          s.engine.load_revisions?.every(
            (r, i) => r === latest.load_revisions?.[i],
          ))
      ) {
        trackMetadata.current = s.engine.decks.map((d, i) =>
          stabilizeTrack(trackMetadata.current[i], d.track ?? null),
        );
        metadataLoads.current = s.engine.load_revisions;
        metadataHydrated.current = true;
        engineStore.publish(
          withTrackMetadata(
            latest.load_revisions ? latest : s.engine,
            trackMetadata.current,
            metadataLoads.current,
          ),
          engineStore.receivedAt() || performance.now(),
        );
      }
      reloadLibrary();
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
  }, [fail, hydrateAudio, reloadLibrary]);
  useEffect(() => {
    refresh();
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    let metadataPending = false;
    const fetchMetadata = async () => {
      if (metadataPending) return;
      metadataPending = true;
      try {
        const started = performance.now();
        const metadata = await control<EngineMetadata>("engine.metadata");
        metadataTimings.record(performance.now() - started);
        if (cancelled) return;
        trackMetadata.current = metadata.tracks.map((t, i) =>
          stabilizeTrack(trackMetadata.current[i], t),
        );
        metadataLoads.current = metadata.load_revisions;
        metadataRevision.current = metadata.revision;
        metadataHydrated.current = true;
        if (metadata.samples)
          setSampleNames(
            metadata.samples.map((t, i) => t?.title || `Sample ${i + 1}`),
          );
        // Metadata arrives separately; attach it only to the matching source epoch.
        engineStore.publish(
          withTrackMetadata(
            engineStore.getTransport(),
            trackMetadata.current,
            metadataLoads.current,
          ),
          engineStore.receivedAt(),
        );
      } catch (error) {
        if (!cancelled) fail(error);
      } finally {
        metadataPending = false;
      }
    };
    const poll = async () => {
      const started = performance.now();
      let delay = 33;
      try {
        if (isDesktop && !document.hidden) {
          const s = await control<EngineTransport>("engine.transport");
          transportTimings.record(performance.now() - started);
          if (cancelled) return;
          engineStore.publish(
            withTrackMetadata(s, trackMetadata.current, metadataLoads.current),
            performance.now(),
          );
          // A large metadata delivery never holds up clock delivery.
          if (
            typeof s.metadata_revision === "number"
              ? s.metadata_revision !== metadataRevision.current
              : metadataRevision.current < 0 ||
                !s.load_revisions?.every(
                  (r, i) => r === metadataLoads.current?.[i],
                )
          )
            void fetchMetadata();
        }
      } catch (error) {
        if (!cancelled) fail(error);
        delay = 1000;
      }
      if (!cancelled)
        timer = setTimeout(
          poll,
          Math.max(0, delay - (performance.now() - started)),
        );
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
      control<
        Pick<
          AppState,
          | "midi"
          | "analysis"
          | "smart_modes"
          | "spotify_decks"
          | "audio_status"
          | "audio_devices"
        >
      >("app.activity")
        .then((activity) => {
          if (disposed) return;
          setMidi((old) => retainUnchanged(old, activity.midi));
          setAnalysis((old) => retainUnchanged(old, activity.analysis));
          setSmartModes((old) => retainUnchanged(old, activity.smart_modes));
          setSourceJobs((old) =>
            retainUnchanged(old, activity.spotify_decks || []),
          );
          hydrateAudio(activity.audio_status);
          setDevices((old) => retainUnchanged(old, activity.audio_devices));
        })
        .catch(fail);
    }, 1500);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, [fail]);
  useEffect(() => {
    if (analysis && (analysis.active || analysis.completed > 0)) void refresh();
  }, [analysis?.active, analysis?.completed, refresh]);
  const waveformCommand = useCallback(
    (c: Command, revision?: number) =>
      control<void>(
        "engine.command",
        { command: c },
        { expected_load_revision: revision },
      ).catch((error) => {
        fail(error);
        throw error;
      }),
    [fail],
  );
  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let stop: UnlistenFn | undefined;
    const pendingKinds = new Set<string>();
    let fetching = false;
    const fetchChanges = async () => {
      if (fetching) return;
      fetching = true;
      try {
        while (!disposed && pendingKinds.size) {
          const kinds = new Set(pendingKinds);
          pendingKinds.clear();
          // Cue revisions are delivered through engine.metadata; refreshing the
          // entire app here would throw away immutable waveform tiles.
          if (
            kinds.has("library") ||
            kinds.has("analysis") ||
            kinds.has("details")
          ) {
            if (!disposed) reloadLibrary();
          } else if (kinds.has("metadata") && !disposed) {
            reloadLibrary(true);
          }
          if (kinds.has("playlists") || kinds.has("library")) {
            const next = await control<Playlist[]>("playlist.list");
            if (!disposed) setPlaylists(next);
          }
        }
      } catch (error) {
        if (!disposed) fail(error);
      } finally {
        fetching = false;
      }
    };
    listen<{ kind?: string; command?: string }>(
      "library-updated",
      ({ payload }) => {
        if (payload?.kind === "cues") metadataRevision.current = -1;
        const details = [
          "library.edit_details",
          "library.edit_taxonomy",
          "library.write_metadata",
        ].includes(payload?.command || "");
        pendingKinds.add(details ? "details" : payload?.kind || "library");
        void fetchChanges();
      },
    )
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
    (c: Command, revision?: number) => {
      const deck = typeof c.deck === "number" ? c.deck : undefined;
      control(
        "engine.command",
        { command: c },
        {
          expected_load_revision:
            revision ??
            (deck === undefined
              ? undefined
              : engineStore.getTransport().load_revisions?.[deck]),
        },
      ).catch(fail);
    },
    [fail],
  );
  const setControllerTempoRange = useCallback(
    (deck: number, range: number) => {
      if (isDesktop)
        control("controller.tempo_range", { deck, range }).catch(fail);
    },
    [fail],
  );
  const loadSpotify = useCallback(
    async (deck: number, track: Pick<SpotifyTrack, "uri" | "title">) => {
      const request = ++deckLoadRequest.current[deck];
      setDrawerOpen(false);
      setPendingLoads((old) =>
        old.map((item, i) =>
          i === deck
            ? { source: track.uri, title: track.title, cancellable: true }
            : item,
        ),
      );
      setSourceJobs((old) =>
        [0, 1].map((i) =>
          i === deck
            ? { preparing: true, uri: track.uri }
            : old[i] || { preparing: false },
        ),
      );
      try {
        await control("deck.load", { deck, uri: track.uri });
        if (deckLoadRequest.current[deck] !== request) return;
        await refresh();
        announce(`Loaded ${track.title} on deck ${deck === 0 ? "A" : "B"}`);
      } catch (error) {
        if (deckLoadRequest.current[deck] !== request) return;
        await refresh();
        throw error;
      } finally {
        if (deckLoadRequest.current[deck] === request)
          setPendingLoads((old) =>
            old.map((item, i) => (i === deck ? null : item)),
          );
      }
    },
    [refresh, announce, setDrawerOpen],
  );
  const cancelLoad = useCallback(
    async (deck: number) => {
      const request = ++deckLoadRequest.current[deck];
      try {
        await control("deck.load.cancel", { deck });
        if (deckLoadRequest.current[deck] !== request) return;
        setPendingLoads((old) =>
          old.map((item, i) => (i === deck ? null : item)),
        );
        await refresh();
      } catch (error) {
        if (deckLoadRequest.current[deck] === request) {
          setPendingLoads((old) =>
            old.map((item, i) => (i === deck ? null : item)),
          );
          await refresh();
          fail(error);
        }
      }
    },
    [refresh, fail],
  );
  const startImport = useCallback(
    async (kind: "files" | "folder") => {
      if (analysis?.active || busy) return;
      setBusy(true);
      try {
        const tracks = await control<{ id: number }[]>("library.import", {
          dialog: true,
          kind,
          summary: true,
        });
        if (tracks.length) {
          await refresh();
          announce(
            `Imported ${tracks.length} track${tracks.length === 1 ? "" : "s"}`,
          );
        }
      } catch (e) {
        fail(e);
      } finally {
        setBusy(false);
      }
    },
    [analysis?.active, busy, refresh, announce, fail],
  );
  const importTracks = useCallback(() => startImport("files"), [startImport]);
  const importFolder = useCallback(() => startImport("folder"), [startImport]);
  const load = useCallback(
    async (deck: number, path: string) => {
      const request = ++deckLoadRequest.current[deck];
      const track = library.find(
        (t) => t.path === path || `local:${t.id}` === path,
      );
      setDrawerOpen(false);
      setPendingLoads((old) =>
        old.map((item, i) =>
          i === deck
            ? {
                source: path,
                title: track?.title || path.split(/[\\/]/).pop() || "Track",
                artist: track?.artist ?? undefined,
                cancellable: false,
              }
            : item,
        ),
      );
      try {
        await control("deck.load", { deck, path });
        if (deckLoadRequest.current[deck] !== request) return;
        await refresh();
        announce(`Track loaded to deck ${deck === 0 ? "A" : "B"}`);
      } catch (e) {
        if (deckLoadRequest.current[deck] !== request) return;
        await refresh();
        fail(e);
      } finally {
        if (deckLoadRequest.current[deck] === request)
          setPendingLoads((old) =>
            old.map((item, i) => (i === deck ? null : item)),
          );
      }
    },
    [refresh, announce, fail, library, setDrawerOpen],
  );
  const demo = useCallback(
    async (deck: number) => {
      const request = ++deckLoadRequest.current[deck];
      setDrawerOpen(false);
      setPendingLoads((old) =>
        old.map((item, i) =>
          i === deck
            ? {
                source: `demo:${deck}`,
                title: "Generated test tone",
                cancellable: false,
              }
            : item,
        ),
      );
      try {
        await control("deck.demo", { deck });
        if (deckLoadRequest.current[deck] !== request) return;
        await refresh();
        announce("Generated test tone loaded");
      } catch (e) {
        if (deckLoadRequest.current[deck] !== request) return;
        fail(e);
      } finally {
        if (deckLoadRequest.current[deck] === request)
          setPendingLoads((old) =>
            old.map((item, i) => (i === deck ? null : item)),
          );
      }
    },
    [refresh, announce, fail, setDrawerOpen],
  );
  const reanalyze = useCallback(
    async (deck: number, revision?: number) => {
      await control(
        "deck.analyze",
        { deck },
        {
          expected_load_revision:
            revision ?? engineStore.getTransport().load_revisions?.[deck],
        },
      );
      await refresh();
      announce("Beat, key and waveform analysis updated");
    },
    [refresh, announce],
  );
  const recordingActive = !!(recording.recording || recording.active);
  useEffect(() => {
    if (!isDesktop) return;
    control<RecordingStatus>("recording.status")
      .then(setRecording)
      .catch(() => {});
    if (!recordingActive) return;
    const timer = setInterval(async () => {
      try {
        setRecording(await control<RecordingStatus>("recording.status"));
      } catch {}
    }, 1000);
    return () => clearInterval(timer);
  }, [recordingActive]);
  async function toggleRecording() {
    try {
      const status = await control<RecordingStatus | null>(
        recordingActive ? "recording.stop" : "recording.start",
        recordingActive ? {} : { dialog: true },
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
  const visible = library;
  const controllerContext = useRef<TrackRecord[] | null>(null);
  const syncControllerSelection = useCallback(
    async (id: string | number, tracks: TrackRecord[]) => {
      if (controllerContext.current !== tracks) {
        await control("controller.context", {
          track_ids: tracks.map((t) => t.id),
          selected_track_id: id,
        });
        controllerContext.current = tracks;
      } else await control("controller.selection", { track_id: id });
    },
    [],
  );
  useEffect(() => {
    if (!isDesktop) return;
    let canceled = false;
    let unlisten: UnlistenFn | undefined;
    listen<number>("controller_selection", (event) => {
      setSelection(event.payload);
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
          const sources = ["local", "next"] as const;
          return sources[
            browseIndex(
              sources,
              source === "next" ? "next" : "local",
              action.delta,
            )
          ];
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
      if (librarySource !== "local") {
        const browser =
          librarySource === "spotify"
            ? spotifyBrowser
            : librarySource === "ytmusic"
              ? ytmusicBrowser
              : nextBrowser;
        await browser.current?.handle({
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
        const browser =
          librarySource === "next"
            ? nextBrowser
            : librarySource === "ytmusic"
              ? ytmusicBrowser
              : spotifyBrowser;
        await browser.current?.handle({
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
        setPendingLoads((old) =>
          old.map((item, i) => (i === action.deck ? null : item)),
        );
        await control("deck.instant_double", { deck: action.deck });
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
    if (
      action.type === "browse" &&
      browserNavigationRef.current.layer === "playlists" &&
      librarySource !== "next"
    ) {
      const buttons = Array.from(
        document.querySelectorAll<HTMLButtonElement>(
          ".unified-library > aside [data-local-collection], .unified-library > aside [data-browser-id]",
        ),
      );
      const selected =
        document.activeElement instanceof HTMLButtonElement &&
        buttons.includes(document.activeElement)
          ? document.activeElement
          : buttons.find((b) =>
              librarySource === "local"
                ? b.dataset.localCollection === String(playlist ?? "all")
                : b.getAttribute("aria-pressed") === "true",
            );
      const index = browseIndex(buttons, selected, action.delta);
      if (index >= 0) {
        buttons[index].click();
        buttons[index].focus({ preventScroll: true });
        buttons[index].scrollIntoView({ block: "nearest" });
      }
      return;
    }
    if (librarySource === "next") {
      await nextBrowser.current?.handle(action);
      return;
    }
    if (librarySource === "ytmusic") {
      if (ytmusicBrowser.current) await ytmusicBrowser.current.handle(action);
      else
        announce(
          "Opening the YouTube Music browser. Select a playlist once it loads.",
        );
      return;
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
        let currentTracks = visible;
        const currentIndex = visible.findIndex(
          (track) => track.id === selectedLocalRow.current,
        );
        if (
          localLibrary.hasMore &&
          currentIndex + action.delta >= visible.length - 5
        )
          currentTracks = await loadMoreLibrary();
        const ids = currentTracks.map((track) => track.id);
        const index = browseIndex(ids, selectedLocalRow.current, action.delta);
        if (index >= 0) {
          setSelection(ids[index]);
          await syncControllerSelection(ids[index], currentTracks);
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
      void syncControllerSelection(id, visible).catch(fail);
    },
    [fail, visible, syncControllerSelection],
  );
  useEffect(() => {
    let current = true;
    controllerContext.current = null;
    if (isDesktop)
      control("controller.context", {
        track_ids: visible.map((t) => t.id),
      })
        .then(() => {
          if (current) controllerContext.current = visible;
        })
        .catch(fail);
    return () => {
      current = false;
    };
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
  const localTracksPane = useRef<HTMLDivElement>(null);
  const localTrackKey = useCallback(
    (index: number) => visible[index].id,
    [visible],
  );
  const renderTrackRow = useCallback(
    (i: number, measure: (node: HTMLTableRowElement | null) => void) => {
      const track = visible[i];
      return (
        <tr
          ref={measure}
          data-index={i}
          aria-rowindex={i + 2}
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
              <TrackArtwork localId={track.id} />
              {track.rating || track.color ? (
                <span
                  className="track-classification"
                  aria-label={`${track.rating || 0} stars${track.color ? `, color ${track.color}` : ""}`}
                >
                  <span
                    className="track-color"
                    style={{ background: track.color || "transparent" }}
                  />
                  {track.rating ? "★".repeat(track.rating) : ""}
                </span>
              ) : null}
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
      );
    },
    [visible, selection, load, selectTrack, saveTags, engine.decks[0]?.playing],
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
        importFolder();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [command, settings, importFolder, librarySource, fail]);
  async function refreshPorts() {
    try {
      const ports = await control<{ inputs: string[]; outputs: string[] }>(
        "midi.ports",
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
      control<AudioDeviceInfo[]>("audio.devices").then(setDevices),
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
      await control("audio.start", {
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
        setTheme(controlEnum(v, ["aluminium", "ivory", "graphite"])),
      source: (v) =>
        setLibrarySource(controlEnum(v, ["local", "spotify", "ytmusic"])),
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
        await syncControllerSelection(track.id, visible);
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
        await control("playlist.create", { name: playlistName });
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
            "ytmusic",
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
        await control("audio.start", {
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
          await control("midi.connect", {
            input_name: midiInput,
            output_name: midiOutput || null,
          }),
        );
      },
      "ports-refresh": refreshPorts,
      refresh,
      "import-dialog": importFolder,
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
  const importActive = analysis?.active ?? false;
  const importCurrentFile = analysis?.current_path?.split(/[\\/]/).pop();
  return (
    <div className="app-shell">
      <PerformanceDiagnostics />
      <header className="topbar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <svg
            className="brand-mark"
            viewBox="0 0 22 22"
            aria-hidden="true"
            data-tauri-drag-region
          >
            {BRAND_DOTS.map(([x, y]) => (
              <circle
                key={`${x}-${y}`}
                cx={x}
                cy={y}
                r={1.45}
                className={x === 11 && y === 11 ? "live" : undefined}
              />
            ))}
          </svg>
          <strong data-tauri-drag-region>PulseDeck</strong>
        </div>
        <div className="topbar-center" data-tauri-drag-region />
        <div className="top-actions">
          {recordingActive && (
            <button
              className="record-live"
              onClick={toggleRecording}
              title={
                recording.dropped_frames
                  ? `Recording · ${recording.dropped_frames} dropped frames · click to stop`
                  : "Recording the mix · click to stop"
              }
            >
              <Circle size={8} fill="currentColor" /> REC
            </button>
          )}
          {(busy || importActive) && (
            <span
              className="import-progress"
              role="status"
              title={
                importCurrentFile
                  ? `${analysis?.analyzing ? "Analyzing" : "Importing"} ${importCurrentFile}`
                  : undefined
              }
            >
              <LoaderCircle size={11} className="spin" />
              {importActive
                ? `${analysis?.analyzing ? "Analyzing" : "Importing"} ${analysis!.completed}/${analysis!.total}`
                : "Choosing music…"}
            </span>
          )}
          <div
            className="browser-dock tray"
            aria-label="Music library"
            onKeyDownCapture={browserNavigationKey}
          >
            <button
              data-browser-source={
                librarySource === "next" ? "local" : librarySource
              }
              aria-pressed={drawerOpen && librarySource !== "next"}
              title={
                drawerOpen && librarySource !== "next"
                  ? "Hide Library"
                  : "Open Library"
              }
              className={`${drawerOpen && librarySource !== "next" ? "active" : ""} ${browserNavigation.layer === "footer" && librarySource !== "next" ? "controller-focused" : ""}`}
              onClick={() => {
                if (librarySource === "next") setLibrarySource("local");
                setDrawerOpen(!(drawerOpen && librarySource !== "next"));
              }}
            >
              <Library size={13} /> Library
            </button>
            <button
              data-browser-source="next"
              aria-pressed={drawerOpen && librarySource === "next"}
              title={
                drawerOpen && librarySource === "next"
                  ? "Hide next-move suggestions"
                  : "Suggest the next move"
              }
              className={`${drawerOpen && librarySource === "next" ? "active" : ""} ${browserNavigation.layer === "footer" && librarySource === "next" ? "controller-focused" : ""}`}
              onClick={() => {
                setLibrarySource("next");
                setDrawerOpen(!(drawerOpen && librarySource === "next"));
              }}
            >
              <SkipForward size={13} /> Suggest
            </button>
          </div>
          <div
            className="app-menu"
            ref={appMenuRef}
            onKeyDown={(e) => {
              if (e.key === "Escape" && appMenu) {
                e.stopPropagation();
                setAppMenu(false);
              }
            }}
          >
            <button
              className={`icon-button app-menu-trigger ${audioRunning ? "" : "alert"}`}
              aria-label="Menu"
              aria-haspopup="menu"
              aria-expanded={appMenu}
              onClick={() => setAppMenu((open) => !open)}
            >
              <Settings2 size={15} />
              <ChevronDown size={11} />
            </button>
            {appMenu && (
              <div className="dropdown app-menu-list" role="menu">
                <button
                  role="menuitem"
                  className={`menu-status ${audioRunning ? "connected" : ""}`}
                  onClick={() => {
                    setAppMenu(false);
                    openSettings();
                    setSettingsTab("audio");
                  }}
                >
                  <span className="tiny-dot" />
                  {audioRunning ? "Audio online" : "Audio stopped"}
                  <small>
                    {engine.sample_rate
                      ? `${(engine.sample_rate / 1000).toFixed(1)} kHz`
                      : "set up"}
                  </small>
                </button>
                <button
                  role="menuitem"
                  className={`menu-status ${midi.connected ? "connected" : ""}`}
                  onClick={() => {
                    setAppMenu(false);
                    openSettings();
                    setSettingsTab("midi");
                  }}
                >
                  <span className="tiny-dot" />
                  {midi.connected
                    ? midi.input_name || "Controller connected"
                    : "Controller offline"}
                  <small>{midi.connected ? "midi" : "connect"}</small>
                </button>
                <hr />
                <button
                  role="menuitem"
                  className={recordingActive ? "is-recording" : ""}
                  disabled={
                    (!audioRunning ||
                      engine.decks.some(
                        (d) => d.track?.recording_allowed === false,
                      )) &&
                    !recordingActive
                  }
                  title={
                    engine.decks.some(
                      (d) => d.track?.recording_allowed === false,
                    )
                      ? "Recording is unavailable while a Spotify source is loaded"
                      : "Record the control deck and sampler mix as WAV"
                  }
                  onClick={() => {
                    toggleRecording();
                    setAppMenu(false);
                  }}
                >
                  {recordingActive ? (
                    <Square size={12} fill="currentColor" />
                  ) : (
                    <Circle size={12} fill="currentColor" />
                  )}
                  {recordingActive ? "Stop recording" : "Record mix"}
                  <small>WAV</small>
                </button>
                <button
                  role="menuitemcheckbox"
                  aria-checked={showSampler}
                  onClick={() => {
                    setShowSampler(!showSampler);
                    setAppMenu(false);
                  }}
                >
                  <Zap size={13} />
                  Sampler
                  {showSampler && <Check size={13} />}
                </button>
                <button
                  role="menuitem"
                  disabled={busy || !!analysis?.active}
                  onClick={() => {
                    setAppMenu(false);
                    void importFolder();
                  }}
                >
                  <FolderPlus size={13} />
                  {busy || analysis?.active
                    ? `${analysis?.analyzing ? "Analyzing" : "Importing"}${analysis?.active ? ` ${analysis.completed}/${analysis.total}` : "…"}`
                    : "Import folder…"}
                </button>
                <button
                  role="menuitem"
                  disabled={busy || !!analysis?.active}
                  onClick={() => {
                    setAppMenu(false);
                    void importTracks();
                  }}
                >
                  <Plus size={13} />
                  Import files…
                </button>
                <button
                  role="menuitem"
                  onClick={() => {
                    setAppMenu(false);
                    openSettings();
                  }}
                >
                  <Settings2 size={13} />
                  Settings…
                </button>
              </div>
            )}
          </div>
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
              loading={deckLoading[0]}
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
              loading={deckLoading[1]}
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
                loading={deckLoading[0]}
                loadError={sourceJobs[0]?.error}
                cancelLoad={cancelLoad}
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
                loading={deckLoading[1]}
                loadError={sourceJobs[1]?.error}
                cancelLoad={cancelLoad}
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
                              const sample = await control<unknown>(
                                "sampler.load",
                                { slot, dialog: true },
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
        title={librarySource === "next" ? "Suggest" : "Library"}
        icon={
          librarySource === "next" ? (
            <SkipForward size={17} aria-hidden="true" />
          ) : (
            <Library size={17} aria-hidden="true" />
          )
        }
      >
        {librarySource === "next" ? (
          <SuggestPanel
            engine={engine}
            active={drawerOpen}
            libraryRevision={library}
            controllerLayer={browserNavigation.layer}
            registerControllerBrowser={registerNextBrowser}
            onBrowserPaneChange={navigate}
            onError={fail}
          />
        ) : (
          <div className="unified-library" data-source={librarySource}>
            <aside
              className={`controller-playlists-pane ${localCollectionFocus && browserNavigation.layer !== "close" ? "pane-active" : ""}`}
              aria-label="Library collections"
            >
              <div className="browser-pane-heading">LOCAL</div>
              {[{ id: null, name: "All tracks" }, ...playlists].map(
                (collection) => (
                  <button
                    key={collection.id ?? "all"}
                    data-local-collection={collection.id ?? "all"}
                    className={
                      librarySource === "local" && playlist === collection.id
                        ? "controller-selected"
                        : ""
                    }
                    onClick={() => {
                      setLibrarySource("local");
                      setPlaylist(collection.id);
                      setSelection(null);
                      setLocalCollectionFocus(true);
                    }}
                  >
                    {collection.name}
                  </button>
                ),
              )}
              <div className="browser-pane-heading">SPOTIFY</div>
              <button
                className="spotify-library-home"
                onClick={activateSpotify}
              >
                <Radio size={13} /> Spotify overview
              </button>
              <div
                className="spotify-collection-host"
                ref={setSpotifyCollectionHost}
              />
              <div className="browser-pane-heading">YOUTUBE MUSIC</div>
              <button
                className="spotify-library-home"
                onClick={activateYtmusic}
              >
                <ListMusic size={13} /> YouTube Music overview
              </button>
              <div
                className="ytmusic-collection-host"
                ref={setYtmusicCollectionHost}
              />
            </aside>
            <div className="library-content">
              <div
                className="library-source-view"
                hidden={librarySource !== "spotify"}
              >
                <SpotifyPanel
                  compact
                  collectionHost={spotifyCollectionHost}
                  onActivate={activateSpotify}
                  onLoadDeck={loadSpotify}
                  deckJobs={sourceJobs}
                  registerControllerBrowser={registerSpotifyBrowser}
                  controllerLayer={
                    librarySource === "spotify"
                      ? browserNavigation.layer
                      : "home"
                  }
                  onBrowserPaneChange={navigate}
                />
              </div>
              <div
                className="library-source-view"
                hidden={librarySource !== "ytmusic"}
              >
                <YouTubeMusicPanel
                  active={drawerOpen && librarySource === "ytmusic"}
                  collectionHost={ytmusicCollectionHost}
                  onActivate={activateYtmusic}
                  onLoadSpotify={loadSpotify}
                  onLoadLocal={load}
                  deckJobs={sourceJobs}
                  registerControllerBrowser={registerYtmusicBrowser}
                  controllerLayer={
                    librarySource === "ytmusic"
                      ? browserNavigation.layer
                      : "home"
                  }
                  onBrowserPaneChange={navigate}
                />
              </div>
              <div
                className="library-source-view"
                hidden={librarySource !== "local"}
              >
                <section className="library-panel">
                  <div className="library-toolbar">
                    <div>
                      <h2>
                        {playlists.find((p) => p.id === playlist)?.name ||
                          "Your collection"}
                      </h2>
                      <span>
                        {localLibrary.total} tracks
                        {library.length > 0
                          ? ` · ${fmt(localLibrary.duration)} total`
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
                  <LibraryTools
                    selectedId={selected?.id ?? null}
                    filters={libraryFilters}
                    onFilters={setLibraryFilters}
                    onSaved={reloadLibrary}
                  />
                  <div className="controller-browser-split">
                    <div
                      className={`controller-tracks-pane ${!localCollectionFocus && browserNavigation.layer !== "close" ? "pane-active" : ""}`}
                      data-browser-pane="local-tracks"
                      ref={localTracksPane}
                      tabIndex={-1}
                      onPointerDown={() => setLocalCollectionFocus(false)}
                    >
                      <div className="browser-pane-heading">
                        TRACKS{" "}
                        <small>Press BROWSE to return to playlists</small>
                      </div>
                      {newPlaylist && (
                        <form
                          className="drawer-playlist-form"
                          onSubmit={async (e) => {
                            e.preventDefault();
                            try {
                              await control("playlist.create", {
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
                        <table
                          role="table"
                          className="track-table local-track-table"
                        >
                          <thead>
                            <tr role="row">
                              <th role="columnheader" className="track-number">
                                #
                              </th>
                              <th role="columnheader">TITLE / ARTIST</th>
                              <th role="columnheader">BPM</th>
                              <th role="columnheader">KEY</th>
                              <th role="columnheader">TIME</th>
                              <th role="columnheader">DECK</th>
                              <th role="columnheader">Tags</th>
                            </tr>
                          </thead>
                          <VirtualTrackRows
                            count={visible.length}
                            getKey={localTrackKey}
                            scrollRef={localTracksPane}
                            selectedIndex={visible.findIndex(
                              (t) => t.id === selection,
                            )}
                            focusSelection={
                              drawerOpen &&
                              !localCollectionFocus &&
                              browserNavigation.layer === "tracks"
                            }
                            queryKey={localLibrary.queryKey}
                            hasMore={localLibrary.hasMore}
                            loading={localLibrary.loading}
                            onMore={loadMoreLibrary}
                          >
                            {renderTrackRow}
                          </VirtualTrackRows>
                        </table>
                        {!visible.length && !localLibrary.loading && (
                          <div className="empty-library">
                            <div className="empty-icon">
                              <Music2 size={26} />
                            </div>
                            <h3>
                              {search
                                ? "No matching tracks"
                                : importActive
                                  ? `${analysis?.analyzing ? "Analyzing" : "Importing"} ${analysis!.completed} of ${analysis!.total} tracks`
                                  : "A fresh start for your next set"}
                            </h3>
                            <p>
                              {search
                                ? "Try a different title, artist, BPM or tag."
                                : importActive
                                  ? importCurrentFile
                                    ? `${analysis?.analyzing ? "Analyzing" : "Importing"} ${importCurrentFile}`
                                    : "Preparing your music library…"
                                  : "Import local audio, then drag a track onto either deck."}
                            </p>
                            {!search && !importActive && (
                              <button
                                className="import-button"
                                onClick={importFolder}
                                disabled={busy || importActive}
                              >
                                <FolderPlus size={15} />
                                Import a music folder
                              </button>
                            )}
                            <span>
                              {isDesktop
                                ? "WAV · AIFF · MP3 · FLAC · AAC · OGG"
                                : "Desktop preview · control audio is available in the app"}
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
                                    await control("playlist.add", {
                                      playlist: Number(e.target.value),
                                      track: selected.id,
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
                                    await control("library.remove", {
                                      id: selected.id,
                                    });
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
              </div>
            </div>
          </div>
        )}
      </TrackDrawer>
      {queueNotice && (
        <div
          className="queue-outcome"
          role={queueNotice.decision_required ? "alert" : "status"}
        >
          <strong>
            {queueNotice.decision_required
              ? "Mix needs attention"
              : "Mix finished"}
          </strong>
          <span>
            {queueNotice.state.replaceAll("_", " ")}
            {queueNotice.reason
              ? ` · ${queueNotice.reason.replaceAll("_", " ")}`
              : ""}
          </span>
          <button
            type="button"
            onClick={() => {
              setLibrarySource("next");
              setDrawerOpen(true);
            }}
          >
            Open Suggest
          </button>
          <button
            type="button"
            aria-label="Dismiss mix outcome"
            onClick={() => setQueueNotice(null)}
          >
            Dismiss
          </button>
        </div>
      )}
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
                "ytmusic",
                "roadmap",
              ].map((t) => (
                <button
                  key={t}
                  className={settingsTab === t ? "selected" : ""}
                  onClick={() => setSettingsTab(t)}
                >
                  {t === "midi"
                    ? "Controller"
                    : t === "ytmusic"
                      ? "YouTube Music"
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
                    Each finish changes only the body. Screens, frequency bands
                    and cue colours stay the same, so the decks read identically
                    in every booth.
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
                              await control("mixer.smart_modes", {
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
                              await control("mixer.smart_modes", {
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
                          setDevices(await control("audio.devices"));
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
                              await control("audio.stop");
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
                          await control(
                            midi.connected ? "midi.disconnect" : "midi.connect",
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
                    <AudioLines size={18} />
                    <div>
                      <h3>Beat analysis</h3>
                      <p>
                        Imports add files and folders immediately. Beat, key and
                        waveform analysis runs in the background when you load a
                        track onto a deck, and is saved for next time.
                      </p>
                    </div>
                  </div>
                  <div className="migration-card">
                    <h4>Update library analysis</h4>
                    <p>
                      Update beat, key and waveform analysis. Existing Beat
                      This! timing is reused; missing analysis is computed in
                      the background. Manual BPM and grid edits are kept.
                    </p>
                    {analysis?.active ? (
                      <button
                        className="secondary"
                        onClick={() =>
                          control("library.analysis.cancel").catch(fail)
                        }
                      >
                        Cancel ({analysis.completed}/{analysis.total})
                      </button>
                    ) : (
                      <button
                        className="secondary"
                        disabled={busy}
                        onClick={async () => {
                          try {
                            const updated = await control<TrackRecord[]>(
                              "library.analysis.update",
                            );
                            await refresh();
                            announce(
                              updated.length
                                ? `Beat analysis updated ${updated.length} track${updated.length === 1 ? "" : "s"}`
                                : "Every track already has beat analysis",
                            );
                          } catch (e) {
                            fail(e);
                          }
                        }}
                      >
                        Update library analysis
                      </button>
                    )}
                  </div>
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
                          const imported = await control<{ id: number }[]>(
                            "library.import_xml",
                            { dialog: true, summary: true },
                          );
                          if (imported.length) {
                            await refresh();
                            announce("XML collection imported");
                          }
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
                          const exported = await control<{
                            path: string | null;
                          }>("library.export_xml", { dialog: true });
                          if (exported.path)
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
              {settingsTab === "ytmusic" && <YouTubeMusicPanel settingsOnly />}
              {settingsTab === "roadmap" && (
                <>
                  <div className="settings-section-title">
                    <AudioLines size={18} />
                    <div>
                      <h3>Build capabilities</h3>
                      <p>
                        This is the first control two-deck performance
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

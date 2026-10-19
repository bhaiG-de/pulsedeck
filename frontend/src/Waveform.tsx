import { useLayoutEffect, useRef } from "react";
import { animate } from "motion/mini";
import type { WaveformBands } from "./types";
import {
  hasWaveformBands,
  getWaveformWindow,
  expandWaveformWindow,
  timeToWaveformFraction,
  waveformSeekSeconds,
  nextCue,
  HOT_CUE_COLORS,
  CUE_ROLE_COLORS,
  CUE_ROLE_NAMES,
  UNLABELLED_CUE,
  type DisplayCue,
} from "./waveform-data";
import { registerWaveformPerformance } from "./performance";
import { engineStore } from "./engine-state";
import { WaveformMotion } from "./waveform-motion";
import {
  gridBarBeat,
  uniformBeatGrid,
  visibleGridBeats,
  type BeatGrid,
} from "./beat-grid";
import { WaveformRaster } from "./waveform-renderer";

/* Monochrome screen palette: faint white linework, a dotted baseline and a
   red playhead with a dot cap (as on a field recorder), orange loops (matching
   the loop key), blue labels. Band colours come from BAND_COLORS; hot-cue
   colours stay as analysed. */
const SCREEN = {
  background: "#0b0b0c",
  axis: "#4b4b4f",
  played: "rgba(11,11,12,.5)",
  playhead: "#ff3b30",
  idle: "#4b4b4f",
  idleText: "#6e6e72",
  windowFill: "rgba(242,242,240,.08)",
  windowLine: "rgba(242,242,240,.45)",
  beatLine: "rgba(242,242,240,.1)",
  beatTick: "#8e8e91",
  barLine: "rgba(242,242,240,.26)",
  bar: "#f2f2f0",
  loop: "#ff7a30",
  loopFill: "rgba(255,122,48,.12)",
  loopText: "#ffb084",
  memory: "#b8b8bb",
  labelFill: "rgba(11,11,12,.9)",
  barLabel: "#4a90ff",
  font: (size: number, weight = 400) =>
    `${weight} ${size}px "DM Mono", ui-monospace, monospace`,
};

export type WaveformProps = {
  deckId?: number;
  samples: number[];
  bands?: WaveformBands | null;
  position: number;
  duration: number;
  bpm: number;
  color: string;
  onSeek: (value: number) => void;
  zoom?: number;
  spanBeats?: number;
  cueSeconds?: number | null;
  cuePoint?: number | null;
  memoryCues?: (number | null)[];
  loopStart?: number | null;
  loopEnd?: number | null;
  loopActive?: boolean;
  overview?: boolean;
  hotcues?: (number | null)[];
  /** Saved cue markers, drawn as a coloured strip along the top edge. */
  cueMarkers?: DisplayCue[];
  beatGrid?: BeatGrid;
  beatgridOffset?: number;
  beatgridValid?: boolean;
  beatgridDownbeat?: boolean;
  playing?: boolean;
  playbackRate?: number;
  jogTouch?: boolean;
  sourceId?: string;
  loadRevision?: number;
  snapshotTime?: number;
  positionRevision?: number;
  onScrub?: (
    deltaSourceSeconds: number,
    elapsedMs: number,
  ) => void | Promise<void>;
  onScrubStart?: (positionAtGrab: number) => void | Promise<void>;
  onScrubEnd?: () => void | Promise<void>;
  onZoom?: (deltaY: number) => void;
};
interface Gesture {
  pointerId: number;
  lastX: number;
  originX: number;
  span: number;
  width: number;
  moved: boolean;
  pending: number;
  lastFlushTime: number;
  ready: boolean;
  failed: boolean;
  start: Promise<void>;
  inFlight: Promise<void> | null;
  scrub: (delta: number, elapsedMs: number) => void | Promise<void>;
  end?: () => void | Promise<void>;
}
interface Runtime {
  invalidate: () => void;
  down: (event: PointerEvent) => void;
  move: (event: PointerEvent) => void;
  up: (event: PointerEvent) => void;
  cancel: () => void;
  click: (event: MouseEvent) => void;
}
const empty: number[] = [];
const noCueMarkers: DisplayCue[] = [];

export default function Waveform(input: WaveformProps) {
  const props = useRef(input);
  props.current = input;
  const hostRef = useRef<HTMLDivElement>(null),
    canvasRef = useRef<HTMLCanvasElement>(null),
    markersRef = useRef<HTMLCanvasElement>(null),
    playedRef = useRef<HTMLDivElement>(null),
    playheadRef = useRef<HTMLDivElement>(null),
    labelRef = useRef<HTMLSpanElement>(null),
    pinnedRef = useRef<HTMLDivElement>(null),
    runtime = useRef<Runtime | null>(null);
  useLayoutEffect(() => {
    const canvas = hostRef.current!,
      waveCanvas = canvasRef.current!,
      markerCanvas = markersRef.current!,
      waveContext = waveCanvas.getContext("2d", { alpha: false }),
      markerContext = markerCanvas.getContext("2d");
    if (!waveContext || !markerContext) return;
    let ctx: CanvasRenderingContext2D = waveContext;
    let contentDirty = true,
      stripAnchor: number | null = null,
      stripPadding = 0,
      pixelsPerSecond = 0,
      lastDisplayPosition = 0,
      pinnedKey = "",
      stripAnimation: ReturnType<typeof animate> | null = null;
    const stopStrip = () => {
      stripAnimation?.cancel();
      stripAnimation = null;
    };
    const motion = new WaveformMotion(),
      raster = new WaveformRaster(undefined, () => {
        contentDirty = true;
        lastOverviewPixel = NaN;
        dirty = true;
        ensureFrame();
      });
    const metrics = registerWaveformPerformance(
      `${input.deckId ?? "preview"}-${input.overview ? "overview" : "detail"}`,
      () => raster.tileBuilds,
    );
    let disposed = false,
      frame = 0,
      width = 0,
      height = 0,
      dpr = window.devicePixelRatio || 1,
      lastFrame = -Infinity,
      lastPaintPosition = 0,
      lastOverviewPixel = NaN,
      lastPrewarmPosition = -Infinity,
      dirty = true,
      gesture: Gesture | null = null,
      finishing = 0,
      suppressClick = false;
    let nativeKey = "",
      prewarmTimer: ReturnType<typeof setTimeout> | null = null;
    const current = (): WaveformProps => {
      const p = props.current;
      if (p.deckId === undefined) return p;
      const engine = engineStore.getTransport(),
        deck = engine.decks[p.deckId];
      if (
        !deck ||
        deck.track?.path !== p.sourceId ||
        engine.load_revisions?.[p.deckId] !== p.loadRevision
      )
        return p;
      const delay =
        (engine.audio_quality as { output_latency_frames?: number } | undefined)
          ?.output_latency_frames ?? 0;
      const rate =
        deck.actual_playback_rate ??
        deck.playback_rate ??
        1 + (deck.tempo_percent ?? 0) / 100;
      return {
        ...p,
        position: Math.max(
          0,
          (deck.position_seconds ?? deck.position ?? 0) -
            (deck.playing ? (delay / (engine.sample_rate || 48000)) * rate : 0),
        ),
        playing: deck.playing,
        playbackRate: rate,
        jogTouch: deck.jog_touch,
        positionRevision: deck.position_revision,
        snapshotTime: engineStore.receivedAt(),
        hotcues: deck.hot_cues,
        memoryCues: deck.memory_cues,
        cuePoint: deck.cue_seconds,
        loopStart: deck.loop_start_seconds,
        loopEnd: deck.loop_end_seconds,
        loopActive: deck.loop_active,
      };
    };
    const sourceKey = () =>
      `${props.current.sourceId || ""}:${props.current.duration}:${props.current.spanBeats}:${props.current.bpm}:${props.current.zoom}:${width}:${height}:${dpr}`;
    const schedulePrewarm = (fn: () => void) => {
      if (prewarmTimer !== null) return;
      prewarmTimer = setTimeout(() => {
        prewarmTimer = null;
        fn();
      }, 0);
    };
    const ensureFrame = () => {
      if (!disposed && !frame) frame = requestAnimationFrame(tick);
    };
    const flush = (g: Gesture) => {
      if (!g.ready || g.failed || g.inFlight || Math.abs(g.pending) < 1e-9)
        return;
      const delta = g.pending;
      g.pending = 0;
      let request: Promise<void>;
      try {
        request = Promise.resolve(
          g.scrub(
            delta,
            Math.max(8, Math.min(80, performance.now() - g.lastFlushTime)),
          ),
        );
      } catch {
        g.failed = true;
        return;
      }
      g.lastFlushTime = performance.now();
      g.inFlight = request;
      request.then(
        () => {
          if (g.inFlight === request) g.inFlight = null;
          ensureFrame();
        },
        () => {
          g.failed = true;
          if (g.inFlight === request) g.inFlight = null;
          ensureFrame();
        },
      );
    };
    const finish = (cancelled = false) => {
      const g = gesture;
      if (!g) return;
      gesture = null;
      finishing++;
      suppressClick = true;
      void (async () => {
        try {
          await g.start;
          if (g.inFlight) await g.inFlight;
          if (!g.failed && Math.abs(g.pending) > 1e-9) {
            const delta = g.pending;
            g.pending = 0;
            await g.scrub(
              delta,
              Math.max(8, Math.min(80, performance.now() - g.lastFlushTime)),
            );
          }
        } catch {
          g.failed = true;
        } finally {
          motion.endGrab(performance.now());
          try {
            await g.end?.();
          } catch {
            g.failed = true;
          }
          finishing--;
          if (g.failed) motion.cancelGrab();
          dirty = true;
          ensureFrame();
        }
      })();
    };
    const moveStrip = (now: number) => {
      if (stripAnchor === null) return;
      const p = current(),
        position = motion.position(now);
      const transform = (time: number) =>
        `translateX(${(stripAnchor! - motion.position(time)) * pixelsPerSecond - stripPadding}px)`;
      stopStrip();
      for (const layer of [waveCanvas, markerCanvas])
        layer.style.transform = transform(now);
      if (!p.playing || p.jogTouch || gesture || finishing) return;
      let milliseconds = Math.max(0, 120 - (now - (p.snapshotTime ?? now)));
      const rate = Math.max(0, p.playbackRate ?? 1);
      if (rate > 0) {
        milliseconds = Math.min(
          milliseconds,
          ((p.duration - position) / rate) * 1000,
        );
        if (
          p.loopActive !== false &&
          p.loopEnd != null &&
          p.loopStart != null &&
          position >= p.loopStart &&
          position < p.loopEnd
        )
          milliseconds = Math.min(
            milliseconds,
            ((p.loopEnd - position) / rate) * 1000 - 0.01,
          );
      }
      if (milliseconds <= 0) return;
      // Linear source time, never a spring: the beatgrid must stay on the audio clock.
      stripAnimation = animate(
        [waveCanvas, markerCanvas],
        {
          transform: [
            transform(now),
            transform(now + milliseconds / 2),
            transform(now + milliseconds),
          ],
        },
        { duration: milliseconds / 1000, ease: "linear" },
      );
    };
    const paint = (now: number) => {
      const started = performance.now();
      if (disposed || width <= 0 || height <= 0) return;
      const {
        samples,
        bands,
        duration,
        bpm,
        color,
        zoom = 1,
        spanBeats,
        overview = false,
        hotcues = empty,
        cueMarkers = noCueMarkers,
        beatGrid,
        beatgridOffset = 0,
        beatgridValid = false,
        beatgridDownbeat = false,
        cuePoint,
        cueSeconds,
        memoryCues = empty,
        loopStart,
        loopEnd,
        loopActive,
      } = current();
      const position = motion.position(now),
        h = height,
        strip = !overview && duration > 0 && samples.length > 0,
        viewport = getWaveformWindow({
          position,
          duration,
          bpm,
          zoom,
          overview,
          spanBeats,
        }),
        padding = strip ? Math.min(width / 4, 512) : 0,
        w = width + padding * 2;
      const grid =
        beatGrid ??
        uniformBeatGrid(bpm, beatgridOffset, beatgridValid, beatgridDownbeat);
      playedRef.current!.hidden = !strip;
      playheadRef.current!.hidden = !strip;
      markerCanvas.hidden = !!overview;
      pinnedRef.current!.hidden = !strip;
      if (strip) {
        // Labels that entered before the visible window stay pinned at its edge.
        const pinned = cueMarkers.flatMap((cue) => {
          if (
            cue.start >= viewport.start ||
            cue.end <= viewport.start ||
            !(cue.name || cue.label)
          )
            return [];
          const text = (cue.name || CUE_ROLE_NAMES[cue.label!]).toUpperCase();
          markerContext.font = SCREEN.font(9, 500);
          if (
            markerContext.measureText(text).width + 10 >=
            ((cue.end - viewport.start) * width) / viewport.span
          )
            return [];
          return [
            {
              text,
              color: cue.label ? CUE_ROLE_COLORS[cue.label] : UNLABELLED_CUE,
              loop: false,
            },
          ];
        });
        if (
          loopActive !== false &&
          loopStart != null &&
          loopEnd != null &&
          loopStart < viewport.start &&
          loopEnd > viewport.start
        )
          pinned.push({ text: "LOOP", color: SCREEN.loopText, loop: true });
        const key = JSON.stringify(pinned);
        if (key !== pinnedKey) {
          pinnedKey = key;
          pinnedRef.current!.replaceChildren(
            ...pinned.map(({ text, color, loop }) => {
              const label = document.createElement("span");
              label.textContent = text;
              label.style.color = color;
              label.className = loop
                ? "waveform-pinned-loop"
                : "waveform-pinned-cue";
              return label;
            }),
          );
        }
        const bar = gridBarBeat(grid, position),
          upcoming = nextCue(cueMarkers, position, bpm, grid);
        const text = bar
          ? upcoming?.name
            ? `${bar} bars · ${upcoming.name.toUpperCase()} in ${upcoming.bars}`
            : `${bar} bars`
          : "";
        if (labelRef.current!.textContent !== text)
          labelRef.current!.textContent = text;
        labelRef.current!.hidden = !text;
        const wrapped = position < lastDisplayPosition - 0.1;
        lastDisplayPosition = position;
        if (
          !contentDirty &&
          stripAnchor !== null &&
          Math.abs(position - stripAnchor) * pixelsPerSecond < padding * 0.75
        ) {
          if (
            gesture ||
            finishing ||
            !current().playing ||
            current().jogTouch ||
            wrapped
          )
            moveStrip(now);
          dirty = false;
          return;
        }
      }
      stopStrip();
      stripAnchor = strip ? position : null;
      waveCanvas.dataset.anchorSeconds = String(position);
      stripPadding = padding;
      pixelsPerSecond = viewport.span > 0 ? width / viewport.span : 0;
      waveCanvas.dataset.pixelsPerSecond = String(pixelsPerSecond);
      waveCanvas.dataset.paddingPixels = String(padding);
      const pw = Math.round(w * dpr),
        ph = Math.round(h * dpr);
      for (const layer of [waveCanvas, markerCanvas]) {
        if (layer.width !== pw || layer.height !== ph) {
          layer.width = pw;
          layer.height = ph;
        }
        layer.style.width = `${w}px`;
        layer.style.transform = `translateX(${-padding}px)`;
      }
      ctx = waveContext;
      // An overview only changes when its playhead crosses a physical pixel.
      if (overview && duration > 0 && !gesture && !finishing) {
        const pixel = Math.round((position / duration) * w * dpr);
        if (pixel === lastOverviewPixel) {
          dirty = false;
          return;
        }
        lastOverviewPixel = pixel;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = "#0b0b0c";
      ctx.fillRect(0, 0, w, h);
      markerContext.setTransform(dpr, 0, 0, dpr, 0, 0);
      markerContext.clearRect(0, 0, w, h);
      const view = expandWaveformWindow(viewport, width, padding);
      const { span, start } = view;
      const xFor = (t: number) => timeToWaveformFraction(t, view) * w;
      // Dotted baseline, as on a recorder waiting for signal.
      ctx.strokeStyle = SCREEN.axis;
      ctx.lineWidth = 1;
      ctx.setLineDash([1, 3]);
      ctx.beginPath();
      ctx.moveTo(0, h / 2 + 0.5);
      ctx.lineTo(w, h / 2 + 0.5);
      ctx.stroke();
      ctx.setLineDash([]);
      if (duration > 0 && span > 0) {
        if (overview) {
          const detail = getWaveformWindow({
            position,
            duration,
            bpm,
            zoom,
            spanBeats,
          });
          const left = Math.max(0, xFor(detail.start)),
            right = Math.min(w, xFor(detail.end));
          if (right > left) {
            ctx.fillStyle = SCREEN.windowFill;
            ctx.fillRect(left, 0, right - left, h);
            ctx.strokeStyle = SCREEN.windowLine;
            ctx.lineWidth = 1;
            ctx.strokeRect(
              left + 0.5,
              0.5,
              Math.max(0, right - left - 1),
              h - 1,
            );
          }
        }
        if (
          loopStart != null &&
          loopEnd != null &&
          Number.isFinite(loopStart) &&
          Number.isFinite(loopEnd) &&
          loopEnd > loopStart &&
          loopActive !== false
        ) {
          const left = Math.max(0, xFor(loopStart)),
            right = Math.min(w, xFor(loopEnd));
          if (right > left) {
            ctx.fillStyle = SCREEN.loopFill;
            ctx.fillRect(left, 0, right - left, h);
            ctx.strokeStyle = SCREEN.loop;
            ctx.lineWidth = 1;
            ctx.strokeRect(
              left + 0.5,
              0.5,
              Math.max(0, right - left - 1),
              h - 1,
            );
            if (!overview) {
              ctx.fillStyle = SCREEN.loopText;
              ctx.font = SCREEN.font(9);
              ctx.fillText("LOOP", left + 4, h - 5);
            }
          }
        }
        if (samples.length) {
          raster.configure({
            samples,
            bands,
            duration,
            height: h,
            dpr,
            pixelsPerSecond: w / span,
            color,
            overview,
          });
          raster.draw(ctx, start, w);
          if (overview) {
            ctx.fillStyle = SCREEN.played;
            ctx.fillRect(
              0,
              0,
              Math.max(0, Math.min(w, view.playheadFraction * w)),
              h,
            );
          }
          if (!overview && Math.abs(position - lastPrewarmPosition) > 1) {
            lastPrewarmPosition = position;
            const direction = position >= lastPaintPosition ? 1 : -1;
            const scheduledConfig = sourceKey();
            schedulePrewarm(() => {
              if (!disposed && scheduledConfig === sourceKey())
                raster.prewarm(start, w, direction);
            });
          }
        }
        if (!overview) {
          ctx = markerContext;
          ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
          ctx.clearRect(0, 0, w, h);
        }
        // Saved cue markers: a strip along the top edge, with names on the detail view.
        const strip = overview ? 3 : 4;
        for (const section of cueMarkers) {
          const left = Math.max(0, xFor(section.start)),
            right = Math.min(
              w,
              xFor(section.end === section.start ? section.start : section.end),
            );
          if (section.end === section.start && left > 0 && left < w) {
            ctx.fillStyle = section.label
              ? CUE_ROLE_COLORS[section.label]
              : UNLABELLED_CUE;
            ctx.fillRect(left, 0, 1, h);
            if (!overview && section.name) {
              ctx.font = SCREEN.font(9, 500);
              ctx.fillText(section.name.toUpperCase(), left + 4, 16);
            }
            continue;
          }
          if (right <= left) continue;
          const colour = section.label
            ? CUE_ROLE_COLORS[section.label]
            : UNLABELLED_CUE;
          ctx.fillStyle = colour;
          ctx.fillRect(left, 0, Math.max(1, right - left - 1), strip);
          if (!overview && (section.name || section.label)) {
            const name = (
              section.name || CUE_ROLE_NAMES[section.label!]
            ).toUpperCase();
            ctx.font = SCREEN.font(9, 500);
            const textWidth = ctx.measureText(name).width;
            // Clear of a hot cue flag at the section start; pinned to the left
            // edge while the section's start has scrolled past.
            const labelX = left + (left > 0 ? 18 : 4);
            if (labelX + textWidth + 6 < right) {
              ctx.fillStyle = SCREEN.labelFill;
              ctx.fillRect(labelX - 3, 6, textWidth + 6, 13);
              ctx.fillStyle = colour;
              ctx.fillText(name, labelX, 16);
            }
          }
        }
        // Beat markers sit over the envelopes, so loud passages cannot hide the
        // timing guide. Before/after the real file there is no fabricated grid.
        if (!overview && grid.valid) {
          for (const marker of visibleGridBeats(
            grid,
            start,
            start + span,
            duration,
          )) {
            const major = marker.downbeat,
              x = xFor(marker.time);
            ctx.lineWidth = 1;
            ctx.strokeStyle = major ? SCREEN.barLine : SCREEN.beatLine;
            ctx.beginPath();
            ctx.moveTo(x + 0.5, 0);
            ctx.lineTo(x + 0.5, h);
            ctx.stroke();
            if (major) {
              ctx.fillStyle = SCREEN.bar;
              ctx.beginPath();
              ctx.moveTo(x - 4, 0);
              ctx.lineTo(x + 4, 0);
              ctx.lineTo(x, 5);
              ctx.closePath();
              ctx.fill();
              ctx.beginPath();
              ctx.moveTo(x - 4, h);
              ctx.lineTo(x + 4, h);
              ctx.lineTo(x, h - 5);
              ctx.closePath();
              ctx.fill();
            } else {
              ctx.strokeStyle = SCREEN.beatTick;
              ctx.beginPath();
              ctx.moveTo(x + 0.5, 0);
              ctx.lineTo(x + 0.5, 8);
              ctx.moveTo(x + 0.5, h - 8);
              ctx.lineTo(x + 0.5, h);
              ctx.stroke();
            }
          }
        }
        if (
          loopStart != null &&
          loopEnd != null &&
          loopEnd > loopStart &&
          loopActive !== false
        ) {
          ctx.strokeStyle = SCREEN.loop;
          ctx.lineWidth = 1;
          for (const boundary of [loopStart, loopEnd]) {
            const x = xFor(boundary);
            if (x < 0 || x > w) continue;
            ctx.beginPath();
            ctx.moveTo(x + 0.5, 0);
            ctx.lineTo(x + 0.5, h);
            ctx.stroke();
          }
        }
        hotcues.forEach((t, i) => {
          if (t === null || !Number.isFinite(t) || t < 0 || t > duration)
            return;
          const x = xFor(t);
          if (x < 0 || x > w) return;
          ctx.fillStyle = HOT_CUE_COLORS[i % HOT_CUE_COLORS.length];
          ctx.fillRect(Math.min(w - 1, x), 0, 1, h);
          const flagX = Math.min(x, w - (overview ? 9 : 15));
          ctx.fillRect(flagX, 0, overview ? 9 : 15, overview ? 9 : 16);
          ctx.fillStyle = SCREEN.background;
          ctx.font = SCREEN.font(overview ? 7 : 10, 500);
          ctx.fillText(
            String.fromCharCode(65 + i),
            flagX + (overview ? 2 : 4),
            overview ? 7 : 12,
          );
        });
        const bottomMarker = (
          seconds: number,
          label: string,
          markerColor: string,
          height: number,
        ) => {
          if (!Number.isFinite(seconds) || seconds < 0 || seconds > duration)
            return;
          const markerX = xFor(seconds);
          if (markerX < 0 || markerX > w) return;
          ctx.strokeStyle = markerColor;
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(markerX, h - height);
          ctx.lineTo(markerX, h);
          ctx.stroke();
          if (!overview) {
            ctx.fillStyle = markerColor;
            ctx.font = SCREEN.font(9);
            ctx.fillText(label, markerX + 3, h - height + 8);
          }
        };
        memoryCues.forEach((cue, i) => {
          if (cue !== null)
            bottomMarker(cue, `M${i + 1}`, SCREEN.memory, overview ? 5 : 18);
        });
        const mainCue = cuePoint !== undefined ? cuePoint : cueSeconds;
        if (mainCue != null)
          bottomMarker(mainCue, "C", color, overview ? 12 : 30);
        if (overview || !strip) {
          const x = overview
            ? Math.max(0.75, Math.min(w - 0.75, view.playheadFraction * w))
            : w / 2;
          ctx.fillStyle = SCREEN.playhead;
          ctx.fillRect(x - 0.75, 0, 1.5, h);
        }
      }
      if (!duration && !overview) {
        ctx.fillStyle = SCREEN.idle;
        ctx.fillRect(w / 2 - 0.75, 0, 1.5, h);
        ctx.beginPath();
        ctx.moveTo(w / 2 - 5, 0);
        ctx.lineTo(w / 2 + 5, 0);
        ctx.lineTo(w / 2, 7);
        ctx.fill();
      }
      if (!samples.length && !overview) {
        ctx.fillStyle = SCREEN.idleText;
        ctx.textAlign = "center";
        ctx.font = SCREEN.font(11);
        ctx.fillText(
          duration ? "waveform analysis pending" : "load a track to begin",
          w / 2,
          h / 2,
        );
        ctx.textAlign = "left";
      }

      metrics.paint(
        now,
        performance.now() - started,
        !!current().playing && !document.hidden,
      );
      lastPaintPosition = position;
      contentDirty = false;
      dirty = false;
      moveStrip(now);
    };
    function tick(now: number) {
      metrics.frame(now, !!current().playing && !document.hidden);
      frame = 0;
      lastFrame = now;
      if (gesture) flush(gesture);
      if (dirty || gesture || finishing || motion.animating(now)) paint(now);
      if (gesture?.failed) finish(true);
      if (gesture || finishing || motion.animating(now)) ensureFrame();
    }
    const measure = () => {
      const rect = canvas.getBoundingClientRect();
      width = rect.width;
      height = rect.height;
      dpr = window.devicePixelRatio || 1;
      contentDirty = true;
      dirty = true;
      lastOverviewPixel = NaN;
      paint(performance.now());
      ensureFrame();
    };
    const invalidate = (force = false) => {
      if (force) {
        lastOverviewPixel = NaN;
        contentDirty = true;
      }
      const p = current(),
        now = performance.now(),
        key = `${p.position}:${p.duration}:${p.playing}:${p.playbackRate}:${p.jogTouch}:${p.sourceId}:${p.loadRevision}:${p.positionRevision}`;
      if (nativeKey === key && !force) return;
      if (nativeKey !== key) {
        const previousSource = nativeKey;
        nativeKey = key;
        motion.update(
          {
            position: p.position,
            duration: p.duration,
            playing: !!p.playing,
            playbackRate: p.playbackRate ?? 1,
            jogTouch: p.jogTouch,
            sourceId: `${p.sourceId ?? ""}@${p.loadRevision}`,
            time: p.snapshotTime ?? now,
            positionRevision: p.positionRevision,
            loopStart: p.loopStart,
            loopEnd: p.loopEnd,
            loopActive: p.loopActive,
          },
          now,
        );
        if (
          previousSource &&
          gesture &&
          (p.sourceId !== grabSource || p.loadRevision !== grabRevision)
        ) {
          finish(true);
        }
      }
      dirty = true;
      if (!force) moveStrip(now);
      ensureFrame();
    };
    let grabSource: string | undefined, grabRevision: number | undefined;
    runtime.current = {
      invalidate: () => invalidate(true),
      down: (event) => {
        suppressClick = false;
        const p = current();
        if (
          finishing ||
          !p.onScrub ||
          p.overview ||
          event.button !== 0 ||
          p.duration <= 0 ||
          width <= 0
        )
          return;
        const now = performance.now(),
          grabbed = motion.beginGrab(now),
          view = getWaveformWindow({
            position: grabbed,
            duration: p.duration,
            bpm: p.bpm,
            zoom: p.zoom,
            spanBeats: p.spanBeats,
          });
        grabSource = p.sourceId;
        grabRevision = p.loadRevision;
        const g: Gesture = {
          pointerId: event.pointerId,
          lastX: event.clientX,
          originX: event.clientX,
          span: view.span,
          width,
          moved: false,
          pending: 0,
          lastFlushTime: now,
          ready: false,
          failed: false,
          start: Promise.resolve(),
          inFlight: null,
          scrub: p.onScrub,
          end: p.onScrubEnd,
        };
        gesture = g;
        canvas.setPointerCapture(event.pointerId);
        try {
          g.start = Promise.resolve(p.onScrubStart?.(grabbed)).then(
            () => {
              g.ready = true;
              ensureFrame();
            },
            () => {
              g.failed = true;
              ensureFrame();
            },
          );
        } catch {
          g.failed = true;
          g.start = Promise.resolve();
        }
        dirty = true;
        paint(now);
        ensureFrame();
      },
      move: (event) => {
        const g = gesture;
        if (!g || g.pointerId !== event.pointerId) return;
        const dx = event.clientX - g.lastX;
        g.lastX = event.clientX;
        if (Math.abs(event.clientX - g.originX) > 2) g.moved = true;
        g.pending += motion.moveGrab((-dx / g.width) * g.span);
        dirty = true;
        ensureFrame();
      },
      up: (event) => {
        if (!gesture || gesture.pointerId !== event.pointerId) return;
        finish();
        if (canvas.hasPointerCapture(event.pointerId))
          canvas.releasePointerCapture(event.pointerId);
      },
      cancel: () => finish(true),
      click: (event) => {
        if (suppressClick || finishing) {
          suppressClick = false;
          return;
        }
        const p = current();
        if (p.duration <= 0) return;
        const rect = canvas.getBoundingClientRect();
        if (rect.width <= 0) return;
        const now = performance.now(),
          view = getWaveformWindow({
            position: motion.position(now),
            duration: p.duration,
            bpm: p.bpm,
            zoom: p.zoom,
            overview: p.overview,
            spanBeats: p.spanBeats,
          }),
          target = waveformSeekSeconds(
            view,
            (event.clientX - rect.left) / rect.width,
            p.duration,
          );
        motion.expectSeek(target, now);
        p.onSeek(target);
        dirty = true;
        paint(now);
        ensureFrame();
      },
    };
    const blur = () => finish(true);
    window.addEventListener("blur", blur);
    const observer = new ResizeObserver(measure);
    observer.observe(canvas);
    const wheel = (event: WheelEvent) => {
      if (props.current.onZoom && Number.isFinite(event.deltaY)) {
        event.preventDefault();
        props.current.onZoom(event.deltaY);
      }
    };
    canvas.addEventListener("wheel", wheel, { passive: false });
    measure();
    invalidate();
    const unsubscribe = engineStore.subscribeTransport(() => invalidate());
    return () => {
      unsubscribe();
      stopStrip();
      raster.dispose();
      metrics.dispose();
      disposed = true;
      finish(true);
      if (frame) cancelAnimationFrame(frame);
      if (prewarmTimer !== null) clearTimeout(prewarmTimer);
      observer.disconnect();
      window.removeEventListener("blur", blur);
      canvas.removeEventListener("wheel", wheel);
      runtime.current = null;
    };
  }, []);
  useLayoutEffect(() => {
    runtime.current?.invalidate();
  }, [
    input.deckId,
    input.samples,
    input.bands,
    input.duration,
    input.bpm,
    input.color,
    input.zoom,
    input.spanBeats,
    input.hotcues?.join(","),
    input.memoryCues?.join(","),
    input.cueMarkers,
    input.cuePoint,
    input.cueSeconds,
    input.loopStart,
    input.loopEnd,
    input.loopActive,
    input.overview,
    input.beatGrid,
    input.beatgridOffset,
    input.beatgridValid,
    input.beatgridDownbeat,
    input.sourceId,
    input.loadRevision,
    input.deckId === undefined ? input.position : undefined,
    input.deckId === undefined ? input.playing : undefined,
    input.deckId === undefined ? input.playbackRate : undefined,
    input.deckId === undefined ? input.jogTouch : undefined,
    input.deckId === undefined ? input.positionRevision : undefined,
  ]);
  const spectral = hasWaveformBands(input.bands, input.samples.length);
  return (
    <div
      ref={hostRef}
      role="img"
      className={input.overview ? "waveform overview" : "waveform"}
      aria-label={`${spectral ? "Three-band low, mid and high frequency" : "Peak amplitude"} waveform; ${input.overview ? "click to seek" : "hold to stop and drag to scratch"}`}
      style={{
        touchAction: input.onScrub ? "none" : undefined,
        cursor: input.onScrub && !input.overview ? "grab" : undefined,
      }}
      onPointerDown={(e) => runtime.current?.down(e.nativeEvent)}
      onPointerMove={(e) => runtime.current?.move(e.nativeEvent)}
      onPointerUp={(e) => runtime.current?.up(e.nativeEvent)}
      onPointerCancel={() => runtime.current?.cancel()}
      onLostPointerCapture={() => runtime.current?.cancel()}
      onClick={(e) => runtime.current?.click(e.nativeEvent)}
    >
      <canvas ref={canvasRef} aria-hidden="true" />
      <div ref={playedRef} className="waveform-played" hidden />
      <canvas ref={markersRef} aria-hidden="true" />
      <div ref={pinnedRef} className="waveform-pinned" hidden />
      <div ref={playheadRef} className="waveform-playhead" hidden>
        <span ref={labelRef} />
      </div>
    </div>
  );
}

import { useLayoutEffect, useRef } from "react";
import type { WaveformBands } from "./types";
import {
  hasWaveformBands,
  getWaveformWindow,
  timeToWaveformFraction,
  waveformSeekSeconds,
  nextSection,
  HOT_CUE_COLORS,
  SECTION_COLORS,
  SECTION_NAMES,
  UNLABELLED_SECTION,
  type DisplaySection,
} from "./waveform-data";
import { WaveformMotion } from "./waveform-motion";
import type { WaveformGuide } from "./transition";
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
  /** Song sections, drawn as a coloured strip along the top edge. */
  sections?: DisplaySection[];
  /** Transition guide marks: when to start the other deck, the stretch
   * both play together and where the new track's drop lands. */
  guides?: WaveformGuide[];
  /** "START B in 8", drawn under the bar counter beside the playhead. */
  guideCountdown?: { text: string; color: string } | null;
  beatGrid?: BeatGrid;
  beatgridOffset?: number;
  beatgridValid?: boolean;
  beatgridDownbeat?: boolean;
  playing?: boolean;
  playbackRate?: number;
  jogTouch?: boolean;
  sourceId?: string;
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
const noSections: DisplaySection[] = [];
const noGuides: WaveformGuide[] = [];

export default function Waveform(input: WaveformProps) {
  const props = useRef(input);
  props.current = input;
  const canvasRef = useRef<HTMLCanvasElement>(null),
    runtime = useRef<Runtime | null>(null);
  useLayoutEffect(() => {
    const canvas = canvasRef.current!,
      ctx = canvas.getContext("2d");
    if (!ctx) return;
    const motion = new WaveformMotion(),
      raster = new WaveformRaster();
    let disposed = false,
      frame = 0,
      width = 0,
      height = 0,
      dpr = window.devicePixelRatio || 1,
      lastFrame = -Infinity,
      lastPaintPosition = 0,
      lastPrewarmPosition = -Infinity,
      dirty = true,
      gesture: Gesture | null = null,
      finishing = 0,
      suppressClick = false;
    let nativeKey = "",
      prewarmTimer: ReturnType<typeof setTimeout> | null = null;
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
    const paint = (now: number) => {
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
        sections = noSections,
        guides = noGuides,
        guideCountdown,
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
      } = props.current;
      const position = motion.position(now),
        w = width,
        h = height;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const view = getWaveformWindow({
          position,
          duration,
          bpm,
          zoom,
          overview,
          spanBeats,
        }),
        { span, start } = view;
      const grid =
        beatGrid ??
        uniformBeatGrid(bpm, beatgridOffset, beatgridValid, beatgridDownbeat);
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
          // Played audio steps back so what is coming next reads first.
          const played = overview
            ? Math.max(0, Math.min(w, view.playheadFraction * w))
            : w / 2;
          ctx.fillStyle = SCREEN.played;
          ctx.fillRect(0, 0, played, h);
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
        // Song sections: a strip along the top edge, with names on the detail view.
        const strip = overview ? 3 : 4;
        for (const section of sections) {
          const left = Math.max(0, xFor(section.start)),
            right = Math.min(w, xFor(section.end));
          if (right <= left) continue;
          const colour = section.label
            ? SECTION_COLORS[section.label]
            : UNLABELLED_SECTION;
          ctx.fillStyle = colour;
          ctx.fillRect(left, 0, Math.max(1, right - left - 1), strip);
          if (!overview && section.label) {
            const name = SECTION_NAMES[section.label].toUpperCase();
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
        // Transition guide: a tinted "together" span, a dashed start line and
        // a dotted landing line in the incoming deck's colour, labelled above
        // the cue markers on the detail view.
        for (const guide of guides) {
          const x = xFor(guide.at);
          if (guide.until !== undefined && guide.until > guide.at) {
            const left = Math.max(0, x),
              right = Math.min(w, xFor(guide.until));
            if (right > left) {
              ctx.globalAlpha = 0.1;
              ctx.fillStyle = guide.color;
              ctx.fillRect(left, 0, right - left, h);
              ctx.globalAlpha = 1;
            }
          }
          if (x < 0 || x > w) continue;
          ctx.strokeStyle = guide.color;
          ctx.lineWidth = guide.kind === "landing" ? 1 : 1.5;
          ctx.setLineDash(guide.kind === "landing" ? [2, 3] : [5, 3]);
          ctx.beginPath();
          ctx.moveTo(x + 0.5, 0);
          ctx.lineTo(x + 0.5, h);
          ctx.stroke();
          ctx.setLineDash([]);
          if (!overview) {
            ctx.font = SCREEN.font(9, 500);
            const textWidth = ctx.measureText(guide.label).width,
              labelX = Math.max(3, Math.min(x + 4, w - textWidth - 6)),
              labelY = Math.max(22, h - 46);
            ctx.fillStyle = SCREEN.labelFill;
            ctx.fillRect(labelX - 3, labelY, textWidth + 6, 13);
            ctx.fillStyle = guide.color;
            ctx.fillText(guide.label, labelX, labelY + 10);
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
        const x = overview
          ? Math.max(0.75, Math.min(w - 0.75, view.playheadFraction * w))
          : w / 2;
        ctx.fillStyle = SCREEN.playhead;
        ctx.fillRect(x - 0.75, 0, 1.5, h);
        if (!overview) {
          ctx.beginPath();
          ctx.arc(x, 4, 3.5, 0, Math.PI * 2);
          ctx.fill();
          const label = gridBarBeat(grid, position);
          if (label) {
            ctx.font = SCREEN.font(10, 500);
            const upcoming = nextSection(sections, position, bpm, grid);
            const text = upcoming?.label
                ? `${label} bars · ${SECTION_NAMES[upcoming.label].toUpperCase()} in ${upcoming.bars}`
                : `${label} bars`,
              size = ctx.measureText(text).width;
            ctx.fillStyle = SCREEN.labelFill;
            ctx.fillRect(x + 5, 17, size + 8, 16);
            ctx.fillStyle = SCREEN.barLabel;
            ctx.fillText(text, x + 9, 29);
          }
          if (guideCountdown) {
            ctx.font = SCREEN.font(10, 500);
            const size = ctx.measureText(guideCountdown.text).width;
            ctx.fillStyle = SCREEN.labelFill;
            ctx.fillRect(x + 5, 35, size + 8, 16);
            ctx.fillStyle = guideCountdown.color;
            ctx.fillText(guideCountdown.text, x + 9, 47);
          }
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

      lastPaintPosition = position;
      dirty = false;
    };
    function tick(now: number) {
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
      const pw = Math.round(width * dpr),
        ph = Math.round(height * dpr);
      if (canvas.width !== pw || canvas.height !== ph) {
        canvas.width = pw;
        canvas.height = ph;
      }
      dirty = true;
      paint(performance.now());
      ensureFrame();
    };
    const invalidate = () => {
      const p = props.current,
        now = performance.now(),
        key = `${p.position}:${p.duration}:${p.playing}:${p.playbackRate}:${p.jogTouch}:${p.sourceId}:${p.snapshotTime}:${p.positionRevision}`;
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
            sourceId: p.sourceId,
            time: p.snapshotTime ?? now,
            positionRevision: p.positionRevision,
            loopStart: p.loopStart,
            loopEnd: p.loopEnd,
            loopActive: p.loopActive,
          },
          now,
        );
        if (previousSource && gesture && p.sourceId !== grabSource) {
          finish(true);
        }
      }
      dirty = true;
      if (now - lastFrame > 50) paint(now);
      ensureFrame();
    };
    let grabSource: string | undefined;
    runtime.current = {
      invalidate,
      down: (event) => {
        suppressClick = false;
        const p = props.current;
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
        const p = props.current;
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
    return () => {
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
  }, [input]);
  const spectral = hasWaveformBands(input.bands, input.samples.length);
  return (
    <canvas
      ref={canvasRef}
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
    />
  );
}

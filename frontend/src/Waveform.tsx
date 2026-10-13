import { useLayoutEffect, useRef } from "react";
import type { WaveformBands } from "./types";
import {
  hasWaveformBands,
  getWaveformWindow,
  timeToWaveformFraction,
  waveformSeekSeconds,
  waveformBarBeat,
  HOT_CUE_COLORS,
} from "./waveform-data";
import { WaveformMotion } from "./waveform-motion";
import { WaveformRaster } from "./waveform-renderer";

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
  beatgridOffset?: number;
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
        beatgridOffset = 0,
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
      const xFor = (t: number) => timeToWaveformFraction(t, view) * w;
      ctx.strokeStyle = "#222a35";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, h / 2 + 0.5);
      ctx.lineTo(w, h / 2 + 0.5);
      ctx.stroke();
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
            ctx.fillStyle = "rgba(193,213,239,.09)";
            ctx.fillRect(left, 0, right - left, h);
            ctx.strokeStyle = "rgba(193,213,239,.42)";
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
            ctx.fillStyle = "rgba(80,190,199,.12)";
            ctx.fillRect(left, 0, right - left, h);
            ctx.strokeStyle = "#64c1cc";
            ctx.lineWidth = 1;
            ctx.strokeRect(
              left + 0.5,
              0.5,
              Math.max(0, right - left - 1),
              h - 1,
            );
            if (!overview) {
              ctx.fillStyle = "#b0e3e9";
              ctx.font = "8px sans-serif";
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
        // Beat markers sit over the envelopes, so loud passages cannot hide the
        // timing guide. Before/after the real file there is no fabricated grid.
        if (
          !overview &&
          Number.isFinite(bpm) &&
          bpm > 0 &&
          Number.isFinite(beatgridOffset)
        ) {
          const beat = 60 / bpm;
          for (
            let t =
              Math.ceil((Math.max(0, start) - beatgridOffset) / beat) * beat +
              beatgridOffset;
            t <= Math.min(duration, start + span);
            t += beat
          ) {
            const major = Math.round((t - beatgridOffset) / beat) % 4 === 0,
              x = xFor(t);
            ctx.lineWidth = 1;
            ctx.strokeStyle = major
              ? "rgba(232,112,111,.7)"
              : "rgba(172,190,213,.35)";
            ctx.beginPath();
            ctx.moveTo(x + 0.5, 0);
            ctx.lineTo(x + 0.5, h);
            ctx.stroke();
            if (major) {
              ctx.fillStyle = "#e8706f";
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
              ctx.strokeStyle = "#a7b7cd";
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
          ctx.strokeStyle = "#64c1cc";
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
          ctx.fillStyle = "#131923";
          ctx.font = overview ? "bold 7px sans-serif" : "bold 10px sans-serif";
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
            ctx.font = "8px sans-serif";
            ctx.fillText(label, markerX + 3, h - height + 8);
          }
        };
        memoryCues.forEach((cue, i) => {
          if (cue !== null)
            bottomMarker(cue, `M${i + 1}`, "#b2bfd3", overview ? 5 : 18);
        });
        const mainCue = cuePoint !== undefined ? cuePoint : cueSeconds;
        if (mainCue != null)
          bottomMarker(mainCue, "C", color, overview ? 12 : 30);
        const x = overview
          ? Math.max(0.75, Math.min(w - 0.75, view.playheadFraction * w))
          : w / 2;
        ctx.fillStyle = "#eff3f8";
        ctx.fillRect(x - 0.75, 0, 1.5, h);
        if (!overview) {
          ctx.beginPath();
          ctx.moveTo(x - 5, 0);
          ctx.lineTo(x + 5, 0);
          ctx.lineTo(x, 7);
          ctx.fill();
          const label = waveformBarBeat(position, bpm, beatgridOffset);
          if (label) {
            ctx.font = "600 11px sans-serif";
            const text = `${label} BARS`,
              size = ctx.measureText(text).width;
            ctx.fillStyle = "rgba(17,24,34,.9)";
            ctx.fillRect(x + 5, 17, size + 8, 16);
            ctx.fillStyle = "#75b8fa";
            ctx.fillText(text, x + 9, 29);
          }
        }
      }
      if (!duration && !overview) {
        ctx.fillStyle = "#60728c";
        ctx.fillRect(w / 2 - 0.75, 0, 1.5, h);
        ctx.beginPath();
        ctx.moveTo(w / 2 - 5, 0);
        ctx.lineTo(w / 2 + 5, 0);
        ctx.lineTo(w / 2, 7);
        ctx.fill();
      }
      if (!samples.length && !overview) {
        ctx.fillStyle = "#65768e";
        ctx.textAlign = "center";
        ctx.font = "11px system-ui";
        ctx.fillText(
          duration ? "Waveform analysis pending" : "LOAD A TRACK TO BEGIN",
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

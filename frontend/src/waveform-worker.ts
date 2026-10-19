import {
  WaveformRaster,
  type CanvasFactory,
  type RasterSource,
} from "./waveform-renderer";
// Uses exactly the same envelope/tile primitive as the renderer. Only expensive
// source indexing and cold pixel construction live in this worker.
const raster = new WaveformRaster(
  ((w: number, h: number) =>
    new OffscreenCanvas(w, h)) as unknown as CanvasFactory,
);
let generation = 0,
  config: RasterSource | null = null;
self.onmessage = ({ data }) => {
  if (data.type === "configure") {
    generation = data.generation;
    config = data.config;
    raster.configure(config!);
    return;
  }
  if (data.type !== "tile" || data.generation !== generation || !config) return;
  const canvas = new OffscreenCanvas(
    Math.ceil(raster.tileWidth * config.dpr),
    Math.max(1, Math.ceil(config.height * config.dpr)),
  );
  const ctx = canvas.getContext("2d")!;
  ctx.scale(config.dpr, config.dpr);
  raster.draw(
    ctx as unknown as CanvasRenderingContext2D,
    (data.number * raster.tileWidth) / config.pixelsPerSecond,
    raster.tileWidth - 0.001,
  );
  const bitmap = canvas.transferToImageBitmap();
  self.postMessage(
    { generation, number: data.number, bitmap },
    { transfer: [bitmap] },
  );
};

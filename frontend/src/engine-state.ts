import {
  emptyEngine,
  type DeckSnapshot,
  type EngineSnapshot,
  type TrackRecord,
} from "./types.ts";
export interface EngineTransport extends EngineSnapshot {
  metadata_revision?: number;
}
export interface EngineMetadata {
  revision: number;
  load_revisions?: number[];
  tracks: (TrackRecord | null)[];
  samples?: (TrackRecord | null)[];
}
/** Attach immutable source data without reconstructing waveform arrays every frame. */
export function withTrackMetadata(
  transport: EngineSnapshot,
  tracks: (TrackRecord | null)[],
  metadataLoads?: number[],
): EngineSnapshot {
  return {
    ...transport,
    decks: transport.decks.map((deck, i) => ({
      ...deck,
      track:
        !metadataLoads || metadataLoads[i] === transport.load_revisions?.[i]
          ? (tracks[i] ?? null)
          : null,
    })),
  };
}

const rootDeckKeys = [
  "track",
  "playing",
  "preparation_pending",
  "launch_pending",
  "beatgrid_offset_seconds",
  "beatgrid_valid",
  "beatgrid_downbeat",
  "beatgrid_manual",
  "beatgrid_uniform",
  "beatgrid_anchor_index",
  "bpm",
  "position_revision",
  "loop_start_seconds",
  "loop_end_seconds",
  "loop_active",
  "grid_revision",
] as const;
const visualOnly = new Set([
  "position_seconds",
  "position",
  "peak",
  "peak_left",
  "peak_right",
  "beat_position",
  "actual_playback_rate",
  "playback_bpm",
  "local_bpm",
  "actual_playback_bpm",
]);
function equalValue(a: unknown, b: unknown): boolean {
  return (
    Object.is(a, b) ||
    (Array.isArray(a) &&
      Array.isArray(b) &&
      a.length === b.length &&
      a.every((v, i) => Object.is(v, b[i])))
  );
}
const mixerOnly = new Set([
  "gain",
  "channel_fader",
  "eq_low",
  "eq_mid",
  "eq_high",
  "filter",
  "cue_enabled",
  "echo",
  "reverb",
  "fx_beats",
]);
function sameDeck(
  a: DeckSnapshot | undefined,
  b: DeckSnapshot,
  structural: boolean,
  deckView = false,
): boolean {
  if (!a) return false;
  const keys = structural
    ? rootDeckKeys
    : Object.keys(b).filter(
        (k) => !visualOnly.has(k) && (!deckView || !mixerOnly.has(k)),
      );
  return (
    keys.every((k) =>
      equalValue(a[k as keyof DeckSnapshot], b[k as keyof DeckSnapshot]),
    ) &&
    (structural ||
      Object.keys(a).filter(
        (k) => !visualOnly.has(k) && (!deckView || !mixerOnly.has(k)),
      ).length === keys.length)
  );
}
type Listener = () => void;
/** The native clock is delivered once. React subscribes to semantic controls,
 * while canvases and tiny clock/meter components consume the transport stream. */
export class EngineStore {
  readonly diagnostics = {
    app_renders: 0,
    transport_ticks: 0,
    structural_updates: 0,
    control_updates: 0,
  };
  private latest: EngineSnapshot = emptyEngine;
  private structural: EngineSnapshot = emptyEngine;
  private controls: EngineSnapshot = emptyEngine;
  private deckViews: DeckSnapshot[] = emptyEngine.decks;
  private time = 0;
  private transportListeners = new Set<Listener>();
  private structuralListeners = new Set<Listener>();
  private controlListeners = new Set<Listener>();
  readonly getTransport = () => this.latest;
  readonly getStructural = () => this.structural;
  readonly getControls = () => this.controls;
  readonly getDeckView = (id: number) => this.deckViews[id];
  readonly receivedAt = () => this.time;
  private subscribe(listeners: Set<Listener>, listener: Listener) {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }
  readonly subscribeTransport = (listener: Listener) =>
    this.subscribe(this.transportListeners, listener);
  readonly subscribeStructural = (listener: Listener) =>
    this.subscribe(this.structuralListeners, listener);
  readonly subscribeControls = (listener: Listener) =>
    this.subscribe(this.controlListeners, listener);
  publish(engine: EngineSnapshot, receivedAt: number) {
    this.diagnostics.transport_ticks++;
    this.latest = engine;
    this.time = receivedAt;
    this.deckViews = engine.decks.map((deck, i) =>
      sameDeck(this.deckViews[i], deck, false, true) ? this.deckViews[i] : deck,
    );
    const rootDecks = engine.decks.map((deck, i) =>
      sameDeck(this.structural.decks[i], deck, true)
        ? this.structural.decks[i]
        : deck,
    );
    if (
      rootDecks.some((d, i) => d !== this.structural.decks[i]) ||
      [
        "running",
        "audio_running",
        "sample_rate",
        "output_channels",
        "engine_epoch",
        "sampler_gain",
      ].some((k) => !equalValue(engine[k], this.structural[k])) ||
      !equalValue(engine.load_revisions, this.structural.load_revisions) ||
      !equalValue(
        (
          engine.sampler_positions_seconds as (number | null)[] | undefined
        )?.map((p) => p !== null),
        (
          this.structural.sampler_positions_seconds as
            (number | null)[] | undefined
        )?.map((p) => p !== null),
      )
    ) {
      this.diagnostics.structural_updates++;
      this.structural = { ...engine, decks: rootDecks };
      this.structuralListeners.forEach((listener) => listener());
    }
    const controlDecks = engine.decks.map((deck, i) =>
      sameDeck(this.controls.decks[i], deck, false)
        ? this.controls.decks[i]
        : deck,
    );
    const topKeys = Object.keys(engine).filter(
      (k) =>
        ![
          "decks",
          "master_peak",
          "cue_peak",
          "rendered_frames",
          "active_ramps",
          "buffer_frames",
          "audio_quality",
          "scheduled_batch",
          "sampler_positions_seconds",
        ].includes(k),
    );
    if (
      controlDecks.some((d, i) => d !== this.controls.decks[i]) ||
      topKeys.some(
        (k) =>
          !equalValue(
            engine[k as keyof EngineSnapshot],
            this.controls[k as keyof EngineSnapshot],
          ),
      )
    ) {
      this.diagnostics.control_updates++;
      this.controls = { ...engine, decks: controlDecks };
      this.controlListeners.forEach((listener) => listener());
    }
    this.transportListeners.forEach((listener) => listener());
  }
}
export const engineStore = new EngineStore();

function sameArray(
  a: number[] | undefined | null,
  b: number[] | undefined | null,
) {
  return (
    a === b ||
    (!!a && !!b && a.length === b.length && a.every((n, i) => n === b[i]))
  );
}
/** A cue-only metadata delivery must not destroy immutable waveform tiles. */
export function stabilizeTrack(
  previous: TrackRecord | null,
  incoming: TrackRecord | null,
): TrackRecord | null {
  if (!previous || !incoming || previous.path !== incoming.path)
    return incoming;
  const track = { ...incoming };
  if (sameArray(previous.waveform, incoming.waveform))
    track.waveform = previous.waveform;
  if (
    previous.waveform_bands &&
    incoming.waveform_bands &&
    ["low", "mid", "high"].every((key) =>
      sameArray(
        previous.waveform_bands![key as "low"],
        incoming.waveform_bands![key as "low"],
      ),
    )
  )
    track.waveform_bands = previous.waveform_bands;
  if (
    previous.neural_rhythm &&
    incoming.neural_rhythm &&
    sameArray(
      previous.neural_rhythm.beats_seconds,
      incoming.neural_rhythm.beats_seconds,
    ) &&
    sameArray(
      previous.neural_rhythm.downbeats_seconds,
      incoming.neural_rhythm.downbeats_seconds,
    )
  ) {
    track.neural_rhythm = {
      ...incoming.neural_rhythm,
      beats_seconds: previous.neural_rhythm.beats_seconds,
      downbeats_seconds: previous.neural_rhythm.downbeats_seconds,
    };
  }
  return track;
}

/** Small background status DTOs must not reconcile the app when unchanged. */
export function retainUnchanged<T>(previous: T, incoming: T): T {
  return JSON.stringify(previous) === JSON.stringify(incoming)
    ? previous
    : incoming;
}

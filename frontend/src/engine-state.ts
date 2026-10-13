import type { EngineSnapshot, TrackRecord } from "./types";
export interface EngineTransport {
  engine: EngineSnapshot;
  metadata_revision: number;
}
export interface EngineMetadata {
  revision: number;
  tracks: (TrackRecord | null)[];
}
/** Attach immutable source data without reconstructing waveform arrays every frame. */
export function withTrackMetadata(
  transport: EngineSnapshot,
  tracks: (TrackRecord | null)[],
): EngineSnapshot {
  return {
    ...transport,
    decks: transport.decks.map((deck, i) => ({
      ...deck,
      track: tracks[i] ?? null,
    })),
  };
}

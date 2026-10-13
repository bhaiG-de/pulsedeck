export interface WaveformSnapshot {
  position: number;
  duration: number;
  playing: boolean;
  playbackRate: number;
  jogTouch?: boolean;
  sourceId?: string;
  time: number;
  positionRevision?: number;
  loopStart?: number | null;
  loopEnd?: number | null;
  loopActive?: boolean;
}
const clamp = (value: number, duration: number) =>
  Math.max(
    0,
    Math.min(Math.max(0, duration), Number.isFinite(value) ? value : 0),
  );
const STALE_MS = 120,
  CORRECTION_MS = 80,
  RELEASE_WAIT_MS = 350;

/** Display clock only. Native transport remains authoritative. */
export class WaveformMotion {
  private snapshot: WaveformSnapshot | null = null;
  private correction = 0;
  private correctionTime = 0;
  private grab: number | null = null;
  private grabRevision: number | undefined;
  private bridge: { target: number; time: number; revision?: number } | null =
    null;
  private base(now: number) {
    const s = this.snapshot;
    if (!s) return 0;
    let value = s.position;
    if (s.playing && !s.jogTouch) {
      const elapsed = Math.min(STALE_MS, Math.max(0, now - s.time)) / 1000;
      value += Math.max(0, s.playbackRate) * elapsed;
      if (
        s.loopActive !== false &&
        s.loopStart != null &&
        s.loopEnd != null &&
        s.loopEnd > s.loopStart &&
        s.position >= s.loopStart &&
        s.position < s.loopEnd &&
        value >= s.loopEnd
      )
        value =
          s.loopStart + ((value - s.loopStart) % (s.loopEnd - s.loopStart));
    }
    return clamp(value, s.duration);
  }
  position(now: number): number {
    if (this.grab !== null) return this.grab;
    if (this.bridge) {
      if (now - this.bridge.time <= RELEASE_WAIT_MS) return this.bridge.target;
      this.bridge = null;
      this.correction = 0;
    }
    const decay = Math.max(0, 1 - (now - this.correctionTime) / CORRECTION_MS);
    return clamp(
      this.base(now) + this.correction * decay,
      this.snapshot?.duration || 0,
    );
  }
  update(next: WaveformSnapshot, now = next.time): void {
    const prior = this.snapshot,
      oldDisplay = this.position(now);
    const sourceChanged = !!prior && prior.sourceId !== next.sourceId;
    if (sourceChanged) {
      this.grab = null;
      this.bridge = null;
      this.correction = 0;
    }
    this.snapshot = {
      ...next,
      position: clamp(next.position, next.duration),
      time: Number.isFinite(next.time) ? next.time : now,
      playbackRate: Number.isFinite(next.playbackRate) ? next.playbackRate : 1,
    };
    if (this.grab !== null) return;
    if (this.bridge) {
      const acknowledged =
        !next.jogTouch &&
        next.time >= this.bridge.time &&
        (this.bridge.revision === undefined ||
          (next.positionRevision !== undefined &&
            next.positionRevision > this.bridge.revision));
      if (!acknowledged) return;
      this.bridge = null;
      const error = oldDisplay - this.base(now);
      this.correction = Math.abs(error) < 0.1 ? error : 0;
      this.correctionTime = now;
      return;
    }
    const revisionChanged = prior?.positionRevision !== next.positionRevision;
    const modeChanged =
      !!prior &&
      (prior.playing !== next.playing || !!prior.jogTouch !== !!next.jogTouch);
    const error = oldDisplay - this.base(now);
    this.correction =
      !prior ||
      sourceChanged ||
      revisionChanged ||
      modeChanged ||
      !next.playing ||
      next.jogTouch ||
      Math.abs(error) > 0.1
        ? 0
        : error;
    this.correctionTime = now;
  }
  beginGrab(now: number): number {
    const p = this.position(now);
    this.grab = p;
    this.grabRevision = this.snapshot?.positionRevision;
    this.bridge = null;
    return p;
  }
  moveGrab(delta: number): number {
    if (this.grab === null || !Number.isFinite(delta)) return 0;
    const before = this.grab;
    this.grab = clamp(before + delta, this.snapshot?.duration || 0);
    return this.grab - before;
  }
  endGrab(now: number): void {
    if (this.grab === null) return;
    this.bridge = { target: this.grab, time: now, revision: this.grabRevision };
    this.grab = null;
    this.correction = 0;
  }
  cancelGrab(): void {
    this.grab = null;
    this.bridge = null;
    this.correction = 0;
  }
  expectSeek(position: number, now: number): void {
    this.grab = null;
    this.bridge = {
      target: clamp(position, this.snapshot?.duration || 0),
      time: now,
      revision: this.snapshot?.positionRevision,
    };
    this.correction = 0;
  }
  animating(now: number): boolean {
    return (
      this.grab !== null ||
      (!!this.bridge && now - this.bridge.time < RELEASE_WAIT_MS) ||
      (!!this.snapshot?.playing &&
        !this.snapshot.jogTouch &&
        now - this.snapshot.time <= STALE_MS) ||
      (this.correction !== 0 && now - this.correctionTime < CORRECTION_MS)
    );
  }
}

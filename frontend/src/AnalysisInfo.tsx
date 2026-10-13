import { useState } from "react";
import { ChevronDown, LoaderCircle, RefreshCw } from "lucide-react";
import type { TrackRecord } from "./types";

const strength = (value: number | undefined | null) =>
  typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.min(1, value))
    : null;
const scoreText = (value: number | null) =>
  value === null ? "unknown" : `${Math.round(value * 100)} / 100`;

export default function AnalysisInfo({
  track,
  onReanalyze,
  baseBpm,
  onChooseBpm,
}: {
  track: TrackRecord;
  onReanalyze: () => Promise<void>;
  baseBpm: number;
  onChooseBpm: (bpm: number) => void;
}) {
  const [details, setDetails] = useState(false);
  const [reanalyzing, setReanalyzing] = useState(false);
  const [error, setError] = useState("");
  const rhythm = strength(track.bpm_confidence),
    key = strength(track.key_confidence);
  const ambiguous = !!track.bpm_ambiguous;
  const manual =
    track.bpm_source === "manual" ||
    (!!track.bpm && Math.abs(baseBpm - track.bpm) > 0.005);
  const tagged = track.bpm_source === "tag" || track.bpm_source === "import";
  const rhythmLabel = manual
    ? "BPM manual"
    : tagged
      ? "BPM tag"
      : ambiguous
        ? "BPM ambiguous"
        : rhythm === null
          ? "BPM unverified"
          : `Rhythm ${scoreText(rhythm)}`;
  const gridLabel =
    track.beatgrid_source === "manual"
      ? "Grid manual"
      : track.beatgrid_source === "synthetic"
        ? "Grid demo"
        : track.beatgrid_confidence != null
          ? "Grid estimated"
          : track.bpm
            ? "Align beatgrid"
            : "Grid unavailable";
  const keyLabel = track.key
    ? `Key strength ${scoreText(key)}`
    : track.key_candidates?.length
      ? "Key ambiguous"
      : "Key unavailable";
  async function reanalyze() {
    setReanalyzing(true);
    setError("");
    try {
      await onReanalyze();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setReanalyzing(false);
    }
  }
  return (
    <div className="analysis-strip">
      <details
        open={details}
        onToggle={(e) => setDetails(e.currentTarget.open)}
      >
        <summary title="Inspect analysis support and alternative tempos">
          <span
            className={`analysis-chip ${ambiguous || (rhythm !== null && rhythm < 0.45) ? "uncertain" : ""}`}
          >
            {rhythmLabel}
          </span>
          <span
            className={`analysis-chip ${!track.key || (key !== null && key < 0.45) ? "uncertain" : ""}`}
          >
            {keyLabel}
          </span>
          <span
            className={`analysis-chip ${gridLabel === "Align beatgrid" ? "uncertain" : ""}`}
          >
            {gridLabel}
          </span>
          <ChevronDown size={10} />
        </summary>
        <div className="analysis-popover">
          <h4>Analysis support</h4>
          <p>
            These scores describe rhythm and tonal evidence. They are heuristic
            strengths, not probabilities of a correct BPM or key.
          </p>
          <p>
            {track.beatgrid_source === "manual"
              ? "The grid origin was set manually."
              : track.beatgrid_confidence != null
                ? "Grid lines follow an estimated periodic beat origin. The stronger markers group four beats; they do not identify a verified musical downbeat."
                : "Align the first beat in the grid editor before relying on the beat lines."}
          </p>
          <div className="analysis-current">
            <span>
              Rhythm support<strong>{scoreText(rhythm)}</strong>
            </span>
            <span>
              Key strength<strong>{scoreText(key)}</strong>
            </span>
          </div>
          {(manual || tagged) && (
            <p>
              {manual
                ? "The current beatgrid tempo is manually overridden."
                : "The tempo comes from a track tag; verify it against the beatgrid."}
            </p>
          )}
          {ambiguous && (
            <p className="analysis-caution">
              More than one tempo is plausible. Check the beatgrid and half /
              double tempo before syncing.
            </p>
          )}{" "}
          {!!track.bpm_candidates?.length && (
            <>
              <h5>Tempo candidates</h5>
              <div className="bpm-candidates">
                {track.bpm_candidates.slice(0, 5).map((candidate, i) => (
                  <button
                    key={`${candidate.bpm}-${i}`}
                    onClick={() => {
                      onChooseBpm(candidate.bpm);
                      setDetails(false);
                    }}
                    title="Review this BPM in the grid editor"
                  >
                    <strong>{candidate.bpm.toFixed(2)} BPM</strong>
                    <span>support {scoreText(strength(candidate.score))}</span>
                  </button>
                ))}
              </div>
            </>
          )}
          {!!track.key_candidates?.length && (
            <>
              <h5>Tonal candidates</h5>
              <div className="key-candidates">
                {track.key_candidates.slice(0, 5).map((candidate, i) => (
                  <div key={`${candidate.label}-${i}`}>
                    <span>
                      {candidate.label} · {candidate.camelot}
                    </span>
                    <small>
                      strength {scoreText(strength(candidate.score))}
                    </small>
                  </div>
                ))}
              </div>
            </>
          )}
          <p>
            Key estimates can vary between sections of a track. Listen before
            relying on harmonic matching.
          </p>
        </div>
      </details>
      <button
        className="reanalyze-button"
        disabled={reanalyzing}
        onClick={reanalyze}
        title="Refresh beat, key and three-band analysis without unloading the deck"
      >
        {reanalyzing ? (
          <LoaderCircle size={10} className="spin" />
        ) : (
          <RefreshCw size={10} />
        )}{" "}
        {reanalyzing ? "Analyzing…" : "Reanalyze"}
      </button>
      {error && (
        <p className="analysis-inline-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

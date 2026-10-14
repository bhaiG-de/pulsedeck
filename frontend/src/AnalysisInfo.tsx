import { useState } from "react";
import { ChevronDown, LoaderCircle, RefreshCw } from "lucide-react";
import type { DeckSnapshot, TrackRecord } from "./types";
import { useControl } from "./control-bridge";
import { controlBoolean, controlNumber } from "./control-state";

const strength = (value: number | undefined | null) =>
  typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.min(1, value))
    : null;
const scoreText = (value: number | null) =>
  value === null ? "unknown" : `${Math.round(value * 100)} / 100`;
const modelScoreText = (value: number | null) =>
  value === null ? "unknown" : value.toFixed(2);

export default function AnalysisInfo({
  track,
  onReanalyze,
  baseBpm,
  onChooseBpm,
  controlTarget,
  deck,
}: {
  track: TrackRecord;
  onReanalyze: () => Promise<void>;
  baseBpm: number;
  onChooseBpm: (bpm: number) => void;
  controlTarget?: string;
  deck?: DeckSnapshot;
}) {
  const [details, setDetails] = useState(false);
  const [reanalyzing, setReanalyzing] = useState(false);
  const [error, setError] = useState("");
  const model = track.neural_rhythm;
  const pulseRepairs = model?.bar_evidence?.flatMap(e => e.pulse_repairs ?? []) ?? [];
  const rhythm = strength(model?.beat_confidence ?? track.bpm_confidence),
    downbeat = strength(model?.downbeat_confidence),
    key = strength(track.key_confidence);
  const ambiguous = !!track.bpm_ambiguous && !deck?.beatgrid_uniform;
  const referenceBpm = model?.bpm ?? track.bpm;
  const manual =
    track.bpm_source === "manual" ||
    (!!referenceBpm && Math.abs(baseBpm - referenceBpm) > 0.005);
  const manualGrid =
    deck?.beatgrid_manual ?? track.beatgrid_source === "manual";
  const validGrid =
    deck?.beatgrid_valid ?? track.beatgrid_offset_seconds != null;
  const tagged =
    !model && (track.bpm_source === "tag" || track.bpm_source === "import");
  const rhythmLabel = manual
    ? "BPM manual"
    : model
      ? ambiguous
        ? pulseRepairs.length ? "Beat model · pulse corrected" : "Beat model · tempo varies"
        : "Beat model"
      : tagged
        ? "BPM tag"
        : ambiguous
          ? "BPM ambiguous"
          : rhythm === null
            ? "BPM unverified"
            : `Rhythm ${scoreText(rhythm)}`;
  const gridLabel =
    deck?.beatgrid_uniform && !manualGrid
      ? "Grid tempo override"
      : manualGrid
        ? "Grid manual"
        : model
          ? "Grid model"
          : track.rhythm_error
            ? validGrid
              ? "Grid fallback"
              : "Model unavailable"
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
  useControl(
    controlTarget || `analysis-${track.id}`,
    {
      details: (v) => setDetails(controlBoolean(v)),
      candidate: (v) => {
        onChooseBpm(controlNumber(v, 30, 300));
        setDetails(false);
      },
      reanalyze,
      "error-dismiss": () => setError(""),
    },
    { details, reanalyzing, error, candidates: track.bpm_candidates },
  );
  return (
    <div className="analysis-strip">
      <details
        open={details}
        onToggle={(e) => setDetails(e.currentTarget.open)}
      >
        <summary title="Inspect analysis support and alternative tempos">
          <span
            className={`analysis-chip ${ambiguous || (!model && rhythm !== null && rhythm < 0.45) ? "uncertain" : ""}`}
            title={
              model
                ? "Beat model responses are uncalibrated; inspect beat and downbeat scores in details."
                : undefined
            }
          >
            {rhythmLabel}
          </span>
          <span
            className={`analysis-chip ${!track.key || (key !== null && key < 0.45) ? "uncertain" : ""}`}
          >
            {keyLabel}
          </span>
          <span
            className={`analysis-chip ${gridLabel === "Align beatgrid" || !!track.rhythm_error ? "uncertain" : ""}`}
          >
            {gridLabel}
          </span>
          <ChevronDown size={10} />
        </summary>
        <div className="analysis-popover">
          <h4>Analysis support</h4>
          <p>
            {model
              ? "Beat and downbeat scores summarize model responses around detected events. They are uncalibrated, not probabilities that the grid is correct. Key scores remain heuristic strengths."
              : "These scores describe rhythm and tonal evidence. They are heuristic strengths, not probabilities of a correct BPM or key."}
          </p>
          <p>
            {deck?.beatgrid_uniform
              ? "The BPM override uses a constant tempo grid across the track. Align bar one on a known downbeat. Original model events remain in the analysis."
              : model
                ? manualGrid
                  ? "The selected beat is aligned manually as bar 1. Detected beat intervals are retained. Stronger markers count four beats from the selected anchor."
                  : "Grid lines follow detected source-time beat events, including changes in tempo. The detector selects a three- or four-beat meter for the track. Stronger markers and bar labels follow its predicted downbeats. Verify the musical alignment by listening or adjust it in the grid editor."
                : manualGrid
                  ? "The periodic grid origin was set manually as bar 1. Stronger markers count four beats from that origin."
                  : track.beatgrid_confidence != null
                    ? "Grid lines follow estimated periodic beats. The musical downbeat and bar origin remain unconfirmed."
                    : "Align the first beat in the grid editor before relying on the beat lines."}
          </p>
          {model && <p>Detector: {model.model}</p>}
          {track.rhythm_error && (
            <p className="analysis-caution" title={track.rhythm_error}>
              {model
                ? "The latest beat analysis failed; the existing model timeline remains available."
                : validGrid
                  ? "Beat model analysis failed. The current grid uses a manual or estimated fallback; verify it before syncing."
                  : "Beat model analysis failed. Align the grid manually or reanalyze before syncing."}
            </p>
          )}
          <div className="analysis-current">
            <span>
              {model ? "Beat score · uncalibrated" : "Rhythm support"}
              <strong>
                {model ? modelScoreText(rhythm) : scoreText(rhythm)}
              </strong>
            </span>
            {model && (
              <span>
                Downbeat score · uncalibrated
                <strong>{modelScoreText(downbeat)}</strong>
              </span>
            )}
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
              {model
                ? "Local beat timing spans a wide tempo range. This may be a tempo change or a half/double interpretation switch. Check the intended passage and pulse before syncing."
                : "More than one tempo is plausible. Check the beatgrid and half / double tempo before syncing."}
            </p>
          )}{" "}
          {model?.beats_per_bar && <p>Detected {model.beats_per_bar} beats per bar. Verify bar alignment before mixing.</p>}
          {!!pulseRepairs.length && <p>Acoustic pulse correction reconstructed {pulseRepairs.reduce((n,r) => n+r.inserted_beats,0)} beats in {pulseRepairs.length} passages. Check these passages before syncing.</p>}
          {!!track.bpm_candidates?.length && (
            <>
              <h5>
                {model ? "Spectral tempo alternatives" : "Tempo candidates"}
              </h5>
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

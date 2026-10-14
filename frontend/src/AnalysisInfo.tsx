import { useState } from "react";
import { ChevronDown, LoaderCircle, RefreshCw } from "lucide-react";
import type { DeckSnapshot, TrackRecord, Vocabulary } from "./types";
import {
  SECTION_COLORS,
  SECTION_NAMES,
  UNLABELLED_SECTION,
  displaySections,
} from "./waveform-data";
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
  onVocabulary,
  baseBpm,
  onChooseBpm,
  controlTarget,
  deck,
}: {
  track: TrackRecord;
  onReanalyze: () => Promise<void>;
  onVocabulary: (vocabulary: Vocabulary | null) => Promise<void>;
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
  const manualGrid = deck?.beatgrid_manual ?? track.beatgrid_source === "manual";
  const validGrid = deck?.beatgrid_valid ?? track.beatgrid_offset_seconds != null;
  const tagged = track.bpm_source === "tag" || track.bpm_source === "import";
  // Only what a DJ has to act on before mixing; scores live in the popover.
  const flags: string[] = [];
  if (!manual && ambiguous) flags.push("bpm ½ / 2×?");
  else if (!manual && (rhythm === null || rhythm < 0.45))
    flags.push("check bpm");
  if (
    track.beatgrid_source !== "manual" &&
    track.beatgrid_source !== "synthetic" &&
    track.beatgrid_confidence == null
  )
    flags.push(track.bpm ? "align grid" : "no grid");
  else if (
    track.beatgrid_source === "model" &&
    (strength(track.beatgrid_confidence) ?? 0) < 0.5
  )
    flags.push("check beat 1");
  if (!track.key) flags.push(track.key_candidates?.length ? "key ?" : "no key");
  else if (key !== null && key < 0.45) flags.push("key ?");
  const stage = track.sections;
  const sections = displaySections(
    stage,
    track.duration_seconds ?? track.duration ?? 0,
  );
  const vocabulary = stage?.vocabulary_override ?? stage?.vocabulary;
  async function chooseVocabulary(choice: Vocabulary) {
    setError("");
    try {
      await onVocabulary(choice === stage?.vocabulary ? null : choice);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  }
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
          {flags.length ? (
            flags.map((flag) => (
              <span className="analysis-chip uncertain" key={flag}>
                {flag}
              </span>
            ))
          ) : (
            <span className="analysis-chip">analysis ok</span>
          )}
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
                  ? "The selected beat is aligned manually as bar 1. Detected beat intervals are retained. Stronger markers follow the detected meter from the selected anchor."
                  : "Grid lines follow detected source-time beat events, including changes in tempo. The detector compares three-, four-, five- and seven-beat meter hypotheses. Stronger markers and bar labels follow its predicted downbeats. Verify the musical alignment by listening or adjust it in the grid editor."
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
          {!manual && track.bpm_source === "model" && (
            <p>
              The tempo comes from the beat model. Rhythm support is the share
              of detected beats that sit on the grid.
            </p>
          )}
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
          {!!sections.length && (
            <>
              <h5>Sections</h5>
              <div className="section-list">
                {sections.map((s, i) => (
                  <span key={`${s.start}-${i}`}>
                    <i
                      style={{
                        background: s.label
                          ? SECTION_COLORS[s.label]
                          : UNLABELLED_SECTION,
                      }}
                    />
                    {s.label ? SECTION_NAMES[s.label] : "Section"} {s.bars}
                  </span>
                ))}
              </div>
              <div className="bpm-candidates">
                {(["dance", "hip_hop"] as const).map((choice) => (
                  <button
                    key={choice}
                    className={vocabulary === choice ? "selected" : ""}
                    onClick={() => chooseVocabulary(choice)}
                    title={
                      choice === "dance"
                        ? "Label intro, build-up, drop, breakdown and outro"
                        : "Label intro, verse, hook and outro"
                    }
                  >
                    <strong>
                      {choice === "dance" ? "Dance labels" : "Hip-hop labels"}
                    </strong>
                    <span>
                      {stage?.vocabulary === choice ? "detected" : "switch"}
                    </span>
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
        aria-label={reanalyzing ? "Analyzing" : "Reanalyze"}
        title="Refresh the beat model, key and three-band analysis without unloading the deck"
      >
        {reanalyzing ? (
          <LoaderCircle size={11} className="spin" />
        ) : (
          <RefreshCw size={11} />
        )}
      </button>
      {error && (
        <p className="analysis-inline-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

import { useState } from "react";
import { ChevronDown, LoaderCircle, RefreshCw } from "lucide-react";
import type { TrackRecord, Vocabulary } from "./types";
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

export default function AnalysisInfo({
  track,
  onReanalyze,
  onVocabulary,
  baseBpm,
  onChooseBpm,
  controlTarget,
}: {
  track: TrackRecord;
  onReanalyze: () => Promise<void>;
  onVocabulary: (vocabulary: Vocabulary | null) => Promise<void>;
  baseBpm: number;
  onChooseBpm: (bpm: number) => void;
  controlTarget?: string;
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
            These scores describe rhythm and tonal evidence. They are heuristic
            strengths, not probabilities of a correct BPM or key.
          </p>
          <p>
            {track.beatgrid_source === "manual"
              ? "The grid origin was set manually."
              : track.beatgrid_source === "model"
                ? "Grid lines follow the beat model. The stronger lines mark beat 1 of each bar; if a bar starts elsewhere, shift beat 1 in the grid editor."
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

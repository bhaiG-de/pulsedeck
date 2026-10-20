import { useEffect, useRef, useState } from "react";
import { Check, Download, LoaderCircle } from "lucide-react";
import { control, isDesktop } from "./api";
import BrandMark from "./BrandMark";

const AGENTS = [
  { id: "codex", label: "Codex", detail: "OpenAI" },
  { id: "claude", label: "Claude Code", detail: "Anthropic" },
] as const;
type Agent = (typeof AGENTS)[number]["id"];
type Report = {
  agent: Agent;
  installed: boolean;
  destination: string;
  backup: string | null;
  error: string | null;
};

export default function AgentSetup({
  close,
  embedded = false,
}: {
  close?: () => void;
  embedded?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [selected, setSelected] = useState<Agent[]>([]);
  const [reports, setReports] = useState<Report[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const installed = new Set(
    reports.filter((r) => r.installed).map((r) => r.agent),
  );
  const pending = selected.filter((agent) => !installed.has(agent));
  const done = installed.size > 0 && pending.length === 0;
  useEffect(() => {
    if (embedded) return;
    const focused = document.activeElement as HTMLElement | null;
    const element = dialog.current;
    if (typeof element?.showModal === "function") element.showModal();
    else if (element) {
      // Older WebViews still need a visible, keyboard-dismissable welcome.
      element.setAttribute("open", "");
      element.classList.add("legacy-dialog");
      element.focus();
    }
    return () => {
      focused?.focus();
    };
  }, [embedded]);

  async function install() {
    if (busy || !pending.length) return;
    setBusy(true);
    setError("");
    try {
      const result = await control<Report[]>("skills.install", {
        agents: pending,
      });
      setReports((previous) => [
        ...previous.filter((r) => !pending.includes(r.agent)),
        ...result,
      ]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const content = (
    <>
      {!embedded && (
        <header className="agent-setup-brand">
          <span>
            <BrandMark /> <strong>pulsedeck</strong>
          </span>
          <small>
            <i /> WELCOME / 01
          </small>
        </header>
      )}
      <div className="agent-setup-body">
        <p className="agent-setup-eyebrow">YOUR MUSIC. A LITTLE HELP.</p>
        <h2 id="agent-setup-title">
          {embedded ? "Your agents. Ready to DJ." : "Welcome to PulseDeck."}
        </h2>
        <p id="agent-setup-description">
          Give your coding agent the DJ skills to find tracks, prepare a mix and
          control PulseDeck.
        </p>
        <div
          className="agent-setup-choices"
          role="group"
          aria-label="Install DJ skills for"
        >
          {AGENTS.map((agent) => {
            const report = reports.find((r) => r.agent === agent.id);
            return (
              <label
                key={agent.id}
                className={selected.includes(agent.id) ? "selected" : ""}
              >
                <span className="agent-setup-choice-heading">
                  <strong>{agent.label}</strong>
                  <input
                    type="checkbox"
                    checked={selected.includes(agent.id)}
                    disabled={busy || installed.has(agent.id)}
                    onChange={(e) =>
                      setSelected((previous) =>
                        e.target.checked
                          ? [...previous, agent.id]
                          : previous.filter((id) => id !== agent.id),
                      )
                    }
                  />
                </span>
                <span className="agent-setup-agent-detail">
                  {installed.has(agent.id) ? (
                    <>
                      <Check size={12} /> Installed
                    </>
                  ) : (
                    agent.detail
                  )}
                </span>
                {report?.error && selected.includes(agent.id) && (
                  <span role="alert" className="agent-setup-error">
                    {report.error}
                  </span>
                )}
              </label>
            );
          })}
        </div>
        <p className="agent-setup-note">
          Choose one or both. Three skills per agent. Existing skill packages
          are backed up before updates.
        </p>
        {installed.size > 0 && (
          <p role="status" className="agent-setup-success">
            <Check size={15} /> Restart{" "}
            {AGENTS.filter((a) => installed.has(a.id))
              .map((a) => a.label)
              .join(" and ")}{" "}
            to discover your DJ skills.
          </p>
        )}
        {reports.some((r) => r.backup) && (
          <details className="agent-setup-backups">
            <summary>Previous skills saved</summary>
            {reports
              .filter((r) => r.backup)
              .map((r) => (
                <p key={r.agent}>{r.backup}</p>
              ))}
          </details>
        )}
        {error && (
          <p role="alert" className="agent-setup-error">
            {error}
          </p>
        )}
        {!isDesktop && (
          <p className="agent-setup-note">
            Open the desktop app to install skills.
          </p>
        )}
      </div>
      <footer className="agent-setup-footer">
        <span>
          {embedded
            ? "Skills stay on this Mac. No account connection needed."
            : "Available anytime in Settings → Agent skills."}
        </span>
        <div>
          {!embedded && (
            <button disabled={busy} onClick={close}>
              {done ? "Close" : "Not now"}
            </button>
          )}
          <button
            className="import-button"
            disabled={
              busy ||
              (!done && (!pending.length || !isDesktop)) ||
              (embedded && done)
            }
            onClick={() => (done ? close?.() : void install())}
          >
            {busy ? (
              <LoaderCircle className="spin" size={14} />
            ) : done ? (
              <Check size={14} />
            ) : (
              <Download size={14} />
            )}
            {busy
              ? "Installing…"
              : done
                ? embedded
                  ? "Installed"
                  : "Continue"
                : "Install skills"}
          </button>
        </div>
      </footer>
    </>
  );
  return embedded ? (
    <section className="agent-setup embedded">{content}</section>
  ) : (
    <dialog
      ref={dialog}
      tabIndex={-1}
      className="agent-setup"
      aria-labelledby="agent-setup-title"
      aria-describedby="agent-setup-description"
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") {
          e.preventDefault();
          if (!busy) close?.();
        }
      }}
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) close?.();
      }}
    >
      {content}
    </dialog>
  );
}

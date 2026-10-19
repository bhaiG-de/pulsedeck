import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { LatestControlWriter } from "./control-state";

export default function TempoSlider({
  value,
  range,
  write,
  children,
}: {
  value: number;
  range: number;
  write: (percent: number) => Promise<void>;
  children?: ReactNode;
}) {
  const [draft, setDraft] = useState<number | null>(null);
  const [gesture, setGesture] = useState(false);
  const [busy, setBusy] = useState(false);
  const [bounds, setBounds] = useState<[number, number] | null>(null);
  const writeRef = useRef(write);
  writeRef.current = write;
  const writer = useRef<LatestControlWriter | null>(null);
  useEffect(() => {
    const current = new LatestControlWriter(
      (percent) => writeRef.current(percent),
      (error) => {
        setBusy(false);
        if (error) setDraft(null);
      },
    );
    writer.current = current;
    return () => {
      current.dispose();
      writer.current = null;
    };
  }, []);
  // Wait for the renderer echo after release; never let stale snapshots move a drag.
  useEffect(() => {
    if (gesture || busy || draft === null) return;
    if (Math.abs(value - draft) < 0.006) {
      setDraft(null);
      return;
    }
    const timer = setTimeout(() => setDraft(null), 500);
    return () => clearTimeout(timer);
  }, [value, draft, gesture, busy]);
  // A symmetric, discrete range avoids a moving scale when Sync is outside ±6%.
  const extent =
    [6, 10, 16, 100].find((n) => n >= range && n >= Math.abs(value)) ?? 100;
  const limits: [number, number] = bounds ?? [Math.max(-90, -extent), extent];
  const begin = () => {
    setGesture(true);
    setBounds(limits);
  };
  const end = () => {
    setGesture(false);
    setBounds(null);
  };
  return (
    <>
      <label className="range-control vertical">
        <span>TEMPO</span>
        <input
          aria-label="TEMPO"
          type="range"
          min={limits[0]}
          max={limits[1]}
          step={0.01}
          value={draft ?? value}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture(event.pointerId);
            begin();
          }}
          onPointerUp={end}
          onPointerCancel={end}
          onLostPointerCapture={end}
          onKeyDown={(event) => {
            if (
              [
                "ArrowUp",
                "ArrowDown",
                "ArrowLeft",
                "ArrowRight",
                "Home",
                "End",
                "PageUp",
                "PageDown",
              ].includes(event.key)
            )
              begin();
          }}
          onKeyUp={end}
          onBlur={end}
          onChange={(event) => {
            const percent = +event.target.value;
            setDraft(percent);
            setBusy(true);
            writer.current?.set(percent);
          }}
        />
      </label>
      {children}
      <button
        className="tempo-reset"
        onClick={() => {
          end();
          setDraft(0);
          setBusy(true);
          writer.current?.set(0);
        }}
      >
        RESET
      </button>
    </>
  );
}

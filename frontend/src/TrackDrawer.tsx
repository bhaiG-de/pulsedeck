import {
  type ReactNode,
  type KeyboardEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { X } from "lucide-react";
import { useControl } from "./control-bridge";
import { controlNumber } from "./control-state";

/** Nonmodal so tracks can be dragged onto the live decks behind the sheet. */
export default function TrackDrawer({
  open,
  onClose,
  title,
  icon,
  children,
  controllerCloseFocused = false,
  controllerPaneFocused = false,
  onNavigationKey,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  icon?: ReactNode;
  children: ReactNode;
  controllerCloseFocused?: boolean;
  controllerPaneFocused?: boolean;
  onNavigationKey?: (event: KeyboardEvent) => void;
}) {
  const sheet = useRef<HTMLElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const gesture = useRef<{ y: number; height: number } | null>(null);
  const [height, setHeight] = useState(70);
  const [dragging, setDragging] = useState(false);
  useControl(
    "drawer",
    {
      height: (v) => {
        const n = controlNumber(v, 10, 72);
        setHeight(n);
        if (n < 18) onClose();
      },
      "height-snap": () =>
        setHeight(
          [28, 44, 70].reduce((best, snap) =>
            Math.abs(snap - height) < Math.abs(best - height) ? snap : best,
          ),
        ),
      close: onClose,
    },
    { open, height, dragging },
  );
  useEffect(() => {
    const element = sheet.current;
    const hadFocus = element?.contains(document.activeElement);
    if (element) element.inert = !open;
    if (open) {
      setHeight(70);
      previousFocus.current = document.activeElement as HTMLElement;
      if (!controllerPaneFocused)
        heading.current?.focus({ preventScroll: true });
    } else if (hadFocus) {
      previousFocus.current?.focus({ preventScroll: true });
    }
  }, [open]);
  useEffect(() => {
    if (open && controllerCloseFocused)
      closeButton.current?.focus({ preventScroll: true });
  }, [open, controllerCloseFocused]);
  function finishDrag() {
    if (!gesture.current) return;
    gesture.current = null;
    setDragging(false);
    if (height < 18) {
      onClose();
      setHeight(70);
    } else
      setHeight(
        [28, 44, 70].reduce((best, snap) =>
          Math.abs(snap - height) < Math.abs(best - height) ? snap : best,
        ),
      );
  }
  return (
    <>
      <div
        className={`drawer-shade ${open ? "is-open" : ""}`}
        aria-hidden="true"
      />
      <section
        ref={sheet}
        role="dialog"
        aria-modal="false"
        aria-labelledby="track-drawer-title"
        aria-hidden={!open}
        className={`track-drawer ${open ? "is-open" : ""} ${dragging ? "is-resizing" : ""}`}
        style={{ height: `${height}dvh` }}
        onKeyDownCapture={onNavigationKey}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            onClose();
          }
        }}
        onDragStartCapture={() => setHeight(28)}
      >
        <div className="drawer-handle-area">
          <div
            role="slider"
            tabIndex={0}
            aria-label="Resize track browser"
            aria-valuemin={10}
            aria-valuemax={72}
            aria-valuenow={height}
            aria-valuetext={`${Math.round(height)} percent of window height`}
            className="drawer-swipe-handle"
            onPointerDown={(e) => {
              e.preventDefault();
              e.currentTarget.setPointerCapture(e.pointerId);
              gesture.current = { y: e.clientY, height };
              setDragging(true);
            }}
            onPointerMove={(e) => {
              if (gesture.current)
                setHeight(
                  Math.min(
                    72,
                    Math.max(
                      10,
                      gesture.current.height +
                        ((gesture.current.y - e.clientY) / window.innerHeight) *
                          100,
                    ),
                  ),
                );
            }}
            onPointerUp={(e) => {
              e.currentTarget.releasePointerCapture(e.pointerId);
              finishDrag();
            }}
            onPointerCancel={finishDrag}
            onKeyDown={(e) => {
              if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                e.preventDefault();
                setHeight((h) =>
                  Math.max(
                    28,
                    Math.min(70, h + (e.key === "ArrowUp" ? 8 : -8)),
                  ),
                );
              } else if (e.key === "Home") {
                e.preventDefault();
                setHeight(28);
              } else if (e.key === "End") {
                e.preventDefault();
                setHeight(70);
              }
            }}
          >
            <span />
          </div>
        </div>
        <header className="drawer-heading">
          <div>
            {icon && <span className="drawer-icon">{icon}</span>}
            <h2 id="track-drawer-title" ref={heading} tabIndex={-1}>
              {title}
            </h2>
          </div>
          <button
            ref={closeButton}
            data-controller-close
            className={`icon-button ${controllerCloseFocused ? "controller-focused" : ""}`}
            aria-label="Close track browser"
            onClick={onClose}
          >
            <X size={18} />
          </button>
        </header>
        <div className="drawer-content">{children}</div>
      </section>
    </>
  );
}

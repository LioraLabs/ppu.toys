import "../styles/tokens.css";
import "./studio.css";
import { useRef, useState } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactNode } from "react";

export interface StudioLayoutProps {
  /** Top chrome strip (ToolbarWired in the app). */
  toolbar: ReactNode;
  /** Left activity rail (ActivityRailWired in the app). */
  rail: ReactNode;
  /** Center editor column — expected to render `<section className="editor">`
   *  (EditorPane in the app). */
  editor: ReactNode;
  /** Right column — expected to render `<aside className="right">`
   *  (RightColumn in the app). */
  right: ReactNode;
  /** Bottom dock under the editor (Inspector in the app). Height drags on the
   *  dock bar; visibility is the caller's (dockOpen/onDockToggle — the app
   *  wires the shared inspector store so the rail can reveal the dock too).
   *  Omit the slot and the bar disappears with it. */
  dock?: ReactNode;
  dockOpen?: boolean;
  onDockToggle?: () => void;
}

const RIGHT_W_KEY = "ppu.rightPaneW";
const MIN_RIGHT = 380; // narrower and the output/tab chrome wraps unusably
const MIN_EDITOR = 380; // keep a workable editor no matter how far the drag goes

const DOCK_H_KEY = "ppu.dockH";
const MIN_DOCK = 140;
const MIN_EDITOR_H = 160;
/** Movement below this is a click (toggle), not a drag (resize). */
const DRAG_SLOP = 4;

function loadRightW(): number {
  const v = Number(localStorage.getItem(RIGHT_W_KEY));
  return Number.isFinite(v) && v >= MIN_RIGHT ? v : 600;
}
function loadDockH(): number {
  const v = Number(localStorage.getItem(DOCK_H_KEY));
  return Number.isFinite(v) && v >= MIN_DOCK ? v : 280;
}
/** Presentational studio arrangement: the toolbar-over-three-columns grid that
 *  studio.css hangs off, with every region injected as a slot. Studio fills the
 *  slots with the wired app; the shell fixture (StudioLayout.fixture) fills them
 *  with fixture-fed presentational pieces so the whole composition renders
 *  wasm-free. Owns the tokens/studio css imports so both fillers get styled.
 *
 *  Two persisted user splits: the editor|right divider drags the right
 *  column's width (--right-w inline), and the dock bar under the editor is a
 *  click-to-toggle, drag-to-resize handle for the bottom dock. */
export function StudioLayout({
  toolbar,
  rail,
  editor,
  right,
  dock,
  dockOpen = true,
  onDockToggle,
}: StudioLayoutProps) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const centerRef = useRef<HTMLDivElement>(null);
  const [rightW, setRightW] = useState(loadRightW);
  const [dockH, setDockH] = useState(loadDockH);

  const startRightDrag = (e: ReactPointerEvent) => {
    e.preventDefault();
    const body = bodyRef.current;
    if (!body) return;
    const rect = body.getBoundingClientRect();
    const railW = body.firstElementChild?.getBoundingClientRect().width ?? 0;
    const max = Math.max(MIN_RIGHT, rect.width - railW - MIN_EDITOR);
    let latest = rightW;
    const move = (ev: PointerEvent) => {
      latest = Math.round(Math.min(max, Math.max(MIN_RIGHT, rect.right - ev.clientX)));
      setRightW(latest);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      localStorage.setItem(RIGHT_W_KEY, String(latest));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  // One bar, two gestures: a clean click toggles the dock; any real vertical
  // drag (while open) resizes it.
  const startDockDrag = (e: ReactPointerEvent) => {
    e.preventDefault();
    const center = centerRef.current;
    if (!center) return;
    const rect = center.getBoundingClientRect();
    const max = Math.max(MIN_DOCK, rect.height - MIN_EDITOR_H);
    const startY = e.clientY;
    let latest = dockH;
    let dragged = false;
    const move = (ev: PointerEvent) => {
      if (!dockOpen) return; // closed: the bar is a plain button
      if (!dragged && Math.abs(ev.clientY - startY) < DRAG_SLOP) return;
      dragged = true;
      latest = Math.round(Math.min(max, Math.max(MIN_DOCK, rect.bottom - ev.clientY)));
      setDockH(latest);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (dragged) localStorage.setItem(DOCK_H_KEY, String(latest));
      else onDockToggle?.();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div className="studio">
      {toolbar}
      <div
        className="studio-body"
        ref={bodyRef}
        style={{ "--right-w": `${rightW}px` } as CSSProperties}
      >
        {rail}
        <div className="center-col" ref={centerRef}>
          {editor}
          {dock !== undefined && (
            <>
              <button
                type="button"
                className="dock-bar"
                data-open={dockOpen}
                aria-expanded={dockOpen}
                aria-label={dockOpen ? "Hide inspector (drag to resize)" : "Show inspector"}
                onPointerDown={startDockDrag}
              >
                <span className="dock-bar-chev">{dockOpen ? "▾" : "▴"}</span>
                INSPECTOR
              </button>
              {dockOpen && (
                <div className="dock" style={{ height: dockH }}>
                  {dock}
                </div>
              )}
            </>
          )}
        </div>
        <div
          className="pane-splitter"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize output column"
          onPointerDown={startRightDrag}
        />
        {right}
      </div>
    </div>
  );
}

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
}

const RIGHT_W_KEY = "ppu.rightPaneW";
const MIN_RIGHT = 380; // narrower and the inspector tabs wrap unusably
const MIN_EDITOR = 380; // keep a workable editor no matter how far the drag goes

function loadRightW(): number {
  const v = Number(localStorage.getItem(RIGHT_W_KEY));
  return Number.isFinite(v) && v >= MIN_RIGHT ? v : 600;
}

/** Presentational studio arrangement: the toolbar-over-three-columns grid that
 *  studio.css hangs off, with every region injected as a slot. Studio fills the
 *  slots with the wired app; the shell fixture (StudioLayout.fixture) fills them
 *  with fixture-fed presentational pieces so the whole composition renders
 *  wasm-free. Owns the tokens/studio css imports so both fillers get styled.
 *
 *  The editor|right divider is a drag handle: the right column's width
 *  overrides --right-w inline and persists across sessions. */
export function StudioLayout({ toolbar, rail, editor, right }: StudioLayoutProps) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const [rightW, setRightW] = useState(loadRightW);

  const startDrag = (e: ReactPointerEvent) => {
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

  return (
    <div className="studio">
      {toolbar}
      <div
        className="studio-body"
        ref={bodyRef}
        style={{ "--right-w": `${rightW}px` } as CSSProperties}
      >
        {rail}
        {editor}
        <div
          className="pane-splitter"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize output column"
          onPointerDown={startDrag}
        />
        {right}
      </div>
    </div>
  );
}

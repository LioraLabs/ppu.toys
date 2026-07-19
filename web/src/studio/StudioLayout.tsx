import "../styles/tokens.css";
import "./studio.css";
import type { ReactNode } from "react";

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

/** Presentational studio arrangement: the toolbar-over-three-columns grid that
 *  studio.css hangs off, with every region injected as a slot. Studio fills the
 *  slots with the wired app; the shell fixture (StudioLayout.fixture) fills them
 *  with fixture-fed presentational pieces so the whole composition renders
 *  wasm-free. Owns the tokens/studio css imports so both fillers get styled. */
export function StudioLayout({ toolbar, rail, editor, right }: StudioLayoutProps) {
  return (
    <div className="studio">
      {toolbar}
      <div className="studio-body">
        {rail}
        {editor}
        {right}
      </div>
    </div>
  );
}

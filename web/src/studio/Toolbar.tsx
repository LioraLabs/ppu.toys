import { type ReactNode } from "react";
import type { Theme } from "./theme";

export interface ToolbarProps {
  /** Open-sketch name shown in the project chip. */
  sketchName?: string;
  /** Unsaved-changes marker — renders the unsaved dot when true. */
  dirty?: boolean;
  /** Active theme; picks the toggle button's label ("Light" while dark). */
  theme?: Theme;
  /** ▶ Run handler (restart the transport in production). */
  onRun?: () => void;
  /** Theme toggle handler. */
  onToggleTheme?: () => void;
  /** Injected wired "+ Source" control (AddSourceButton in production). Kept as
   *  a slot because it transitively imports transport/ppuCore, which the
   *  presentational toolbar must not. */
  sourceSlot?: ReactNode;
  /** Injected wired cloud actions (WorkspaceActions in production). Slot for the
   *  same reason — it reads the session/network. */
  workspaceSlot?: ReactNode;
}

/** Presentational toolbar: a pure function of props + injected action slots. No
 *  transport, theme store, or wired children imported here — ToolbarWired
 *  supplies the handlers and the AddSourceButton/WorkspaceActions slots. */
export function Toolbar({
  sketchName = "dusk-parallax",
  dirty = false,
  theme = "dark",
  onRun,
  onToggleTheme,
  sourceSlot,
  workspaceSlot,
}: ToolbarProps) {
  return (
    <header className="toolbar">
      <div className="logo-mark">p</div>
      <div className="wordmark">
        ppu<span className="dot">.</span>toys
      </div>
      <div className="tb-divider" />
      <div className="project">
        <span className="project-name">{sketchName}</span>
        {dirty && <span className="unsaved-dot" />}
      </div>
      <div className="tb-spacer" />
      <button type="button" className="btn-solid" onClick={() => onRun?.()}>
        ▶ Run
      </button>
      {sourceSlot}
      <button type="button" className="btn-ghost" onClick={() => onToggleTheme?.()} aria-label="Toggle color theme">
        {theme === "dark" ? "Light" : "Dark"}
      </button>
      {workspaceSlot}
      <div className="avatar" />
    </header>
  );
}

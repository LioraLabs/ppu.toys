import { useEffect, useRef, useState, type ReactNode } from "react";
import { Avatar } from "../components/Avatar";
import type { Theme } from "./theme";

export interface ToolbarUser {
  id: string;
  handle: string;
  avatar: string | null;
}

export interface ToolbarProps {
  /** Open-sketch name shown in the project chip. */
  sketchName?: string;
  /** Unsaved-changes marker — renders the unsaved dot when true. */
  dirty?: boolean;
  /** Active theme; picks the toggle button's label ("Light" while dark). */
  theme?: Theme;
  /** Signed-in user for the account menu; absent renders no account chrome
   *  (the workspace slot already carries the sign-in link). */
  user?: ToolbarUser | null;
  /** ▶ Run handler (restart the transport in production). */
  onRun?: () => void;
  /** Theme toggle handler. */
  onToggleTheme?: () => void;
  /** Account-menu sign out. */
  onSignOut?: () => void;
  /** Account-menu "New toy" (opens a fresh starter in the studio). */
  onNewToy?: () => void;
  /** Injected wired cloud actions (WorkspaceActions in production). Slot for the
   *  same reason — it reads the session/network. */
  workspaceSlot?: ReactNode;
}

/** Account avatar + dropdown. Plain anchors, not router Links — the toolbar
 *  renders router-less in fixtures, and leaving the studio is a real page
 *  navigation anyway. */
function AccountMenu({ user, onSignOut, onNewToy }: { user: ToolbarUser; onSignOut?: () => void; onNewToy?: () => void }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // light-dismiss: click anywhere outside, or Escape
  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="tb-account" ref={rootRef}>
      <button
        type="button"
        className="tb-avatar-btn"
        aria-label={`Account menu for ${user.handle}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <Avatar handle={user.handle} id={user.id} avatar={user.avatar} size={30} />
      </button>
      {open && (
        <div className="tb-menu" role="menu">
          <a className="tb-menu-item" role="menuitem" href={`/u/${user.handle}`}>
            {user.handle}
          </a>
          <button
            type="button"
            className="tb-menu-item"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onNewToy?.();
            }}
          >
            New toy
          </button>
          <a className="tb-menu-item" role="menuitem" href="/">
            Wall
          </a>
          <button type="button" className="tb-menu-item" role="menuitem" onClick={() => onSignOut?.()}>
            Sign out
          </button>
        </div>
      )}
    </div>
  );
}

/** Presentational toolbar: a pure function of props + injected action slots. No
 *  transport, theme store, or wired children imported here — ToolbarWired
 *  supplies the handlers and the WorkspaceActions slot. */
export function Toolbar({
  sketchName = "dusk-parallax",
  dirty = false,
  theme = "dark",
  user = null,
  onRun,
  onToggleTheme,
  onSignOut,
  onNewToy,
  workspaceSlot,
}: ToolbarProps) {
  return (
    <header className="toolbar">
      <a className="tb-home" href="/" title="Back to the wall">
        <div className="logo-mark">p</div>
        <div className="wordmark">
          ppu<span className="dot">.</span>toys
        </div>
      </a>
      <div className="tb-divider" />
      <div className="project">
        <span className="project-name">{sketchName}</span>
        {dirty && <span className="unsaved-dot" />}
      </div>
      <div className="tb-spacer" />
      <button type="button" className="btn-solid" onClick={() => onRun?.()} title="Restart from t=0 (Ctrl+Enter)">
        ▶ Run
      </button>
      <button type="button" className="btn-ghost" onClick={() => onToggleTheme?.()} aria-label="Toggle color theme">
        {theme === "dark" ? "Light" : "Dark"}
      </button>
      {workspaceSlot}
      {user && <AccountMenu user={user} onSignOut={onSignOut} onNewToy={onNewToy} />}
    </header>
  );
}

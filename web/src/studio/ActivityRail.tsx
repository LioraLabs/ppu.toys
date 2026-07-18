import { type ReactNode } from "react";

export type RailItemId = "files" | "layers" | "palette" | "sprites" | "settings";

export interface ActivityRailProps {
  /** Item shown selected (inset-indicator). Defaults to the handoff's active item. */
  active?: RailItemId;
  /** Whether the Files panel is open — drives the Files button's pressed/active
   *  appearance independently of `active` (the file library is a toggle, not a
   *  persistent view selection). Owned by the wired wrapper. */
  filesOpen?: boolean;
  /** Rail action registration point. The wired wrapper toggles the sketch
   *  library on "files"; later tickets claim the rest. No-op when absent. */
  onSelect?: (id: RailItemId) => void;
}

function RailItem({
  id,
  active,
  label,
  className,
  onSelect,
  children,
}: {
  id: RailItemId;
  active?: boolean;
  label: string;
  className?: string;
  onSelect?: (id: RailItemId) => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={"rail-item" + (active ? " rail-item--active" : "") + (className ? " " + className : "")}
      title={label}
      aria-label={label}
      onClick={() => onSelect?.(id)}
    >
      {children}
    </button>
  );
}

/** Presentational left nav: a pure function of `active` + `filesOpen`. Renders no
 *  panels and reads no store — the wired wrapper (ActivityRailWired) owns the
 *  files-panel state and mounts LibraryPanel. */
export function ActivityRail({ active = "layers", filesOpen = false, onSelect }: ActivityRailProps) {
  return (
    <nav className="rail">
      <RailItem id="files" label="Files" active={filesOpen || active === "files"} onSelect={onSelect}>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="2.5" width="12" height="13" rx="2"/>
          <line x1="3" y1="6" x2="15" y2="6"/>
        </svg>
      </RailItem>

      <RailItem id="layers" label="Memory & layers" active={active === "layers"} onSelect={onSelect}>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <path d="M9 2 16 6 9 10 2 6Z"/>
          <path d="M2 10 9 14 16 10"/>
        </svg>
      </RailItem>

      <RailItem id="palette" label="Palette" active={active === "palette"} onSelect={onSelect}>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="3" width="5" height="5" rx="1" fill="currentColor"/>
          <rect x="10" y="3" width="5" height="5" rx="1"/>
          <rect x="3" y="10" width="5" height="5" rx="1"/>
          <rect x="10" y="10" width="5" height="5" rx="1" fill="currentColor"/>
        </svg>
      </RailItem>

      <RailItem id="sprites" label="Sprites" active={active === "sprites"} onSelect={onSelect}>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="3" width="12" height="12" rx="2"/>
          <rect x="6" y="7" width="2" height="2" fill="currentColor" stroke="none"/>
          <rect x="10" y="10" width="2" height="2" fill="currentColor" stroke="none"/>
        </svg>
      </RailItem>

      <div className="rail-spacer" />

      <RailItem id="settings" label="Settings" className="settings" active={active === "settings"} onSelect={onSelect}>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <circle cx="9" cy="9" r="3"/>
          <circle cx="9" cy="9" r="6.5" strokeDasharray="2 2"/>
        </svg>
      </RailItem>
    </nav>
  );
}

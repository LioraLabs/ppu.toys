import { type ReactNode } from "react";

export type RailItemId = "assets" | "layers" | "palette" | "sprites" | "settings";

export interface ActivityRailProps {
  /** Item shown selected (inset-indicator); none when absent. */
  active?: RailItemId;
  /** Whether the Assets panel is open — drives that button's pressed/active
   *  appearance independently of `active` (the flyout is a toggle, not a
   *  persistent view selection). Owned by the wired wrapper. */
  assetsOpen?: boolean;
  /** Same toggle treatment for the Settings flyout. */
  settingsOpen?: boolean;
  /** Rail actions: assets/settings toggle their flyouts, layers/palette/sprites
   *  jump to the matching inspector view (wired wrapper). No-op when absent. */
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

/** Presentational left nav: a pure function of its props. Renders no panels
 *  and reads no store — the wired wrapper (ActivityRailWired) owns the flyout
 *  state and mounts AssetsPanel / SettingsPanel. */
export function ActivityRail({ active, assetsOpen = false, settingsOpen = false, onSelect }: ActivityRailProps) {
  return (
    <nav className="rail">
      <RailItem id="assets" label="Assets" active={assetsOpen || active === "assets"} onSelect={onSelect}>
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

      <RailItem id="settings" label="Settings" className="settings" active={settingsOpen || active === "settings"} onSelect={onSelect}>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <circle cx="9" cy="9" r="3"/>
          <circle cx="9" cy="9" r="6.5" strokeDasharray="2 2"/>
        </svg>
      </RailItem>
    </nav>
  );
}

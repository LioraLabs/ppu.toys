import { useState, type ReactNode } from "react";
import { LibraryPanel } from "./sketches/LibraryPanel";

function RailItem({
  active,
  label,
  className,
  onClick,
  children,
}: {
  active?: boolean;
  label: string;
  className?: string;
  onClick?: () => void;
  children: ReactNode;
}) {
  return (
    <button
      className={"rail-item" + (active ? " rail-item--active" : "") + (className ? " " + className : "")}
      title={label}
      aria-label={label}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

export function ActivityRail() {
  const [filesOpen, setFilesOpen] = useState(false);
  return (
    <>
    <nav className="rail">
      <RailItem label="Files" active={filesOpen} onClick={() => setFilesOpen((v) => !v)}>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="2.5" width="12" height="13" rx="2"/>
          <line x1="3" y1="6" x2="15" y2="6"/>
        </svg>
      </RailItem>

      <RailItem label="Layers" active>
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <path d="M9 2 16 6 9 10 2 6Z"/>
          <path d="M2 10 9 14 16 10"/>
        </svg>
      </RailItem>

      <RailItem label="Palette">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="3" width="5" height="5" rx="1" fill="currentColor"/>
          <rect x="10" y="3" width="5" height="5" rx="1"/>
          <rect x="3" y="10" width="5" height="5" rx="1"/>
          <rect x="10" y="10" width="5" height="5" rx="1" fill="currentColor"/>
        </svg>
      </RailItem>

      <RailItem label="Sprites">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="3" width="12" height="12" rx="2"/>
          <rect x="6" y="7" width="2" height="2" fill="currentColor" stroke="none"/>
          <rect x="10" y="10" width="2" height="2" fill="currentColor" stroke="none"/>
        </svg>
      </RailItem>

      <RailItem label="Registers">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <line x1="3" y1="5" x2="15" y2="5"/>
          <line x1="3" y1="9" x2="15" y2="9"/>
          <line x1="3" y1="13" x2="10" y2="13"/>
        </svg>
      </RailItem>

      <div className="rail-spacer" />

      <RailItem label="Settings" className="settings">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
          <circle cx="9" cy="9" r="3"/>
          <circle cx="9" cy="9" r="6.5" strokeDasharray="2 2"/>
        </svg>
      </RailItem>
    </nav>
    {filesOpen && <LibraryPanel onClose={() => setFilesOpen(false)} />}
    </>
  );
}

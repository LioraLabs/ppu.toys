import { transport } from "./transport/transport";
import { useTheme } from "./theme";
import { AddSourceButton } from "./sources/AddSourceButton";

export interface ToolbarProps {
  /** Open-sketch name. Placeholder default until the sketch store wires
   *  the real open sketch through Studio. */
  sketchName?: string;
  /** Unsaved-changes marker — the sketch store's dirty seam lands here. */
  dirty?: boolean;
}

export function Toolbar({ sketchName = "dusk-parallax", dirty = false }: ToolbarProps) {
  const { theme, toggleTheme } = useTheme();
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
      <button type="button" className="btn-solid" onClick={() => transport.restart()}>
        ▶ Run
      </button>
      <AddSourceButton />
      <button type="button" className="btn-ghost" onClick={toggleTheme} aria-label="Toggle color theme">
        {theme === "dark" ? "Light" : "Dark"}
      </button>
      {/* Share button intentionally absent — hidden until S1 */}
      <div className="avatar" />
    </header>
  );
}

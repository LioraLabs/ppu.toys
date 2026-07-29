import type { Theme } from "./theme";
import "./sketches/sketches.css"; // library-head/title/btn chrome shared with the panel flyouts

export interface SettingsPanelProps {
  theme: Theme;
  onToggleTheme: () => void;
  /** Editor input mode: true = vim keybindings, false = plain. */
  vimMode: boolean;
  onToggleVim: () => void;
  onClose: () => void;
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <span className="settings-label">{label}</span>
        {hint && <span className="settings-hint">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

function Segment<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (next: T) => void;
  label: string;
}) {
  return (
    <div className="settings-seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          className={"settings-seg-btn" + (value === o.id ? " settings-seg-btn--on" : "")}
          aria-pressed={value === o.id}
          onClick={() => value !== o.id && onChange(o.id)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Settings flyout, mounted off the rail's gear item. Presentational — the
 *  wired wrapper (ActivityRailWired) supplies the theme + editor stores. */
export function SettingsPanel({
  theme,
  onToggleTheme,
  vimMode,
  onToggleVim,
  onClose,
}: SettingsPanelProps) {
  return (
    <aside className="settings-panel" aria-label="Settings">
      <header className="library-head">
        <span className="library-title">SETTINGS</span>
        <button type="button" className="library-btn" aria-label="Close settings" onClick={onClose}>
          ×
        </button>
      </header>
      <Row label="Editor keys" hint="Vim is opt-in; plain is standard editing">
        <Segment
          label="Editor keybindings"
          value={vimMode ? "vim" : "plain"}
          options={[
            { id: "plain", label: "Plain" },
            { id: "vim", label: "Vim" },
          ]}
          onChange={onToggleVim}
        />
      </Row>
      <Row label="Theme">
        <Segment
          label="Color theme"
          value={theme}
          options={[
            { id: "dark", label: "Dark" },
            { id: "light", label: "Light" },
          ]}
          onChange={onToggleTheme}
        />
      </Row>
    </aside>
  );
}

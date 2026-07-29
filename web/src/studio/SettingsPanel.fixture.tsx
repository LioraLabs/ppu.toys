import { useState } from "react";
import { SettingsPanel } from "./SettingsPanel";
import { OverlayStage } from "../cosmos/FixtureStage";
import type { Theme } from "./theme";
import "./studio.css";

// Presentational flyout: local state stands in for the theme/editor stores the
// wired wrapper supplies, so both segment controls are interactive in isolation.
function Interactive({ vim = false, theme: t0 = "dark" }: { vim?: boolean; theme?: Theme }) {
  const [vimMode, setVimMode] = useState(vim);
  const [theme, setTheme] = useState<Theme>(t0);
  return (
    <OverlayStage>
      <SettingsPanel
        theme={theme}
        onToggleTheme={() => setTheme((v) => (v === "dark" ? "light" : "dark"))}
        vimMode={vimMode}
        onToggleVim={() => setVimMode((v) => !v)}
        onClose={() => {}}
      />
    </OverlayStage>
  );
}

const Default = () => <Interactive />;
const VimOn = () => <Interactive vim />;

export default {
  Default,
  VimOn,
};

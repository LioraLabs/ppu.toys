import { useState } from "react";
import type { ReactNode } from "react";
import { Toolbar } from "./Toolbar";
import { SettingsPanel } from "./SettingsPanel";
import { transport } from "./transport/transport";
import { useTheme } from "./theme";
import { editorSettings, useVimMode } from "./editor/editorSettings";
import { useSession, sessionStore } from "../api/session";
import { WorkspaceActions } from "./cloud/WorkspaceActions";
import { openSketchStore } from "./sketches/openSketch";

export interface ToolbarWiredProps {
  /** Open-sketch name (from the sketch store via Studio). */
  sketchName?: string;
  /** Unsaved-changes marker (from the sketch store via Studio). */
  dirty?: boolean;
  /** Layout controls (LayoutMenu over the dock api) — a slot because the api
   *  only exists once the dock has mounted. */
  layoutSlot?: ReactNode;
}

/** Wired container: reads the theme store and drives the transport, injecting
 *  the wired AddSourceButton / WorkspaceActions as the presentational toolbar's
 *  slots. Render-identical to the pre-split Toolbar. */
export function ToolbarWired({ sketchName, dirty, layoutSlot }: ToolbarWiredProps) {
  const { theme, toggleTheme } = useTheme();
  const vimMode = useVimMode();
  const [settingsOpen, setSettingsOpen] = useState(false);
  // session refresh is owned by WorkspaceActions (the studio's session seam);
  // this just mirrors the resolved user into the account menu.
  const { user } = useSession();
  return (
    <>
      <Toolbar
        sketchName={sketchName}
        dirty={dirty}
        theme={theme}
        user={user && { id: user.id, handle: user.handle, avatar: user.avatar }}
        onRun={() => transport.restart()}
        onToggleTheme={toggleTheme}
        onSignOut={() => void sessionStore.signOut()}
        onNewToy={() => void openSketchStore.newSketch()}
        onToggleSettings={() => setSettingsOpen((o) => !o)}
        layoutSlot={layoutSlot}
        workspaceSlot={<WorkspaceActions />}
      />
      {settingsOpen && (
        <SettingsPanel
          theme={theme}
          onToggleTheme={toggleTheme}
          vimMode={vimMode}
          onToggleVim={editorSettings.toggleVim}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </>
  );
}

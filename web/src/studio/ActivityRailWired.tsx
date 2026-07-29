import { useState } from "react";
import { ActivityRail, type RailItemId } from "./ActivityRail";
import { AssetsPanel } from "./sources/AssetsPanel";
import { SettingsPanel } from "./SettingsPanel";
import { inspectorStore, useInspectorView } from "./inspector/inspectorStore";
import { editorSettings, useVimMode } from "./editor/editorSettings";
import { useTheme } from "./theme";

/** Which rail item the current inspector view corresponds to (layers/sprites
 *  are shortcuts into inspector dock tabs, so they highlight from the same
 *  store they drive — only while the dock is actually visible). */
function railActive(view: ReturnType<typeof useInspectorView>): RailItemId | undefined {
  if (!view.dockOpen) return undefined;
  if (view.tab === "memory") return "layers";
  if (view.tab === "sprites") return "sprites";
  return undefined;
}

/** Wired container: owns the Assets/Settings flyout state and routes the
 *  view-shortcut items (layers/palette/sprites) into the inspector store. */
export function ActivityRailWired() {
  const [open, setOpen] = useState<"assets" | "settings" | null>(null);
  const view = useInspectorView();
  const vimMode = useVimMode();
  const { theme, toggleTheme } = useTheme();

  const select = (id: RailItemId) => {
    switch (id) {
      case "assets":
      case "settings":
        setOpen((v) => (v === id ? null : id));
        break;
      case "layers":
        // Memory tab hosts VRAM/CGRAM + the layer stack; re-click hides the dock.
        if (view.dockOpen && view.tab === "memory") inspectorStore.toggleDock();
        else inspectorStore.setTab("memory");
        break;
      case "sprites":
        if (view.dockOpen && view.tab === "sprites") inspectorStore.toggleDock();
        else inspectorStore.setTab("sprites");
        break;
    }
  };

  return (
    <>
      <ActivityRail
        active={railActive(view)}
        assetsOpen={open === "assets"}
        settingsOpen={open === "settings"}
        onSelect={select}
      />
      {open === "assets" && <AssetsPanel onClose={() => setOpen(null)} />}
      {open === "settings" && (
        <SettingsPanel
          theme={theme}
          onToggleTheme={toggleTheme}
          vimMode={vimMode}
          onToggleVim={editorSettings.toggleVim}
          onClose={() => setOpen(null)}
        />
      )}
    </>
  );
}

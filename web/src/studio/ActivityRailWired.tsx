import { useState } from "react";
import { ActivityRail, type RailItemId } from "./ActivityRail";
import { LibraryPanel } from "./sketches/LibraryPanel";
import { SettingsPanel } from "./SettingsPanel";
import { inspectorStore, useInspectorView } from "./inspector/inspectorStore";
import { editorSettings, useVimMode } from "./editor/editorSettings";
import { useTheme } from "./theme";

/** Which rail item the current inspector view corresponds to (layers/palette/
 *  sprites are shortcuts into inspector views, so they highlight from the same
 *  store they drive). */
function railActive(view: ReturnType<typeof useInspectorView>): RailItemId | undefined {
  if (view.overlay === "memory-layers") return "layers";
  if (view.overlay) return undefined;
  if (view.tab === "memory") return "palette";
  if (view.tab === "sprites") return "sprites";
  return undefined;
}

/** Wired container: owns the Files/Settings flyout state and routes the
 *  view-shortcut items (layers/palette/sprites) into the inspector store. */
export function ActivityRailWired() {
  const [open, setOpen] = useState<"files" | "settings" | null>(null);
  const view = useInspectorView();
  const vimMode = useVimMode();
  const { theme, toggleTheme } = useTheme();

  const select = (id: RailItemId) => {
    switch (id) {
      case "files":
      case "settings":
        setOpen((v) => (v === id ? null : id));
        break;
      case "layers":
        // The rail's "Memory & layers" is the full overlay view; clicking it
        // again collapses back to the tabs.
        if (view.overlay === "memory-layers") inspectorStore.collapse();
        else inspectorStore.openOverlay("memory-layers");
        break;
      case "palette":
        inspectorStore.setTab("memory"); // CGRAM ownership leads the Memory tab
        break;
      case "sprites":
        inspectorStore.setTab("sprites");
        break;
    }
  };

  return (
    <>
      <ActivityRail
        active={railActive(view)}
        filesOpen={open === "files"}
        settingsOpen={open === "settings"}
        onSelect={select}
      />
      {open === "files" && <LibraryPanel onClose={() => setOpen(null)} />}
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

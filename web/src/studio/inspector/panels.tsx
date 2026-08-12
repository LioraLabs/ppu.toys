import type { ReactNode } from "react";
import { useInspectorFrame } from "./useInspectorFrame";
import type { TabId } from "./tabs";
import { TraceTab } from "./TraceTab";
import { MemoryTabWired } from "./MemoryTabWired";
import { ComposeTabWired } from "./ComposeTabWired";
import { WindowsTabWired } from "./WindowsTabWired";
import { Mode7PanelWired } from "./Mode7PanelWired";
import { RegistersTab } from "./RegistersTab";
import { SpritesTab } from "./SpritesTab";
import { VramTabWired } from "./VramTabWired";
import "./inspector.css";

/** The former Inspector tab bodies as free-standing dock-panel contents. The
 *  frame-consuming pages read the shared transport via useInspectorFrame (or a
 *  fixture's InspectorFrameProvider). The Inspector chrome itself is gone —
 *  a dockview tab group IS the tab view now. */
function RegistersPanel() {
  return <RegistersTab frame={useInspectorFrame()} />;
}
function SpritesPanel() {
  return <SpritesTab frame={useInspectorFrame()} />;
}
function VramPanel() {
  return <VramTabWired frame={useInspectorFrame()} />;
}

export const WIRED_INSPECTOR_PANELS: Record<TabId, () => ReactNode> = {
  trace: () => <TraceTab />,
  memory: () => <MemoryTabWired />,
  compose: () => <ComposeTabWired />,
  windows: () => <WindowsTabWired />,
  m7: () => <Mode7PanelWired />,
  registers: () => <RegistersPanel />,
  sprites: () => <SpritesPanel />,
  vram: () => <VramPanel />,
};

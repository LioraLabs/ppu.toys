import { useState } from "react";
import { useInspectorFrame } from "./useInspectorFrame";
import { INSPECTOR_TABS, overlayForTab, type OverlayId, type TabId } from "./tabs";
import { TraceTab } from "./TraceTab";
import { MemoryTabWired } from "./MemoryTabWired";
import { ComposeTabWired } from "./ComposeTabWired";
import { WindowsTab } from "./WindowsTab";
import { MemoryLayersOverlay } from "./MemoryLayersOverlay";
import { CompositorOverlayWired } from "./CompositorOverlayWired";
import { RegistersTab } from "./RegistersTab";
import { SpritesTab } from "./SpritesTab";
import { VramTabWired } from "./VramTabWired";
import "./inspector.css";

export function Inspector() {
  const [tab, setTab] = useState<TabId>("trace");
  const [overlay, setOverlay] = useState<OverlayId | null>(null);
  const frame = useInspectorFrame();
  const expandTarget = overlayForTab(tab);
  return (
    <div className="inspector">
      <div className="insp-tabs">
        {INSPECTOR_TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={"insp-tab" + (tab === t.id ? " insp-tab--active" : "")}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
        <div className="tb-spacer" />
        {expandTarget && (
          <button
            type="button"
            className="btn-ghost insp-expand"
            onClick={() => setOverlay(expandTarget)}
          >
            ⤢ Expand
          </button>
        )}
      </div>
      {tab === "trace" && <TraceTab />}
      {tab === "memory" && <MemoryTabWired />}
      {tab === "compose" && <ComposeTabWired />}
      {tab === "windows" && <WindowsTab />}
      {tab === "registers" && <RegistersTab frame={frame} />}
      {tab === "sprites" && <SpritesTab frame={frame} />}
      {tab === "vram" && <VramTabWired frame={frame} />}
      {overlay === "memory-layers" && <MemoryLayersOverlay onCollapse={() => setOverlay(null)} />}
      {overlay === "compositor" && <CompositorOverlayWired onCollapse={() => setOverlay(null)} />}
    </div>
  );
}

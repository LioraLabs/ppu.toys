import { useState, type ReactNode } from "react";
import type { FrameResult } from "../../ppu/core";
import { useInspectorFrame } from "./useInspectorFrame";
import { INSPECTOR_TABS, overlayForTab, type OverlayId, type TabId } from "./tabs";
import { TraceTab } from "./TraceTab";
import { MemoryTabWired } from "./MemoryTabWired";
import { ComposeTabWired } from "./ComposeTabWired";
import { WindowsTab } from "./WindowsTab";
import { MemoryLayersOverlayWired } from "./MemoryLayersOverlayWired";
import { CompositorOverlayWired } from "./CompositorOverlayWired";
import { RegistersTab } from "./RegistersTab";
import { SpritesTab } from "./SpritesTab";
import { VramTabWired } from "./VramTabWired";
import "./inspector.css";

export interface InspectorProps {
  /** Body renderer for the active tab. Defaults to the live wired set (which
   *  reads the shared ppuCore for memory/vram/compose) — a fixture overrides it
   *  with fixture-fed presentational tabs so the inspector chrome + tab
   *  switching render wasm-free (see StudioLayout.fixture). */
  renderTab?: (tab: TabId, frame: FrameResult) => ReactNode;
  /** Same seam for the ⤢ Expand overlays (both wired defaults are core-bound). */
  renderOverlay?: (overlay: OverlayId, frame: FrameResult, onCollapse: () => void) => ReactNode;
}

function wiredTab(tab: TabId, frame: FrameResult): ReactNode {
  switch (tab) {
    case "trace":
      return <TraceTab />;
    case "memory":
      return <MemoryTabWired />;
    case "compose":
      return <ComposeTabWired />;
    case "windows":
      return <WindowsTab />;
    case "registers":
      return <RegistersTab frame={frame} />;
    case "sprites":
      return <SpritesTab frame={frame} />;
    case "vram":
      return <VramTabWired frame={frame} />;
  }
}

function wiredOverlay(overlay: OverlayId, _frame: FrameResult, onCollapse: () => void): ReactNode {
  return overlay === "memory-layers" ? (
    <MemoryLayersOverlayWired onCollapse={onCollapse} />
  ) : (
    <CompositorOverlayWired onCollapse={onCollapse} />
  );
}

export function Inspector({ renderTab = wiredTab, renderOverlay = wiredOverlay }: InspectorProps = {}) {
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
      {renderTab(tab, frame)}
      {overlay && renderOverlay(overlay, frame, () => setOverlay(null))}
    </div>
  );
}

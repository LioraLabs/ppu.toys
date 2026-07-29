import { type ReactNode } from "react";
import type { FrameResult } from "../../ppu/core";
import { useInspectorFrame } from "./useInspectorFrame";
import { inspectorStore, useInspectorView } from "./inspectorStore";
import { INSPECTOR_TABS, type TabId } from "./tabs";
import { TraceTab } from "./TraceTab";
import { MemoryTabWired } from "./MemoryTabWired";
import { ComposeTabWired } from "./ComposeTabWired";
import { WindowsTab } from "./WindowsTab";
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

export function Inspector({ renderTab = wiredTab }: InspectorProps = {}) {
  // Shared store, not local state: the ActivityRail drives these too.
  const { tab } = useInspectorView();
  const setTab = inspectorStore.setTab;
  const frame = useInspectorFrame();
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
      </div>
      {renderTab(tab, frame)}
    </div>
  );
}

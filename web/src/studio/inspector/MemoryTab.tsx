import type { FrameResult } from "../../ppu/core";
import { bgMode } from "./format";
import { useCopyToast } from "./copyToast";
import { cgramOwners, vramRegions } from "./tracemem/regions";
import { CgramGrid, VramBar, VramLegend } from "./tracemem/MemoryPanels";
import { LayersPanel } from "./tracemem/LayersPanel";
import "./tracemem/tracemem.css";

/** MEMORY — VRAM regions + CGRAM ownership, derived from the LIVE binding
 *  registers each frame (M9 deviation: never the handoff's hardcoded table);
 *  the layer priority stack + visibility toggles ride alongside (rehomed here
 *  when the Memory & Layers Expand overlay was retired). */
export function MemoryTab({ frame, vram }: { frame: FrameResult; vram: Uint16Array }) {
  const { toast, copy } = useCopyToast();
  const regions = vramRegions(frame.registers, vram);
  const owners = cgramOwners(frame.registers, vram, frame.oam);
  return (
    <div className="insp-scroll">
      <div className="mem-cols">
        <div className="mem-col-layers">
          <div className="insp-subhead">LAYERS</div>
          <LayersPanel mode={bgMode(frame.registers)} />
        </div>
        <div className="mem-col-main">
          <div className="insp-subhead">VRAM · 32,768 WORDS</div>
          <VramBar regions={regions} onCopy={copy} />
          <VramLegend regions={regions} onCopy={copy} />
          <div className="insp-subhead">CGRAM OWNERSHIP · 16 × 16</div>
          <CgramGrid cgram={frame.cgram} owners={owners} />
        </div>
      </div>
      {toast}
    </div>
  );
}

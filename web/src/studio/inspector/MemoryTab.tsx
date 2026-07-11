import { ppuCore } from "../../ppu/instance";
import { useInspectorFrame } from "./useInspectorFrame";
import { useCopyToast } from "./copyToast";
import { cgramOwners, vramRegions } from "./tracemem/regions";
import { CgramGrid, VramBar, VramLegend } from "./tracemem/MemoryPanels";
import "./tracemem/tracemem.css";

/** MEMORY — VRAM regions + CGRAM ownership, derived from the LIVE binding
 *  registers each frame (M9 deviation: never the handoff's hardcoded table). */
export function MemoryTab() {
  const frame = useInspectorFrame();
  const { toast, copy } = useCopyToast();
  const vram = ppuCore.vram();
  const regions = vramRegions(frame.registers, vram);
  const owners = cgramOwners(frame.registers, vram, frame.oam);
  return (
    <div className="insp-scroll">
      <div className="insp-subhead">VRAM · 32,768 WORDS</div>
      <VramBar regions={regions} onCopy={copy} />
      <VramLegend regions={regions} onCopy={copy} />
      <div className="insp-subhead">CGRAM OWNERSHIP · 16 × 16</div>
      <CgramGrid cgram={frame.cgram} owners={owners} />
      {toast}
    </div>
  );
}

import { useInspectorFrame } from "./useInspectorFrame";
import { ppuCore } from "../../ppu/instance";
import { TraceChain } from "./tracemem/TraceChain";
import { MemoryLayersOverlay } from "./MemoryLayersOverlay";

/** Wired container: live frame (seam) + live VRAM/import reports off the shared
 *  core, with the rasterizer-bound resolution chain injected. */
export function MemoryLayersOverlayWired({ onCollapse }: { onCollapse: () => void }) {
  const frame = useInspectorFrame();
  return (
    <MemoryLayersOverlay
      onCollapse={onCollapse}
      frame={frame}
      vram={ppuCore.vram()}
      reports={ppuCore.importReports()}
      chain={(copy) => <TraceChain frame={frame} copy={copy} variant="overlay" />}
    />
  );
}

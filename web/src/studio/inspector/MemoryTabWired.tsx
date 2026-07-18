import { ppuCore } from "../../ppu/instance";
import { useInspectorFrame } from "./useInspectorFrame";
import { MemoryTab } from "./MemoryTab";

/** Wired container: the live frame (via the seam) + live VRAM off the shared core. */
export function MemoryTabWired() {
  const frame = useInspectorFrame();
  return <MemoryTab frame={frame} vram={ppuCore.vram()} />;
}

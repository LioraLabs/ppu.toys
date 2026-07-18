import type { FrameResult } from "../../ppu/core";
import { ppuCore } from "../../ppu/instance";
import { VramTab } from "./VramTab";

/** Wired container: reads the live VRAM + import reports off the shared core and
 *  hands them to the presentational VramTab. */
export function VramTabWired({ frame }: { frame: FrameResult | null }) {
  return <VramTab frame={frame} vram={ppuCore.vram()} reports={ppuCore.importReports()} />;
}

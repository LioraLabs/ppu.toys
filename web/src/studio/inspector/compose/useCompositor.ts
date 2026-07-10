import type { FrameResult } from "../../../ppu/core";
import { useInspectorFrame } from "../useInspectorFrame";
import { effectiveReg, type ReadReg, type RegWrite } from "./model";

/** Everything the Compose/Windows sections render from.
 *  ppu-61: the pin seam is gone; `pins` is a stub empty array and `read` falls
 *  back to the live register value only until Task 7 rewires this onto the
 *  generated pokes.lua writer. */
/* ppu-61: replaced in Task 7 */
export interface Compositor {
  frame: FrameResult;
  pins: RegWrite[];
  /** Effective value: pin > live register > power-on default. */
  read: ReadReg;
  isPinned: (addr: number) => boolean;
}

export function useCompositor(): Compositor {
  const frame = useInspectorFrame();
  const pins: RegWrite[] = []; /* ppu-61: replaced in Task 7 */
  return {
    frame,
    pins,
    read: (addr) => effectiveReg(frame.registers, pins, addr).value,
    isPinned: () => false /* ppu-61: replaced in Task 7 */,
  };
}

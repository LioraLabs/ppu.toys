import type { FrameResult, PinnedRegister } from "../../../ppu/core";
import { useInspectorFrame } from "../useInspectorFrame";
import { effectiveReg, type ReadReg } from "./model";
import { usePins } from "./pinStore";

/** Everything the Compose/Windows sections render from. ALL state lives in
 *  the core (frame registers + pinned overrides) — the docked tabs and the
 *  Compositor overlay both read through this hook and write through the pin
 *  store, so editing in one place updates the other by construction. */
export interface Compositor {
  frame: FrameResult;
  pins: PinnedRegister[];
  /** Effective value: pin > live register > power-on default. */
  read: ReadReg;
  isPinned: (addr: number) => boolean;
}

export function useCompositor(): Compositor {
  const frame = useInspectorFrame();
  const pins = usePins();
  return {
    frame,
    pins,
    read: (addr) => effectiveReg(frame.registers, pins, addr).value,
    isPinned: (addr) => pins.some((p) => p.addr === addr),
  };
}

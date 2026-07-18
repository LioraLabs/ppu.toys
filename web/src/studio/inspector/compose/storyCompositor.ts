import type { FrameResult } from "../../../ppu/core";
import { liveReg } from "./model";
import type { Compositor } from "./useCompositor";

/** Build the Compositor shape from a fixture frame for stories/tests — reads
 *  registers via the pure liveReg (no ppuCore), writes are inert no-ops, and
 *  nothing is poked. Lets compose/window panels render wasm-free. */
export function makeFixtureCompositor(frame: FrameResult): Compositor {
  return {
    frame,
    read: (addr) => liveReg(frame.registers, addr),
    write: () => {},
    writeMany: () => {},
    pokedAt: () => [],
  };
}

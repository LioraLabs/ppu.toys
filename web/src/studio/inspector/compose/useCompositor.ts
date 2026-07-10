import type { FrameResult } from "../../../ppu/core";
import { POKES_FILE, type Poke } from "../../pokes/pokes";
import { hasApplyCall, poke, pokeMany, usePokes } from "../../pokes/pokeStore";
import { openContextFiles, useOpenSketch } from "../../sketches/openSketch";
import { useInspectorFrame } from "../useInspectorFrame";
import { REG_LVALUES, liveReg, regPoke, type ReadReg, type RegWrite } from "./model";

/** Everything the Compose/Windows sections render from — shared by the docked
 *  tabs and the Compositor overlay. Controls READ the live register value
 *  (the script wins: apply_pokes() runs at the top of frame()) and WRITE
 *  whole-register pokes into the generated pokes.lua. */
export interface Compositor {
  frame: FrameResult;
  pokes: Poke[];
  /** Live register value, else power-on default. */
  read: ReadReg;
  /** Upsert one whole-register poke. */
  write: (addr: number, value: number) => void;
  /** Upsert a batch of register pokes in ONE pokes.lua regeneration. */
  writeMany: (writes: readonly RegWrite[]) => void;
  /** The poke targeting `addr`, if any. */
  pokedAt: (addr: number) => Poke | undefined;
  /** Something outside pokes.lua calls apply_pokes(). */
  pokesApplied: boolean;
  /** Verbatim pokes.lua source — the PokeBar copy-function chip. */
  pokesSource: string;
}

export function useCompositor(): Compositor {
  const frame = useInspectorFrame();
  const pokes = usePokes();
  const files = openContextFiles(useOpenSketch());
  return {
    frame,
    pokes,
    read: (addr) => liveReg(frame.registers, addr),
    write: (addr, value) => poke(regPoke(addr, value)),
    writeMany: (writes) => pokeMany(writes.map((w) => regPoke(w.addr, w.value))),
    pokedAt: (addr) => pokes.find((p) => p.lvalue === REG_LVALUES[addr]),
    pokesApplied: hasApplyCall(files),
    pokesSource: files.find((f) => f.name === POKES_FILE)?.source ?? "",
  };
}

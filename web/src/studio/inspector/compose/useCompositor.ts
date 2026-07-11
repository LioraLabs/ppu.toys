import type { FrameResult } from "../../../ppu/core";
import type { Poke } from "../../pokes/pokes";
import { pokeMany, usePokes } from "../../pokes/pokeStore";
import { useInspectorFrame } from "../useInspectorFrame";
import { liveReg, pokesAt, writesToPokes, type FieldWrite, type ReadReg } from "./model";
import { pokeDialect } from "./dialect";

/** The compositor write path: project field writes through the persisted
 *  dialect setting and upsert in ONE pokes.lua regeneration (cross-dialect
 *  eviction happens inside pokeMany). Plain function so the wiring tests
 *  drive it without rendering the hook. */
export function compositorWrite(writes: readonly FieldWrite[]): void {
  pokeMany(writesToPokes(writes, pokeDialect.get()));
}

/** Everything the Compose/Windows sections render from — shared by the docked
 *  tabs and the Compositor overlay. Controls READ the live register value
 *  (the script wins: apply_pokes() runs at the top of frame()) and WRITE
 *  friendly field pokes into the generated pokes.lua — one line per touched
 *  control, each overriding only its own bits (raw whole-register pokes when
 *  the persisted dialect setting says so). */
export interface Compositor {
  frame: FrameResult;
  /** Live register value, else power-on default. */
  read: ReadReg;
  /** Upsert one control action's poke. */
  write: (w: FieldWrite) => void;
  /** Upsert a batch of field writes in ONE pokes.lua regeneration. */
  writeMany: (writes: readonly FieldWrite[]) => void;
  /** Pokes targeting a control: raw poke on `addr` plus the listed fields
   *  (or, without a list, every field living in the register). */
  pokedAt: (addr: number, fields?: readonly string[]) => Poke[];
}

export function useCompositor(): Compositor {
  const frame = useInspectorFrame();
  const pokes = usePokes();
  return {
    frame,
    read: (addr) => liveReg(frame.registers, addr),
    write: (w) => compositorWrite([w]),
    writeMany: compositorWrite,
    pokedAt: (addr, fields) => pokesAt(pokes, addr, fields),
  };
}

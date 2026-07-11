import type { FrameResult } from "../../../ppu/core";
import { POKES_FILE, type Poke } from "../../pokes/pokes";
import { hasApplyCall, pokeMany, usePokes } from "../../pokes/pokeStore";
import { openContextFiles, useOpenSketch } from "../../sketches/openSketch";
import { useInspectorFrame } from "../useInspectorFrame";
import { liveReg, pokesAt, writesToPokes, type FieldWrite, type PokeDialect, type ReadReg } from "./model";

/** Poke dialect the controls emit. The upcoming raw/friendly toggle replaces
 *  this constant with a user-facing setting — writesToPokes is the projection
 *  point, nothing else changes. */
const DIALECT: PokeDialect = "friendly";

/** Everything the Compose/Windows sections render from — shared by the docked
 *  tabs and the Compositor overlay. Controls READ the live register value
 *  (the script wins: apply_pokes() runs at the top of frame()) and WRITE
 *  friendly field pokes into the generated pokes.lua — one line per touched
 *  control, each overriding only its own bits (raw whole-register pokes when
 *  the dialect says so). */
export interface Compositor {
  frame: FrameResult;
  pokes: Poke[];
  /** Live register value, else power-on default. */
  read: ReadReg;
  /** Upsert one control action's poke. */
  write: (w: FieldWrite) => void;
  /** Upsert a batch of field writes in ONE pokes.lua regeneration. */
  writeMany: (writes: readonly FieldWrite[]) => void;
  /** Pokes targeting a control: raw poke on `addr` plus the listed fields
   *  (or, without a list, every field living in the register). */
  pokedAt: (addr: number, fields?: readonly string[]) => Poke[];
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
    write: (w) => pokeMany(writesToPokes([w], DIALECT)),
    writeMany: (writes) => pokeMany(writesToPokes(writes, DIALECT)),
    pokedAt: (addr, fields) => pokesAt(pokes, addr, fields),
    pokesApplied: hasApplyCall(files),
    pokesSource: files.find((f) => f.name === POKES_FILE)?.source ?? "",
  };
}

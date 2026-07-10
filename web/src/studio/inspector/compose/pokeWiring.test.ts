import { describe, it, expect, beforeEach } from "vitest";
import "fake-indexeddb/auto";
import type { RegisterView } from "../../../ppu/core";
import { openSketchStore, openContextFiles } from "../../sketches/openSketch";
import { POKES_FILE } from "../../pokes/pokes";
import { currentPokes, poke, pokeMany } from "../../pokes/pokeStore";
import { REG, liveReg, pokeMatchesLive, regPoke, toggleMaskBit } from "./model";

/** Logic-level wiring tests: the compose/windows control handlers are
 *  (decode via liveReg) -> (encode via the model helpers) -> poke(regPoke(...)).
 *  These drive that pipeline against the real stores, no DOM. */

const rv = (addr: number, name: string, value: number): RegisterView => ({
  addr,
  name,
  value,
  changed: false,
});

function pokesSource(): string {
  return openContextFiles(openSketchStore.state()).find((f) => f.name === POKES_FILE)!.source;
}

describe("poke wiring", () => {
  beforeEach(() => openSketchStore.newSketch());

  it("a matrix-cell toggle lands the right TM line in pokes.lua", () => {
    // the handler: read live TM (power-on fallback), flip one bit, poke whole reg
    const tm = liveReg([], REG.TM); // mock core omits TM -> 0x1f
    const w = toggleMaskBit(REG.TM, tm, 2);
    poke(regPoke(w.addr, w.value));
    expect(pokesSource()).toContain("  TM = 0x1b -- $212C");
    expect(currentPokes(openSketchStore.state())).toEqual([
      { lvalue: "TM", expr: "0x1b", note: "$212C" },
    ]);
  });

  it("a batch write produces every line in ONE editFile (one store emit)", () => {
    let emits = 0;
    const unsub = openSketchStore.subscribe(() => emits++);
    pokeMany([regPoke(REG.WH0, 40), regPoke(REG.WH1, 200)]);
    unsub();
    expect(emits).toBe(1);
    expect(pokesSource()).toContain("  WH0 = 0x28 -- $2126");
    expect(pokesSource()).toContain("  WH1 = 0xc8 -- $2127");
  });

  it("re-poking the same register replaces its line (drag = many writes, one line)", () => {
    poke(regPoke(REG.WH0, 40));
    poke(regPoke(REG.WH0, 41));
    expect(currentPokes(openSketchStore.state())).toEqual([
      { lvalue: "WH0", expr: "0x29", note: "$2126" },
    ]);
  });

  it("a poke on a DEMO context forks it; pokes.lua stays index 0", () => {
    openSketchStore._resetForTests(); // back to the boot demo
    expect(openSketchStore.state().context.kind).toBe("demo");
    poke(regPoke(REG.WH0, 40));
    const s = openSketchStore.state();
    expect(s.context.kind).toBe("sketch");
    expect(openContextFiles(s)[0].name).toBe(POKES_FILE);
    expect(currentPokes(s)).toEqual([{ lvalue: "WH0", expr: "0x28", note: "$2126" }]);
  });
});

describe("pokeMatchesLive (solid/hollow marker decision)", () => {
  it("true when the live register equals the poked value (solid)", () => {
    expect(pokeMatchesLive(regPoke(REG.TM, 0x13), [rv(REG.TM, "TM", 0x13)])).toBe(true);
  });

  it("true against the power-on default when the core omits the register", () => {
    expect(pokeMatchesLive(regPoke(REG.TM, 0x1f), [])).toBe(true);
  });

  it("false when a script write overrode the poke (hollow)", () => {
    expect(pokeMatchesLive(regPoke(REG.TM, 0x13), [rv(REG.TM, "TM", 0x1f)])).toBe(false);
  });

  it("null (non-comparable) for a non-numeric expr or an unmapped lvalue", () => {
    expect(pokeMatchesLive({ lvalue: "TM", expr: "0x10 | 0x03" }, [])).toBeNull();
    expect(pokeMatchesLive({ lvalue: "cgram[0x41]", expr: "0x13" }, [])).toBeNull();
  });
});

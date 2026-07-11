import { describe, it, expect, beforeEach, afterEach } from "vitest";
import "fake-indexeddb/auto";
import type { RegisterView } from "../../../ppu/core";
import { openSketchStore, openContextFiles } from "../../sketches/openSketch";
import { POKES_FILE } from "../../pokes/pokes";
import { currentPokes, poke, pokeMany } from "../../pokes/pokeStore";
import {
  REG,
  liveReg,
  pokeMatchesLive,
  fieldPoke,
  setWindowEdge,
  toggleDesignation,
  writesToPokes,
  regPoke,
  setMathOp,
} from "./model";
import { compositorWrite } from "./useCompositor";
import { pokeDialect } from "./dialect";

/** Logic-level wiring tests: the compose/windows control handlers are
 *  (decode via liveReg) -> (encode via the model emitters, returning
 *  FieldWrite(s)) -> (project via writesToPokes) -> poke/pokeMany.
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
  // reset both sides of each test: order-independent even under test shuffle
  beforeEach(() => pokeDialect.set("friendly"));
  afterEach(() => pokeDialect.set("friendly"));

  it("a matrix-cell toggle lands the friendly field line in pokes.lua", () => {
    // the handler: read live TM (power-on fallback), flip one bit, poke the friendly field
    const tm = liveReg([], REG.TM); // mock core omits TM -> 0x1f
    poke(fieldPoke(toggleDesignation("screen.main.bg3", REG.TM, tm, 2)));
    expect(pokesSource()).toContain("  screen.main.bg3 = false -- $212C");
    expect(currentPokes(openSketchStore.state())).toEqual([
      { lvalue: "screen.main.bg3", expr: "false", note: "$212C" },
    ]);
  });

  it("a batch write produces every friendly line in ONE editFile (one store emit)", () => {
    let emits = 0;
    const unsub = openSketchStore.subscribe(() => emits++);
    pokeMany(writesToPokes([setWindowEdge(REG.WH0, 40), setWindowEdge(REG.WH1, 200)], "friendly"));
    unsub();
    expect(emits).toBe(1);
    expect(pokesSource()).toContain("  win.w1.lo = 40 -- $2126");
    expect(pokesSource()).toContain("  win.w1.hi = 200 -- $2127");
  });

  it("re-poking the same field replaces its line (drag = many writes, one line)", () => {
    poke(fieldPoke(setWindowEdge(REG.WH0, 40)));
    poke(fieldPoke(setWindowEdge(REG.WH0, 41)));
    expect(currentPokes(openSketchStore.state())).toEqual([
      { lvalue: "win.w1.lo", expr: "41", note: "$2126" },
    ]);
  });

  it("the raw dialect still emits whole-register lines through the same projection", () => {
    pokeMany(writesToPokes([setWindowEdge(REG.WH0, 40)], "raw"));
    expect(pokesSource()).toContain("  WH0 = 0x28 -- $2126");
  });

  it("a poke on a DEMO context forks it; pokes.lua stays index 0", () => {
    openSketchStore._resetForTests(); // back to the boot demo
    expect(openSketchStore.state().context.kind).toBe("demo");
    poke(fieldPoke(setWindowEdge(REG.WH0, 40)));
    const s = openSketchStore.state();
    expect(s.context.kind).toBe("sketch");
    expect(openContextFiles(s)[0].name).toBe(POKES_FILE);
    expect(currentPokes(s)).toEqual([{ lvalue: "win.w1.lo", expr: "40", note: "$2126" }]);
  });

  it("a raw write evicts the friendly pokes on the same register, in ONE regeneration", () => {
    pokeMany(writesToPokes([setMathOp("add", 0x00)], "friendly"));
    expect(pokesSource()).toContain('  color.op = "add" -- $2131');
    let emits = 0;
    const unsub = openSketchStore.subscribe(() => emits++);
    pokeMany(writesToPokes([setMathOp("sub", 0x00)], "raw"));
    unsub();
    expect(emits).toBe(1);
    expect(pokesSource()).toContain("  CGADSUB = 0x80 -- $2131");
    expect(pokesSource()).not.toContain("color.op");
  });

  it("a friendly write evicts the raw poke on the same register; other registers survive", () => {
    poke(regPoke(REG.CGADSUB, 0x80));
    poke(regPoke(REG.TM, 0x13));
    pokeMany(writesToPokes([setMathOp("add", 0x80)], "friendly"));
    const lvalues = currentPokes(openSketchStore.state()).map((p) => p.lvalue);
    expect(lvalues).toEqual(["TM", "color.op"]);
  });

  it("the HexPoke path (poke + regPoke) evicts too — hex-editing a register wins over stale fields", () => {
    poke(fieldPoke(toggleDesignation("screen.main.bg3", REG.TM, 0x1f, 2)));
    poke(regPoke(REG.TM, 0x13)); // what HexPoke commits
    expect(currentPokes(openSketchStore.state())).toEqual([{ lvalue: "TM", expr: "0x13", note: "$212C" }]);
  });

  it("unmapped lvalues (cgram[...]) survive eviction on any register", () => {
    poke({ lvalue: "cgram[0x41]", expr: "0x7fff" });
    poke(regPoke(REG.CGADSUB, 0x80));
    // parsed back in file order — the codepoint sort puts uppercase mnemonics first
    expect(currentPokes(openSketchStore.state()).map((p) => p.lvalue)).toEqual(["CGADSUB", "cgram[0x41]"]);
  });

  it("the persisted setting picks the emission dialect: default friendly", () => {
    expect(pokeDialect.get()).toBe("friendly");
    compositorWrite([setMathOp("sub", 0x00)]);
    expect(pokesSource()).toContain('  color.op = "sub" -- $2131');
    expect(pokesSource()).not.toContain("CGADSUB =");
  });

  it("flipping the setting to raw flips emission to whole-register mnemonics", () => {
    pokeDialect.set("raw");
    compositorWrite([setMathOp("sub", 0x00)]);
    expect(pokesSource()).toContain("  CGADSUB = 0x80 -- $2131");
    expect(pokesSource()).not.toContain("color.op");
  });

  it("toggling mid-config: the raw rewrite evicts the friendly line for that register", () => {
    compositorWrite([setMathOp("add", 0x00)]); // friendly (default)
    pokeDialect.set("raw");
    compositorWrite([setMathOp("sub", 0x00)]);
    expect(currentPokes(openSketchStore.state())).toEqual([{ lvalue: "CGADSUB", expr: "0x80", note: "$2131" }]);
  });
});

describe("pokeMatchesLive (solid/hollow marker decision)", () => {
  it("true when the live register equals the poked value (solid)", () => {
    expect(pokeMatchesLive({ lvalue: "TM", expr: "0x13" }, [rv(REG.TM, "TM", 0x13)])).toBe(true);
  });

  it("true against the power-on default when the core omits the register", () => {
    expect(pokeMatchesLive({ lvalue: "TM", expr: "0x1f" }, [])).toBe(true);
  });

  it("false when a script write overrode the poke (hollow)", () => {
    expect(pokeMatchesLive({ lvalue: "TM", expr: "0x13" }, [rv(REG.TM, "TM", 0x1f)])).toBe(false);
  });

  it("null (non-comparable) for a non-numeric expr or an unmapped lvalue", () => {
    expect(pokeMatchesLive({ lvalue: "TM", expr: "0x10 | 0x03" }, [])).toBeNull();
    expect(pokeMatchesLive({ lvalue: "cgram[0x41]", expr: "0x13" }, [])).toBeNull();
  });

  it("friendly field poke: solid against the power-on default, hollow after a script override", () => {
    expect(pokeMatchesLive({ lvalue: "screen.main.bg3", expr: "true" }, [])).toBe(true); // TM=0x1f
    expect(pokeMatchesLive({ lvalue: "screen.main.bg3", expr: "true" }, [rv(REG.TM, "TM", 0x1b)])).toBe(false);
  });
});

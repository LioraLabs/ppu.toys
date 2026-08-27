import { describe, it, expect, beforeEach, afterEach } from "vitest";
import "fake-indexeddb/auto";
import type { RegisterView } from "../../../ppu/core";
import { openSketchStore, openContextFiles } from "../../sketches/openSketch";
import { POKES_FILE } from "../../pokes/pokes";
import {
  currentPokes,
  poke,
  pokeMany,
  removeKeyframeAt,
  setDialect,
  setKeyframeRange,
} from "../../pokes/pokeStore";
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
    const tm = liveReg([], REG.TM); // stub core omits TM -> 0x1f
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

  it("a poke updates the open toy; pokes.lua stays index 0", () => {
    openSketchStore._resetForTests();
    expect(openSketchStore.state().context.kind).toBe("sketch");
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
    expect(currentPokes(openSketchStore.state())).toEqual([
      { lvalue: "TM", expr: "0x13", note: "$212C" },
    ]);
  });

  it("unmapped lvalues (cgram[...]) survive eviction on any register", () => {
    poke({ lvalue: "cgram[0x41]", expr: "0x7fff" });
    poke(regPoke(REG.CGADSUB, 0x80));
    // parsed back in file order — the codepoint sort puts uppercase mnemonics first
    expect(currentPokes(openSketchStore.state()).map((p) => p.lvalue)).toEqual([
      "CGADSUB",
      "cgram[0x41]",
    ]);
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
    expect(currentPokes(openSketchStore.state())).toEqual([
      { lvalue: "CGADSUB", expr: "0x80", note: "$2131" },
    ]);
  });
});

describe("scanline dialect wiring", () => {
  beforeEach(() => openSketchStore.newSketch());
  beforeEach(() => pokeDialect.set("scanline"));
  afterEach(() => pokeDialect.set("friendly"));

  it("a window-edge drag at scanline y writes a keyframe on that line", () => {
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    expect(currentPokes(openSketchStore.state())).toEqual([
      // parsed back off the hook header, which always states its range
      { lvalue: "win.w1.lo", expr: "{{112,58}}", note: "$2126 · per-scanline", range: [0, 223] },
    ]);
    expect(pokesSource()).toContain("    win.w1.lo = pki({{112,58}}, y)");
  });

  it("dragging on another line ADDS a keyframe instead of replacing the poke", () => {
    compositorWrite([setWindowEdge(REG.WH0, 128)], 0);
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    expect(currentPokes(openSketchStore.state())[0].expr).toBe("{{0,128},{112,58}}");
  });

  it("re-dragging on the SAME line replaces that keyframe", () => {
    compositorWrite([setWindowEdge(REG.WH0, 128)], 112);
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    expect(currentPokes(openSketchStore.state())[0].expr).toBe("{{112,58}}");
  });

  it("falls back to a frame-wide friendly poke where no scanline is selected", () => {
    // Compose has no scanline context; a scanline-dialect write there must
    // still land something sensible rather than nothing.
    compositorWrite([setWindowEdge(REG.WH0, 58)]);
    expect(currentPokes(openSketchStore.state())).toEqual([
      { lvalue: "win.w1.lo", expr: "58", note: "$2126" },
    ]);
  });

  it("a non-numeric field has no curve, so it stays a frame-wide poke", () => {
    // TM power-on is 0x1f (all layers on), so toggling bit 0 clears BG1.
    compositorWrite([toggleDesignation("screen.main.bg1", REG.TM, 0x1f, 0)], 112);
    const p = currentPokes(openSketchStore.state())[0];
    expect(p.expr).toBe("false");
    expect(p.note).not.toContain("per-scanline");
  });

  it("evicts a raw poke on the same register, like any friendly write", () => {
    pokeMany([regPoke(REG.WH0, 40)]);
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    expect(currentPokes(openSketchStore.state()).map((p) => p.lvalue)).toEqual(["win.w1.lo"]);
  });

  it("setKeyframeRange scopes the poke's hook, and later edits keep that range", () => {
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    setKeyframeRange("win.w1.lo", 96, 223);
    expect(pokesSource()).toContain("  hdma(96, 223, function(y)");
    // a further drag must not silently reset the band back to the whole frame
    compositorWrite([setWindowEdge(REG.WH0, 60)], 150);
    const p = currentPokes(openSketchStore.state())[0];
    expect(p.range).toEqual([96, 223]);
    expect(p.expr).toBe("{{112,58},{150,60}}");
  });

  it("pokes on different ranges generate separate hooks", () => {
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    compositorWrite([setWindowEdge(REG.WH1, 198)], 112);
    setKeyframeRange("win.w1.lo", 96, 223);
    const src = pokesSource();
    expect(src.match(/hdma\(/g)).toHaveLength(2);
    expect(src).toContain("  hdma(0, 223, function(y)"); // win.w1.hi, unscoped
    expect(src).toContain("  hdma(96, 223, function(y)"); // win.w1.lo, scoped
  });

  it("removeKeyframeAt drops one line, and unpokes when it was the last", () => {
    compositorWrite([setWindowEdge(REG.WH0, 128)], 0);
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    removeKeyframeAt("win.w1.lo", 0);
    expect(currentPokes(openSketchStore.state())[0].expr).toBe("{{112,58}}");
    removeKeyframeAt("win.w1.lo", 112);
    expect(currentPokes(openSketchStore.state())).toEqual([]);
  });
});

describe("dialect conversion through setDialect", () => {
  beforeEach(() => openSketchStore.newSketch());
  afterEach(() => pokeDialect.set("friendly"));

  it("friendly -> scanline turns each field into a single held keyframe", () => {
    pokeDialect.set("friendly");
    compositorWrite([setWindowEdge(REG.WH0, 58)]);
    setDialect("scanline");
    const p = currentPokes(openSketchStore.state()).find((x) => x.lvalue === "win.w1.lo")!;
    // a single keyframe holds across the frame — same rendered result as the
    // frame-wide poke it came from
    expect(p.expr).toBe("{{0,58}}");
  });

  it("scanline -> friendly collapses the curve to its line-0 value", () => {
    pokeDialect.set("scanline");
    compositorWrite([setWindowEdge(REG.WH0, 128)], 0);
    compositorWrite([setWindowEdge(REG.WH0, 58)], 112);
    setDialect("friendly");
    const p = currentPokes(openSketchStore.state()).find((x) => x.lvalue === "win.w1.lo")!;
    expect(p.expr).toBe("128"); // the sweep is lost — documented, not silent
  });

  it("scanline -> raw folds the line-0 value into the whole register byte", () => {
    pokeDialect.set("scanline");
    compositorWrite([setWindowEdge(REG.WH0, 58)], 0);
    setDialect("raw");
    expect(currentPokes(openSketchStore.state())).toContainEqual({
      lvalue: "WH0",
      expr: "0x3a",
      note: "$2126",
    });
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

  it("null for a scanline poke — no single live value to match against", () => {
    // `registers` is scanline 0 only, so the marker must stay neutral rather
    // than claim a match or an override it cannot check.
    expect(pokeMatchesLive({ lvalue: "win.w1.lo", expr: "{{0,128},{112,58}}" }, [])).toBeNull();
  });

  it("friendly field poke: solid against the power-on default, hollow after a script override", () => {
    expect(pokeMatchesLive({ lvalue: "screen.main.bg3", expr: "true" }, [])).toBe(true); // TM=0x1f
    expect(
      pokeMatchesLive({ lvalue: "screen.main.bg3", expr: "true" }, [rv(REG.TM, "TM", 0x1b)]),
    ).toBe(false);
  });
});

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import "fake-indexeddb/auto";
import { openSketchStore, openContextFiles } from "../sketches/openSketch";
import { POKES_FILE } from "./pokes";
import { currentPokes, poke, pokeMany, unpoke, unpokeMany, clearPokes, hasApplyCall, setDialect } from "./pokeStore";
import { pokeDialect } from "../inspector/compose/dialect";

describe("pokeStore", () => {
  beforeEach(() => openSketchStore.newSketch());

  it("poke writes an lvalue into pokes.lua via editFile and parses back", () => {
    poke({ lvalue: "TM", expr: "0x13", note: "$212C" });
    const src = openContextFiles(openSketchStore.state()).find((f) => f.name === POKES_FILE)!.source;
    expect(src).toContain("  TM = 0x13 -- $212C");
    expect(currentPokes(openSketchStore.state())).toEqual([{ lvalue: "TM", expr: "0x13", note: "$212C" }]);
  });

  it("poking keeps pokes.lua at index 0 (reservation survives editFile)", () => {
    poke({ lvalue: "TM", expr: "0x13" });
    expect(openContextFiles(openSketchStore.state())[0].name).toBe(POKES_FILE);
  });

  it("re-poke replaces, unpoke removes, clearPokes empties", () => {
    poke({ lvalue: "TM", expr: "0x13" });
    poke({ lvalue: "TM", expr: "0x1f" });
    expect(currentPokes(openSketchStore.state())).toEqual([{ lvalue: "TM", expr: "0x1f" }]);
    poke({ lvalue: "WH0", expr: "40" });
    unpoke("TM");
    expect(currentPokes(openSketchStore.state())).toEqual([{ lvalue: "WH0", expr: "40" }]);
    clearPokes();
    expect(currentPokes(openSketchStore.state())).toEqual([]);
  });

  it("pokeMany writes a batch in one editFile (one file version)", () => {
    pokeMany([
      { lvalue: "WH0", expr: "40" },
      { lvalue: "WH1", expr: "200" },
    ]);
    expect(currentPokes(openSketchStore.state()).map((p) => p.lvalue)).toEqual(["WH0", "WH1"]);
  });

  it("unpokeMany removes several lvalues in one store write", () => {
    pokeMany([
      { lvalue: "win.bg1.w1", expr: "true" },
      { lvalue: "win.bg1.w2", expr: "true" },
      { lvalue: "color.op", expr: '"sub"' },
    ]);
    let emits = 0;
    const unsub = openSketchStore.subscribe(() => emits++);
    unpokeMany(["win.bg1.w1", "win.bg1.w2"]);
    unsub();
    expect(emits).toBe(1);
    expect(currentPokes(openSketchStore.state())).toEqual([{ lvalue: "color.op", expr: '"sub"' }]);
  });

  it("hasApplyCall finds the call outside pokes.lua only", () => {
    expect(hasApplyCall(openContextFiles(openSketchStore.state()))).toBe(true); // template calls it
    openSketchStore.editFile("main.lua", "function frame() end");
    expect(hasApplyCall(openContextFiles(openSketchStore.state()))).toBe(false);
  });

  describe("setDialect", () => {
    afterEach(() => pokeDialect.set("friendly"));

    it("setDialect('raw') rewrites existing pokes and persists the setting", () => {
      pokeMany([
        { lvalue: "color.op", expr: '"sub"' },
        { lvalue: "color.half", expr: "true" },
        { lvalue: "color.on.bg1", expr: "true" },
      ]);
      setDialect("raw");
      const after = currentPokes(openSketchStore.state()).map((p) => p.lvalue);
      expect(after).toEqual(["CGADSUB"]);
      expect(pokeDialect.get()).toBe("raw");
    });
  });
});

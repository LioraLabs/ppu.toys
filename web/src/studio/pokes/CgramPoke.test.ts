import { describe, it, expect, beforeEach } from "vitest";
import "fake-indexeddb/auto";
import { openSketchStore, openContextFiles } from "../sketches/openSketch";
import { POKES_FILE } from "./pokes";
import { poke } from "./pokeStore";
import { bgr555ToHex, cgramPoke, hexToBgr555 } from "./CgramPoke";

describe("hexToBgr555 / bgr555ToHex", () => {
  it.each([0x0000, 0x7fff, 0x1a3f, 0x03e0])("round-trips 15-bit value 0x%s", (v) => {
    expect(hexToBgr555(bgr555ToHex(v))).toBe(v);
  });

  it("quantizes #ffffff down to the max 15-bit word", () => {
    expect(hexToBgr555("#ffffff")).toBe(0x7fff);
  });
});

describe("cgramPoke", () => {
  it("builds the exact cgram[] poke object", () => {
    const note = bgr555ToHex(0x1a3f);
    expect(cgramPoke(0x41, 0x1a3f)).toEqual({
      lvalue: "cgram[0x41]",
      expr: "0x1a3f",
      note,
    });
  });
});

describe("cgram poke pipeline", () => {
  beforeEach(() => openSketchStore.newSketch());

  it("poking a picked color lands a cgram[] line in pokes.lua", () => {
    poke(cgramPoke(0x41, hexToBgr555("#52c4ff")));
    const src = openContextFiles(openSketchStore.state()).find((f) => f.name === POKES_FILE)!.source;
    expect(src).toMatch(/ {2}cgram\[0x41\] = 0x[0-9a-f]{4}/);
  });
});

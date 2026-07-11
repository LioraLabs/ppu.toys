import { describe, it, expect, beforeEach } from "vitest";
import "fake-indexeddb/auto";
import { openSketchStore, openContextFiles } from "../sketches/openSketch";
import { POKES_FILE } from "./pokes";
import { poke } from "./pokeStore";
import { regPoke } from "../inspector/compose/model";
import { parseHexPoke } from "./HexPoke";

describe("parseHexPoke", () => {
  it.each([
    ["1f", 0x1f],
    ["0x1f", 0x1f],
    ["$1F", 0x1f],
    ["  1f  ", 0x1f],
  ])("parses %s -> 0x%s", (raw, want) => {
    expect(parseHexPoke(0x212c, raw)).toBe(want);
  });

  it.each(["", "zz", "0x100"])("rejects %s for a one-byte register", (raw) => {
    expect(parseHexPoke(0x212c, raw)).toBeNull();
  });

  it("COLDATA ($2132) accepts up to the 15-bit max", () => {
    expect(parseHexPoke(0x2132, "7fff")).toBe(0x7fff);
  });

  it("COLDATA ($2132) rejects above the 15-bit max", () => {
    expect(parseHexPoke(0x2132, "8000")).toBeNull();
  });
});

describe("HexPoke store pipeline", () => {
  beforeEach(() => openSketchStore.newSketch());

  it("committing a parsed hex value pokes the whole register", () => {
    poke(regPoke(0x212c, parseHexPoke(0x212c, "1f")!));
    const src = openContextFiles(openSketchStore.state()).find((f) => f.name === POKES_FILE)!.source;
    expect(src).toContain("  TM = 0x1f -- $212C");
  });
});

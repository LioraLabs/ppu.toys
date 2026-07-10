import { describe, it, expect } from "vitest";
import { POKES_FILE, EMPTY_POKES, pokesToLua, parsePokes, upsertPoke, type Poke } from "./pokes";

describe("poke generator/parser", () => {
  it("POKES_FILE names the generated file", () => {
    expect(POKES_FILE).toBe("pokes.lua");
  });

  it("EMPTY_POKES defines an empty apply_pokes()", () => {
    expect(EMPTY_POKES).toContain("function apply_pokes()");
    expect(parsePokes(EMPTY_POKES)).toEqual([]);
  });

  it("round-trips a poke set (map -> lua -> map identity)", () => {
    const pokes: Poke[] = [
      { lvalue: "TM", expr: "0x13", note: "$212C main screen" },
      { lvalue: "cgram[0x41]", expr: "0x1a3f", note: "#52c4ff" },
      { lvalue: "WH0", expr: "40" },
    ];
    expect(parsePokes(pokesToLua(pokes))).toEqual(
      [...pokes].sort((a, b) => a.lvalue.localeCompare(b.lvalue)),
    );
  });

  it("generation is deterministic and stably sorted by lvalue", () => {
    const a: Poke = { lvalue: "TS", expr: "0x04" };
    const b: Poke = { lvalue: "TM", expr: "0x13" };
    expect(pokesToLua([a, b])).toBe(pokesToLua([b, a]));
  });

  it("upsertPoke replaces by lvalue and appends new keys", () => {
    const one = upsertPoke([], { lvalue: "TM", expr: "0x13" });
    const two = upsertPoke(one, { lvalue: "TM", expr: "0x1f" });
    expect(two).toEqual([{ lvalue: "TM", expr: "0x1f" }]);
  });

  it("parser drops unrecognized lines (machine ownership)", () => {
    const tampered = pokesToLua([{ lvalue: "TM", expr: "0x13" }]).replace(
      "function apply_pokes()",
      "function apply_pokes()\n  if weird then TM = 1 end",
    );
    expect(parsePokes(tampered)).toEqual([{ lvalue: "TM", expr: "0x13" }]);
  });
});

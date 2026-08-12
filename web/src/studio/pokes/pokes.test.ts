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
      [...pokes].sort((a, b) => (a.lvalue < b.lvalue ? -1 : 1)),
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

describe("scanline dialect", () => {
  const scan: Poke = {
    lvalue: "win.w1.lo",
    expr: "{{0,128},{112,58}}",
    note: "$2126 · per-scanline",
  };

  it("emits nothing extra when there are no scanline pokes", () => {
    // Every bundled demo ships a generated (empty) pokes.lua — that output must
    // stay byte-identical, so the hook and its helpers appear only when used.
    expect(EMPTY_POKES).not.toContain("hdma(");
    expect(EMPTY_POKES).not.toContain("local function pk");
    const flat = pokesToLua([{ lvalue: "TM", expr: "0x13" }]);
    expect(flat).not.toContain("hdma(");
    expect(flat).not.toContain("local function pk");
  });

  it("wraps scanline pokes in ONE hdma hook after the frame-wide lines", () => {
    const out = pokesToLua([{ lvalue: "TM", expr: "0x13" }, scan]);
    expect(out).toContain("  TM = 0x13\n");
    expect(out).toContain("  hdma(0, 223, function(y)\n");
    expect(out).toContain("    win.w1.lo = pki({{0,128},{112,58}}, y) -- $2126 · per-scanline\n");
    expect(out).toContain("  end)\n");
    expect(out.match(/hdma\(/g)).toHaveLength(1); // one hook, not one per poke
  });

  it("picks pkf for fractional fields and pki for integer ones", () => {
    expect(pokesToLua([{ lvalue: "m7.a", expr: "{{0,1},{223,2}}" }])).toContain("= pkf(");
    expect(pokesToLua([scan])).toContain("= pki(");
  });

  it("round-trips scanline pokes back out of the generated file", () => {
    // The hook header always states its range, so an unranged poke comes back
    // carrying the default explicitly — same meaning, now written down.
    expect(parsePokes(pokesToLua([scan]))).toEqual([{ ...scan, range: [0, 223] }]);
  });

  it("round-trips a MIXED set (both dialects in one file)", () => {
    const pokes: Poke[] = [
      { lvalue: "TM", expr: "0x13", note: "$212C" },
      scan,
      { lvalue: "win.w1.hi", expr: "{{0,128},{112,198}}" },
      { lvalue: "cgram[0x41]", expr: "0x1a3f" },
    ];
    const want = [...pokes]
      .sort((a, b) => (a.lvalue < b.lvalue ? -1 : 1))
      .map((p) => (p.expr.startsWith("{") ? { ...p, range: [0, 223] } : p));
    expect(parsePokes(pokesToLua(pokes))).toEqual(want);
  });

  it("stays deterministic regardless of input order", () => {
    const other: Poke = { lvalue: "win.w1.hi", expr: "{{0,200}}" };
    expect(pokesToLua([scan, other])).toBe(pokesToLua([other, scan]));
  });

  it("emits ONE hook per distinct range, in range order", () => {
    const out = pokesToLua([
      { lvalue: "win.w1.lo", expr: "{{96,58}}", range: [96, 223] },
      { lvalue: "m7.a", expr: "{{0,1},{95,2}}", range: [0, 95] },
      { lvalue: "win.w1.hi", expr: "{{96,198}}", range: [96, 223] },
    ]);
    expect(out.match(/hdma\(/g)).toHaveLength(2);
    // ordered by first line, and the two [96,223] pokes share one hook
    expect(out.indexOf("hdma(0, 95,")).toBeLessThan(out.indexOf("hdma(96, 223,"));
    const band = out.slice(out.indexOf("hdma(96, 223,"));
    expect(band).toContain("win.w1.hi = pki(");
    expect(band).toContain("win.w1.lo = pki(");
  });

  it("defaults an unranged scanline poke to the whole frame", () => {
    expect(pokesToLua([scan])).toContain("  hdma(0, 223, function(y)\n");
  });

  it("round-trips the range off the hook header", () => {
    const ranged: Poke = { ...scan, range: [96, 223] };
    expect(parsePokes(pokesToLua([ranged]))).toEqual([ranged]);
  });

  it("round-trips several ranges at once", () => {
    const pokes: Poke[] = [
      { lvalue: "m7.a", expr: "{{0,1},{95,2}}", range: [0, 95] },
      { lvalue: "win.w1.lo", expr: "{{96,58}}", range: [96, 223] },
      { lvalue: "win.w1.hi", expr: "{{96,198}}", range: [96, 223] },
    ];
    const back = parsePokes(pokesToLua(pokes));
    expect(back).toHaveLength(3);
    expect(back.find((p) => p.lvalue === "m7.a")!.range).toEqual([0, 95]);
    expect(back.find((p) => p.lvalue === "win.w1.lo")!.range).toEqual([96, 223]);
  });

  it("stays deterministic across input order with mixed ranges", () => {
    const a: Poke = { lvalue: "win.w1.lo", expr: "{{96,58}}", range: [96, 223] };
    const b: Poke = { lvalue: "m7.a", expr: "{{0,1}}", range: [0, 95] };
    expect(pokesToLua([a, b])).toBe(pokesToLua([b, a]));
  });

  it("a frame-wide poke never carries a range back out", () => {
    const back = parsePokes(pokesToLua([{ lvalue: "TM", expr: "0x13" }]));
    expect(back[0].range).toBeUndefined();
  });

  it("keeps the hook's own lines out of the parsed poke set", () => {
    // `  hdma(...` and `  end)` are 2-space lines inside apply_pokes(); neither
    // carries a ` = `, so the frame-wide line regex must not claim them — and
    // `  end)` must not terminate the block early either.
    const parsed = parsePokes(pokesToLua([{ lvalue: "TM", expr: "0x13" }, scan]));
    expect(parsed.map((p) => p.lvalue).sort()).toEqual(["TM", "win.w1.lo"]);
  });
});

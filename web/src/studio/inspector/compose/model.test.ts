import { describe, expect, it } from "vitest";
import type { RegisterView } from "../../../ppu/core";
import { parsePokes, pokesToLua } from "../../pokes/pokes";
import {
  ADDEND_FIELDS,
  BACKDROP_MATH_BIT,
  COMBINE_FIELDS,
  COMPOSE_LAYERS,
  AREA_FIELDS,
  FIELD_SPECS,
  FIXED_COLOR_SWATCHES,
  FIXED_FIELDS,
  MATH_ENABLE_FIELDS,
  OPERATION_FIELDS,
  REG,
  REG_LVALUES,
  SCREEN_MAIN_FIELDS,
  SCREEN_SUB_FIELDS,
  WINDOW_LAYERS,
  areaValue,
  columnMask,
  combineValue,
  dimOutsideMask,
  equation,
  fieldPoke,
  hexToBgr555,
  liveReg,
  mathAddend,
  mathHalf,
  mathOp,
  nearestEdgeAddr,
  pokeMatchesLive,
  pokesAt,
  regPoke,
  setArea,
  setCombine,
  setFixedColor,
  setMathAddend,
  setMathHalf,
  setMathOp,
  setWindowEdge,
  tintMathRegion,
  toggleDesignation,
  toggleWindowEnable,
  toggleWindowInvert,
  winRowFields,
  windowBounds,
  windowRow,
  withMathAddend,
  withMathHalf,
  withMathOp,
  writesToPokes,
  type FieldWrite,
  type ReadReg,
} from "./model";

/** ReadReg over a sparse addr->value table with the model's power-on defaults. */
const read =
  (vals: Record<number, number>): ReadReg =>
  (addr) =>
    vals[addr] ?? (addr === REG.TM ? 0x1f : 0);

const rv = (addr: number, name: string, value: number): RegisterView => ({
  addr,
  name,
  value,
  changed: false,
});

describe("liveReg", () => {
  it("reads the live core-reported value", () => {
    expect(liveReg([rv(REG.TM, "TM", 0x0b)], REG.TM)).toBe(0x0b);
  });

  it("falls back to power-on defaults when the core omits the register (mock)", () => {
    expect(liveReg([], REG.TM)).toBe(0x1f);
    expect(liveReg([], REG.TS)).toBe(0);
    expect(liveReg([], REG.CGADSUB)).toBe(0);
  });
});

describe("REG_LVALUES + regPoke", () => {
  it("covers every REG address the UI writes", () => {
    for (const addr of Object.values(REG)) {
      expect(REG_LVALUES[addr]).toBeDefined();
    }
  });

  it("regPoke emits the DSL assignment with a $-address note", () => {
    expect(regPoke(0x212c, 0x13)).toEqual({ lvalue: "TM", expr: "0x13", note: "$212C" });
    expect(regPoke(REG.WH0, 0)).toEqual({ lvalue: "WH0", expr: "0x00", note: "$2126" });
  });

  it("every mapped register round-trips through the pokes.lua generator", () => {
    for (const key of Object.keys(REG_LVALUES)) {
      const p = regPoke(Number(key), 0);
      expect(parsePokes(pokesToLua([p]))).toEqual([p]);
    }
  });

  it("throws on an unmapped address", () => {
    expect(() => regPoke(0x2105, 7)).toThrow(/no DSL lvalue/);
  });
});

describe("compose matrix + color math", () => {
  it("layer table matches the TM/TS/CGADSUB bit layout (OBJ is bit 4)", () => {
    expect(COMPOSE_LAYERS.map((l) => [l.id, l.bit])).toEqual([
      ["bg1", 0],
      ["bg2", 1],
      ["bg3", 2],
      ["obj", 4],
    ]);
    expect(BACKDROP_MATH_BIT).toBe(5);
  });

  it("decodes and re-encodes CGADSUB op/half without touching enable bits", () => {
    expect(mathOp(0x3f)).toBe("add");
    expect(mathOp(0xbf)).toBe("sub");
    expect(mathHalf(0x40)).toBe(true);
    expect(mathHalf(0x3f)).toBe(false);
    expect(withMathOp(0x3f, "sub")).toBe(0xbf);
    expect(withMathOp(0xbf, "add")).toBe(0x3f);
    expect(withMathHalf(0x3f, true)).toBe(0x7f);
    expect(withMathHalf(0x7f, false)).toBe(0x3f);
  });

  it("decodes and re-encodes the CGWSEL addend source without touching other bits", () => {
    expect(mathAddend(0x00)).toBe("fixed");
    expect(mathAddend(0x02)).toBe("sub");
    // Preserve prevent/clip nibbles + direct-color bit when flipping addend.
    expect(withMathAddend(0xf1, "sub")).toBe(0xf3);
    expect(withMathAddend(0xf3, "fixed")).toBe(0xf1);
  });

  it("equation chip covers all four op/half combos", () => {
    expect(equation("add", false)).toBe("out = ( main + sub )");
    expect(equation("add", true)).toBe("out = ( main + sub ) ÷ 2");
    expect(equation("sub", false)).toBe("out = ( main − sub )");
    expect(equation("sub", true)).toBe("out = ( main − sub ) ÷ 2");
  });

  it("hexToBgr555 packs the handoff swatches into COLDATA encoding", () => {
    expect(hexToBgr555("#000000")).toBe(0);
    expect(hexToBgr555("#ffffff")).toBe(0x7fff);
    expect(hexToBgr555("#0d2a3a")).toBe((7 << 10) | (5 << 5) | 1);
    expect(FIXED_COLOR_SWATCHES).toHaveLength(6);
  });
});

describe("compose field emitters", () => {
  it("toggleDesignation flips one bit and states the new value as a bool field", () => {
    expect(toggleDesignation("screen.main.bg2", REG.TM, 0x17, 1)).toEqual({
      field: "screen.main.bg2", expr: "false", addr: REG.TM, value: 0x15,
    });
    expect(toggleDesignation("color.on.backdrop", REG.CGADSUB, 0x00, 5)).toEqual({
      field: "color.on.backdrop", expr: "true", addr: REG.CGADSUB, value: 0x20,
    });
  });

  it("setMathOp / setMathHalf write CGADSUB preserving the enable bits", () => {
    expect(setMathOp("sub", 0x3f)).toEqual({ field: "color.op", expr: '"sub"', addr: REG.CGADSUB, value: 0xbf });
    expect(setMathOp("add", 0xbf)).toEqual({ field: "color.op", expr: '"add"', addr: REG.CGADSUB, value: 0x3f });
    expect(setMathHalf(true, 0x3f)).toEqual({ field: "color.half", expr: "true", addr: REG.CGADSUB, value: 0x7f });
  });

  it("setMathAddend writes CGWSEL bit1 only (direct_color + clip + region preserved)", () => {
    expect(setMathAddend("sub", 0xf1)).toEqual({ field: "color.addend", expr: '"sub"', addr: REG.CGWSEL, value: 0xf3 });
    expect(setMathAddend("fixed", 0xf3)).toEqual({ field: "color.addend", expr: '"fixed"', addr: REG.CGWSEL, value: 0xf1 });
  });

  it("setFixedColor emits 15-bit hex COLDATA", () => {
    expect(setFixedColor(0x7fff)).toEqual({ field: "color.fixed", expr: "0x7fff", addr: REG.COLDATA, value: 0x7fff });
    expect(setFixedColor(0)).toEqual({ field: "color.fixed", expr: "0x0000", addr: REG.COLDATA, value: 0 });
  });
});

describe("window select rows", () => {
  const bg1 = WINDOW_LAYERS[0];
  const bg2 = WINDOW_LAYERS[1];
  const bg3 = WINDOW_LAYERS[2];
  const color = WINDOW_LAYERS[4];

  it("decodes enable/invert from the right nibble", () => {
    expect(windowRow(bg2, read({ [REG.W12SEL]: 0xa0 }))).toEqual({
      enabled: true,
      inverted: false,
    });
    expect(windowRow(bg1, read({ [REG.W12SEL]: 0x05 }))).toEqual({
      enabled: false,
      inverted: true,
    });
  });

  it("enable toggle emits w1+w2+main bool fields carrying the folded register bytes", () => {
    expect(toggleWindowEnable(bg1, read({}))).toEqual([
      { field: "win.bg1.w1", expr: "true", addr: REG.W12SEL, value: 0x0a },
      { field: "win.bg1.w2", expr: "true", addr: REG.W12SEL, value: 0x0a },
      { field: "win.bg1.main", expr: "true", addr: REG.TMW, value: 0x01 },
    ]);
    expect(toggleWindowEnable(bg1, read({ [REG.W12SEL]: 0x0a, [REG.TMW]: 0x1f }))).toEqual([
      { field: "win.bg1.w1", expr: "false", addr: REG.W12SEL, value: 0x00 },
      { field: "win.bg1.w2", expr: "false", addr: REG.W12SEL, value: 0x00 },
      { field: "win.bg1.main", expr: "false", addr: REG.TMW, value: 0x1e },
    ]);
  });

  it("color-row enable routes CGWSEL through color.region — never a win field", () => {
    expect(toggleWindowEnable(color, read({ [REG.CGWSEL]: 0x02 }))).toEqual([
      { field: "win.color.w1", expr: "true", addr: REG.WOBJSEL, value: 0xa0 },
      { field: "win.color.w2", expr: "true", addr: REG.WOBJSEL, value: 0xa0 },
      { field: "color.region", expr: '"inside"', addr: REG.CGWSEL, value: 0x12 },
    ]);
    expect(toggleWindowEnable(color, read({ [REG.WOBJSEL]: 0xa0, [REG.CGWSEL]: 0x12 }))).toEqual([
      { field: "win.color.w1", expr: "false", addr: REG.WOBJSEL, value: 0x00 },
      { field: "win.color.w2", expr: "false", addr: REG.WOBJSEL, value: 0x00 },
      { field: "color.region", expr: '"everywhere"', addr: REG.CGWSEL, value: 0x02 },
    ]);
  });

  it("invert toggle is ONE lossy field per row (both invert bits, the core decode)", () => {
    expect(toggleWindowInvert(bg3, read({}))).toEqual([
      { field: "win.bg3.invert", expr: "true", addr: REG.W34SEL, value: 0x05 },
    ]);
    expect(toggleWindowInvert(bg3, read({ [REG.W34SEL]: 0xf5 }))).toEqual([
      { field: "win.bg3.invert", expr: "false", addr: REG.W34SEL, value: 0xf0 },
    ]);
  });

  it("setWindowEdge names the WH scalar field with a decimal expr", () => {
    expect(setWindowEdge(REG.WH0, 40)).toEqual({ field: "win.w1.lo", expr: "40", addr: REG.WH0, value: 40 });
    expect(setWindowEdge(REG.WH3, 200)).toEqual({ field: "win.w2.hi", expr: "200", addr: REG.WH3, value: 200 });
  });
});

describe("combine + area segmenteds", () => {
  it("combineValue is the op all six logic slots agree on, else null", () => {
    expect(combineValue(read({}))).toBe(0);
    expect(combineValue(read({ [REG.WBGLOG]: 0xaa, [REG.WOBJLOG]: 0x0a }))).toBe(2);
    expect(combineValue(read({ [REG.WBGLOG]: 0xab, [REG.WOBJLOG]: 0x0a }))).toBeNull();
    expect(combineValue(read({ [REG.WBGLOG]: 0xaa, [REG.WOBJLOG]: 0x02 }))).toBeNull();
  });

  it("setCombine emits all six layers' combine fields (incl. registers-only bg4)", () => {
    const ws = setCombine(1);
    expect(ws.map((w) => w.field)).toEqual([
      "win.bg1.combine", "win.bg2.combine", "win.bg3.combine",
      "win.bg4.combine", "win.obj.combine", "win.color.combine",
    ]);
    expect(ws.every((w) => w.expr === '"AND"')).toBe(true);
    expect(writesToPokes(ws, "raw")).toEqual([regPoke(REG.WBGLOG, 0x55), regPoke(REG.WOBJLOG, 0x05)]);
    expect(writesToPokes(setCombine(3), "raw")).toEqual([regPoke(REG.WBGLOG, 0xff), regPoke(REG.WOBJLOG, 0x0f)]);
  });

  it("areaValue aggregates the five rows' inverts (mixed = null)", () => {
    expect(areaValue(read({}))).toBe("inside");
    expect(
      areaValue(read({ [REG.W12SEL]: 0x55, [REG.W34SEL]: 0x05, [REG.WOBJSEL]: 0x55 })),
    ).toBe("outside");
    expect(areaValue(read({ [REG.W12SEL]: 0x05 }))).toBeNull();
  });

  it("setArea emits the five UI rows' invert fields; same-register writes share the final byte (BG4 nibble preserved)", () => {
    expect(setArea("outside", read({ [REG.W34SEL]: 0xa0 })).map((w) => [w.field, w.expr, w.addr, w.value])).toEqual([
      ["win.bg1.invert", "true", REG.W12SEL, 0x55],
      ["win.bg2.invert", "true", REG.W12SEL, 0x55],
      ["win.bg3.invert", "true", REG.W34SEL, 0xa5],
      ["win.obj.invert", "true", REG.WOBJSEL, 0x55],
      ["win.color.invert", "true", REG.WOBJSEL, 0x55],
    ]);
  });
});

describe("window preview geometry", () => {
  const b = { wh0: 0, wh1: 127, wh2: 64, wh3: 255 };

  it("windowBounds reads WH0-3", () => {
    expect(
      windowBounds(read({ [REG.WH0]: 3, [REG.WH1]: 9, [REG.WH2]: 27, [REG.WH3]: 81 })),
    ).toEqual({ wh0: 3, wh1: 9, wh2: 27, wh3: 81 });
  });

  it("columnMask combines the raw ranges under each logic op", () => {
    // sample columns: 32 (only W1), 96 (both), 200 (only W2)
    const at = (logic: 0 | 1 | 2 | 3, x: number) => columnMask(b, logic, false)[x];
    expect([at(0, 32), at(0, 96), at(0, 200)]).toEqual([1, 1, 1]); // OR
    expect([at(1, 32), at(1, 96), at(1, 200)]).toEqual([0, 1, 0]); // AND
    expect([at(2, 32), at(2, 96), at(2, 200)]).toEqual([1, 0, 1]); // XOR
    expect([at(3, 32), at(3, 96), at(3, 200)]).toEqual([0, 1, 0]); // XNOR
  });

  it("area=outside inverts the mask", () => {
    const m = columnMask(b, 1, true);
    expect(m[96]).toBe(0);
    expect(m[32]).toBe(1);
  });

  it("nearestEdgeAddr picks the closest WH edge (ties -> lower addr)", () => {
    const bounds = { wh0: 40, wh1: 100, wh2: 64, wh3: 200 };
    expect(nearestEdgeAddr(60, bounds)).toBe(REG.WH2);
    expect(nearestEdgeAddr(0, bounds)).toBe(REG.WH0);
    expect(nearestEdgeAddr(255, bounds)).toBe(REG.WH3);
    expect(nearestEdgeAddr(110, { wh0: 100, wh1: 120, wh2: 0, wh3: 0 })).toBe(REG.WH0);
  });
});

describe("fieldPoke + writesToPokes (dual-dialect projection)", () => {
  const op: FieldWrite = { field: "color.op", expr: '"sub"', addr: REG.CGADSUB, value: 0x80 };
  const w1: FieldWrite = { field: "win.bg1.w1", expr: "true", addr: REG.W12SEL, value: 0x0a };
  const w2: FieldWrite = { field: "win.bg1.w2", expr: "true", addr: REG.W12SEL, value: 0x0a };

  it("fieldPoke emits the friendly assignment with a $-address note", () => {
    expect(fieldPoke(op)).toEqual({ lvalue: "color.op", expr: '"sub"', note: "$2131" });
  });

  it("friendly projection is one poke per field write", () => {
    expect(writesToPokes([w1, w2], "friendly")).toEqual([fieldPoke(w1), fieldPoke(w2)]);
  });

  it("raw projection dedupes per register (last wins) and byte-matches regPoke", () => {
    expect(writesToPokes([w1, w2, op], "raw")).toEqual([
      regPoke(REG.W12SEL, 0x0a),
      regPoke(REG.CGADSUB, 0x80),
    ]);
  });

  it("friendly pokes round-trip through pokesToLua/parsePokes (dialect-agnostic loader)", () => {
    const ps = writesToPokes([op, w1], "friendly");
    expect(parsePokes(pokesToLua(ps))).toEqual([...ps].sort((a, b) => (a.lvalue < b.lvalue ? -1 : 1)));
  });

  it("mixed-dialect output stays codepoint-sorted (raw mnemonics before lowercase fields)", () => {
    const lua = pokesToLua([fieldPoke(op), regPoke(REG.TM, 0x13)]);
    expect(lua.indexOf("TM = 0x13")).toBeLessThan(lua.indexOf('color.op = "sub"'));
  });
});

describe("FIELD_SPECS + friendly pokeMatchesLive", () => {
  it("maps every field to the register the Rust fold owns", () => {
    expect(FIELD_SPECS.get("color.op")?.addr).toBe(REG.CGADSUB);
    expect(FIELD_SPECS.get("color.region")?.addr).toBe(REG.CGWSEL);
    expect(FIELD_SPECS.get("screen.main.obj")?.addr).toBe(REG.TM);
    expect(FIELD_SPECS.get("screen.sub.bg4")?.addr).toBe(REG.TS);
    expect(FIELD_SPECS.get("win.w2.hi")?.addr).toBe(REG.WH3);
    expect(FIELD_SPECS.get("win.bg4.combine")?.addr).toBe(REG.WBGLOG);
    expect(FIELD_SPECS.get("win.obj.sub")?.addr).toBe(REG.TSW);
    expect(FIELD_SPECS.get("win.color.main")).toBeUndefined(); // color window has no TMW bit
    for (const [f, s] of FIELD_SPECS) {
      if (f.startsWith("win.")) expect(s.addr, `${f} must not own CGWSEL`).not.toBe(REG.CGWSEL);
    }
  });

  it("solid when the live register bits decode to the poked field value", () => {
    expect(pokeMatchesLive({ lvalue: "color.op", expr: '"sub"' }, [rv(REG.CGADSUB, "CGADSUB", 0x80)])).toBe(true);
    expect(pokeMatchesLive({ lvalue: "screen.main.bg2", expr: "true" }, [])).toBe(true); // power-on TM=0x1f
    expect(pokeMatchesLive({ lvalue: "color.region", expr: '"inside"' }, [rv(REG.CGWSEL, "CGWSEL", 0x12)])).toBe(true);
    expect(pokeMatchesLive({ lvalue: "win.color.combine", expr: '"AND"' }, [rv(REG.WOBJLOG, "WOBJLOG", 0x04)])).toBe(true);
    expect(pokeMatchesLive({ lvalue: "color.fixed", expr: "0x7fff" }, [rv(REG.COLDATA, "COLDATA", 0x7fff)])).toBe(true);
  });

  it("hollow when a later script write moved the field's bits", () => {
    expect(pokeMatchesLive({ lvalue: "color.op", expr: '"sub"' }, [rv(REG.CGADSUB, "CGADSUB", 0x00)])).toBe(false);
    expect(pokeMatchesLive({ lvalue: "screen.main.bg1", expr: "false" }, [])).toBe(false); // TM=0x1f has bg1 on
  });

  it("numeric fields compare by value across hex/decimal spellings", () => {
    expect(pokeMatchesLive({ lvalue: "win.w1.lo", expr: "40" }, [rv(REG.WH0, "WH0", 40)])).toBe(true);
    expect(pokeMatchesLive({ lvalue: "win.w1.lo", expr: "0x28" }, [rv(REG.WH0, "WH0", 40)])).toBe(true);
    expect(pokeMatchesLive({ lvalue: "win.w1.lo", expr: "40" }, [rv(REG.WH0, "WH0", 41)])).toBe(false);
  });

  it("invert decode is lossy like the core: EITHER invert bit reads true", () => {
    expect(pokeMatchesLive({ lvalue: "win.bg1.invert", expr: "true" }, [rv(REG.W12SEL, "W12SEL", 0x01)])).toBe(true);
    expect(pokeMatchesLive({ lvalue: "win.bg1.invert", expr: "false" }, [rv(REG.W12SEL, "W12SEL", 0x00)])).toBe(true);
  });

  it("null for an unknown field or an unparseable expr", () => {
    expect(pokeMatchesLive({ lvalue: "win.nope.w1", expr: "true" }, [])).toBeNull();
    expect(pokeMatchesLive({ lvalue: "color.op", expr: "sub" }, [])).toBeNull(); // unquoted
  });
});

describe("pokesAt (field-keyed marker lookup)", () => {
  const ps = [
    { lvalue: "W12SEL", expr: "0x03" },
    { lvalue: "win.bg1.w1", expr: "true" },
    { lvalue: "win.bg2.invert", expr: "true" },
  ];
  it("addr-wide (no fields): raw poke + every friendly field living in the register", () => {
    expect(pokesAt(ps, REG.W12SEL).map((p) => p.lvalue)).toEqual(["W12SEL", "win.bg1.w1", "win.bg2.invert"]);
    expect(pokesAt(ps, REG.TM)).toEqual([]);
  });
  it("field-scoped: raw poke + only the listed fields (bg1/bg2 share W12SEL)", () => {
    expect(pokesAt(ps, REG.W12SEL, ["win.bg1.w1", "win.bg1.w2", "win.bg1.invert"]).map((p) => p.lvalue))
      .toEqual(["W12SEL", "win.bg1.w1"]);
  });
});

describe("preview buffers", () => {
  it("dimOutsideMask keeps in-mask pixels and dims the rest (x0.3 R/G, x0.42 B)", () => {
    const fb = new Uint8ClampedArray(256 * 224 * 4).fill(100);
    const mask = new Uint8Array(256);
    for (let x = 0; x < 128; x++) mask[x] = 1;
    const out = dimOutsideMask(fb, mask);
    expect([out[0], out[1], out[2], out[3]]).toEqual([100, 100, 100, 255]); // x=0 inside
    const i = 200 * 4; // x=200 outside, row 0
    expect([out[i], out[i + 1], out[i + 2], out[i + 3]]).toEqual([30, 30, 42, 255]);
  });

  it("tintMathRegion tints only mathMask bit0 pixels", () => {
    const fb = new Uint8ClampedArray(256 * 224 * 4).fill(100);
    const mask = new Uint8Array(256 * 224);
    mask[0] = 1; // math applied
    mask[1] = 2; // clip bit only -> untouched
    const out = tintMathRegion(fb, mask);
    expect([out[0], out[1], out[2]]).toEqual([40, 170, 40]);
    expect([out[4], out[5], out[6]]).toEqual([100, 100, 100]);
    expect(fb[0]).toBe(100); // input untouched
  });
});

describe("emission/decode invariants", () => {
  const sample = (): FieldWrite[] => [
    toggleDesignation("screen.main.bg1", REG.TM, 0x1f, 0),
    toggleDesignation("screen.sub.obj", REG.TS, 0, 4),
    toggleDesignation("color.on.bg3", REG.CGADSUB, 0, 2),
    toggleDesignation("color.on.backdrop", REG.CGADSUB, 0, 5),
    setMathOp("sub", 0), setMathHalf(true, 0), setMathAddend("sub", 0), setFixedColor(1),
    setWindowEdge(REG.WH0, 1), setWindowEdge(REG.WH1, 1), setWindowEdge(REG.WH2, 1), setWindowEdge(REG.WH3, 1),
    ...WINDOW_LAYERS.flatMap((l) => [...toggleWindowEnable(l, read({})), ...toggleWindowInvert(l, read({}))]),
    ...setCombine(2),
    ...setArea("outside", read({})),
  ];

  it("every emitted field is decodable: it's in FIELD_SPECS at the same register", () => {
    for (const w of sample()) {
      expect(FIELD_SPECS.get(w.field), w.field).toBeDefined();
      expect(FIELD_SPECS.get(w.field)?.addr, w.field).toBe(w.addr);
    }
  });

  it("every emitted write reads back SOLID once the core folds its raw value", () => {
    for (const w of sample()) {
      expect(pokeMatchesLive(fieldPoke(w), [rv(w.addr, "", w.value)]), w.field).toBe(true);
    }
  });

  it("a layer row's marker covers EVERY field its controls emit (enable's TMW / region too)", () => {
    for (const l of WINDOW_LAYERS) {
      const covered = winRowFields(l);
      const emitted = [...toggleWindowEnable(l, read({})), ...toggleWindowInvert(l, read({}))];
      for (const w of emitted) expect(covered, `${l.id} row must cover ${w.field}`).toContain(w.field);
    }
  });

  it("header/group field lists are all decodable", () => {
    const groups = [
      ...SCREEN_MAIN_FIELDS, ...SCREEN_SUB_FIELDS, ...MATH_ENABLE_FIELDS,
      ...OPERATION_FIELDS, ...ADDEND_FIELDS, ...FIXED_FIELDS,
      ...COMBINE_FIELDS, ...AREA_FIELDS, ...WINDOW_LAYERS.flatMap(winRowFields),
    ];
    for (const f of groups) expect(FIELD_SPECS.has(f), f).toBe(true);
  });
});

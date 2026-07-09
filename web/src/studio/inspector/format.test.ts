import { describe, it, expect } from "vitest";
import { formatAddr, formatValue, cgram15ToCss, bgMode, screenLayers, colorMath, windowRanges, extbg } from "./format";
import type { RegisterView } from "../../ppu/core";

const reg = (name: string, value: number): RegisterView => ({ addr: 0, name, value, changed: false });

describe("inspector format", () => {
  it("formats register addr as $XXXX uppercase 4-digit", () => {
    expect(formatAddr(0x2100)).toBe("$2100");
    expect(formatAddr(0x420)).toBe("$0420");
  });
  it("formats register value as uppercase hex, min 2 digits", () => {
    expect(formatValue(0x0f)).toBe("0F");
    expect(formatValue(0xc8)).toBe("C8");
    expect(formatValue(0x3def)).toBe("3DEF");
  });
  it("formats multi-digit register values (absolute scroll)", () => {
    expect(formatValue(419)).toBe("1A3");
    expect(formatValue(0)).toBe("00");
  });
  it("expands 15-bit BGR cgram to an rgb() css string", () => {
    expect(cgram15ToCss(0x0000)).toBe("rgb(0, 0, 0)");
    expect(cgram15ToCss(0x7fff)).toBe("rgb(255, 255, 255)");
    // pure red: R=31 -> 0x001f
    expect(cgram15ToCss(0x001f)).toBe("rgb(255, 0, 0)");
    // pure green: G=31 -> 0x03e0
    expect(cgram15ToCss(0x03e0)).toBe("rgb(0, 255, 0)");
    // pure blue: B=31 -> 0x7c00
    expect(cgram15ToCss(0x7c00)).toBe("rgb(0, 0, 255)");
  });
});

describe("bgMode", () => {
  it("reads the low 3 bits of BGMODE", () => {
    expect(bgMode([reg("BGMODE", 0x02)])).toBe(2);
    expect(bgMode([reg("BGMODE", 0x07)])).toBe(7); // mode 7
    expect(bgMode([reg("BGMODE", 0x91)])).toBe(1); // tile-size bits stripped
  });
  it("defaults to mode 1 when BGMODE is absent", () => {
    expect(bgMode([])).toBe(1);
  });
});

describe("inspector m6 decoders", () => {
  const regs = (m: Record<string, number>): RegisterView[] =>
    Object.entries(m).map(([name, value]) => ({ addr: 0, name, value, changed: false }));

  it("screenLayers reads TM/TS bit masks into layer labels", () => {
    const r = regs({ TM: 0x13, TS: 0x04 });
    expect(screenLayers(r, "TM")).toEqual(["BG1", "BG2", "OBJ"]);
    expect(screenLayers(r, "TS")).toEqual(["BG3"]);
  });
  it("screenLayers defaults to power-on TM=all / TS=none when absent", () => {
    expect(screenLayers([], "TM")).toEqual(["BG1", "BG2", "BG3", "BG4", "OBJ"]);
    expect(screenLayers([], "TS")).toEqual([]);
  });
  it("colorMath decodes sign, half, source and enabled layers", () => {
    const r = regs({ CGADSUB: 0x41, CGWSEL: 0x02 });
    expect(colorMath(r)).toEqual({ op: "add", half: true, source: "sub", layers: ["BG1"] });
    const r2 = regs({ CGADSUB: 0x81, CGWSEL: 0x00 });
    expect(colorMath(r2)).toEqual({ op: "sub", half: false, source: "fixed", layers: ["BG1"] });
  });
  it("colorMath reports no layers when none are enabled", () => {
    expect(colorMath(regs({ CGADSUB: 0x00, CGWSEL: 0x00 })).layers).toEqual([]);
  });
  it("colorMath includes BACK when the backdrop math bit is set", () => {
    expect(colorMath(regs({ CGADSUB: 0x20 })).layers).toEqual(["BACK"]);
  });
  it("windowRanges reads WH0-3 into two [left,right] spans", () => {
    const r = regs({ WH0: 32, WH1: 200, WH2: 10, WH3: 240 });
    expect(windowRanges(r)).toEqual({ w1: [32, 200], w2: [10, 240] });
  });
  it("decodes SETINI bit6 as EXTBG", () => {
    expect(extbg(regs({ SETINI: 0x40 }))).toBe(true);
    expect(extbg(regs({ SETINI: 0x00 }))).toBe(false);
    expect(extbg(regs({ SETINI: 0x80 }))).toBe(false); // other bits don't enable it
  });
});

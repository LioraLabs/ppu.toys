import { describe, it, expect } from "vitest";
import { formatAddr, formatValue, cgram15ToCss, bgMode } from "./format";
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

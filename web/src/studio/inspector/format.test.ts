import { describe, it, expect } from "vitest";
import { formatAddr, formatValue, cgram15ToCss } from "./format";

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
  it("expands 15-bit BGR cgram to an rgb() css string", () => {
    expect(cgram15ToCss(0x0000)).toBe("rgb(0, 0, 0)");
    expect(cgram15ToCss(0x7fff)).toBe("rgb(255, 255, 255)");
    // pure red: R=31 -> 0x001f
    expect(cgram15ToCss(0x001f)).toBe("rgb(255, 0, 0)");
    // pure blue: B=31 -> 0x7c00
    expect(cgram15ToCss(0x7c00)).toBe("rgb(0, 0, 255)");
  });
});

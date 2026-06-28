import { describe, it, expect } from "vitest";
import { bgr555ToHex } from "./decode";

describe("bgr555ToHex", () => {
  it("decodes black", () => {
    expect(bgr555ToHex(0x0000)).toBe("#000000");
  });
  it("decodes white (all 5-bit channels max)", () => {
    expect(bgr555ToHex(0x7fff)).toBe("#ffffff");
  });
  it("decodes pure red (low 5 bits)", () => {
    expect(bgr555ToHex(0x001f)).toBe("#ff0000");
  });
  it("decodes pure green (mid 5 bits)", () => {
    expect(bgr555ToHex(0x03e0)).toBe("#00ff00");
  });
  it("decodes pure blue (high 5 bits)", () => {
    expect(bgr555ToHex(0x7c00)).toBe("#0000ff");
  });
  it("ignores the unused bit 15", () => {
    expect(bgr555ToHex(0x8000)).toBe("#000000");
  });
  it("scales a mid value (channel=16 -> 132)", () => {
    // 16/31*255 = 131.6 -> round 132 -> 0x84
    expect(bgr555ToHex(0x0010)).toBe("#840000");
  });
});

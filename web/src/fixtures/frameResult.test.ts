import { describe, expect, it } from "vitest";
import { bgMode } from "../studio/inspector/format";
import { frameResult, makeFrameResult } from "./index";

describe("frameResult fixture", () => {
  it("framebuffer is 256*224*4 RGBA", () => {
    expect(frameResult.framebuffer.length).toBe(256 * 224 * 4);
  });

  it("cgram has 256 entries", () => {
    expect(frameResult.cgram.length).toBe(256);
  });

  it("oam has exactly 128 sprites, some on", () => {
    expect(frameResult.oam.length).toBe(128);
    expect(frameResult.oam.some((s) => s.on)).toBe(true);
  });

  it("BGMODE register decodes to mode 1", () => {
    expect(bgMode(frameResult.registers)).toBe(1);
  });

  it("cgram is not blank", () => {
    expect(frameResult.cgram.some((c) => c !== 0)).toBe(true);
  });

  it("makeFrameResult shallow-merges overrides", () => {
    const overridden = makeFrameResult({
      objOverflow: { rangeOver: true, timeOver: true, maxSprites: 32, maxTiles: 34 },
    });
    expect(overridden.objOverflow.rangeOver).toBe(true);
  });
});

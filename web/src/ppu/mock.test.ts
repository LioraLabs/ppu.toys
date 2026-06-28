import { describe, it, expect } from "vitest";
import { MockPpuCore } from "./mock";
import { WIDTH, HEIGHT } from "./core";

function differs(a: Uint8ClampedArray, b: Uint8ClampedArray): boolean {
  if (a.length !== b.length) return true;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return true;
  return false;
}

describe("MockPpuCore", () => {
  it("frame() returns a full, opaque framebuffer plus registers and cgram", () => {
    const core = new MockPpuCore();
    const { framebuffer, registers, cgram } = core.frame(0, 0);
    expect(framebuffer.length).toBe(WIDTH * HEIGHT * 4);
    expect(framebuffer[3]).toBe(255);
    expect(cgram.length).toBe(256);
    expect(registers.length).toBeGreaterThan(0);
  });

  it("setSource() reports ok", () => {
    expect(new MockPpuCore().setSource("frame=function() end").ok).toBe(true);
  });

  it("framebuffer varies across t/f", () => {
    const core = new MockPpuCore();
    const a = core.frame(0, 0).framebuffer.slice();
    const b = core.frame(1.5, 90).framebuffer.slice();
    expect(differs(a, b)).toBe(true);
  });

  it("hiding the bg1 layer changes the background output", () => {
    const core = new MockPpuCore();
    const lit = core.frame(0.5, 30).framebuffer.slice();
    core.setLayerVisible("bg1", false);
    const dark = core.frame(0.5, 30).framebuffer.slice();
    expect(differs(lit, dark)).toBe(true);
    // top-left corner is background only (the blob never reaches it) -> black
    expect(dark[0]).toBe(0);
    expect(dark[1]).toBe(0);
    expect(dark[2]).toBe(0);
  });
});

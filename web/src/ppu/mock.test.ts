import { describe, it, expect } from "vitest";
import { MockPpuCore } from "./mock";
import { WIDTH, HEIGHT } from "./core";

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
});

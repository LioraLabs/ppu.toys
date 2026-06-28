import { describe, it, expect } from "vitest";
import { MockPpuCore } from "./mock";
import { WIDTH, HEIGHT, RegisterView } from "./core";

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

  it("registers change value and toggle the changed flag across frames", () => {
    const core = new MockPpuCore();
    const r0 = core.frame(0, 0).registers;
    // first frame has no prior values -> nothing flagged changed
    expect(r0.every((r) => r.changed === false)).toBe(true);

    const r1 = core.frame(2, 120).registers;
    const byName = (rs: RegisterView[], n: string) => rs.find((r) => r.name === n)!;
    // BG1HOFS tracks the scroll, so its value moved and it is flagged changed
    expect(byName(r1, "BG1HOFS").value).not.toBe(byName(r0, "BG1HOFS").value);
    expect(byName(r1, "BG1HOFS").changed).toBe(true);
    // a static register keeps its value and is not flagged
    expect(byName(r1, "BGMODE").changed).toBe(false);

    // re-rendering the same frame flags nothing as changed
    const r1again = core.frame(2, 120).registers;
    expect(r1again.every((r) => r.changed === false)).toBe(true);
  });

  it("cgram animates a color-cycling palette window across t", () => {
    const core = new MockPpuCore();
    const c0 = core.frame(0, 0).cgram.slice();
    const c1 = core.frame(1, 60).cgram.slice();
    expect(c0[0x40]).not.toBe(c1[0x40]); // the cycling window moved
    expect(c0[0]).toBe(c1[0]); // the static base gradient did not
  });

  it("uploadTexture stores assets and they nudge the output", () => {
    const core = new MockPpuCore();
    const before = core.frame(0.5, 30).framebuffer.slice();
    const img = { width: 1, height: 1, data: new Uint8ClampedArray(4), colorSpace: "srgb" } as ImageData;
    core.uploadTexture("sky", img);
    const after = core.frame(0.5, 30).framebuffer.slice();
    expect(differs(before, after)).toBe(true);
  });
});

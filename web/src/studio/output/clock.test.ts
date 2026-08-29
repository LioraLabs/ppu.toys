import { describe, it, expect } from "vitest";
import { FPS, advanceClock, seekClock, integerScale } from "./clock";
import { WIDTH, HEIGHT } from "../../ppu/core";

describe("advanceClock", () => {
  it("advances t by real elapsed time and derives f = floor(t*FPS)", () => {
    const c = advanceClock({ t: 0, f: 0 }, 50); // 50ms ~ 3 frames
    expect(c.t).toBeCloseTo(0.05, 5);
    expect(c.f).toBe(Math.floor(0.05 * FPS)); // 3
  });

  it("clamps a huge dt (tab refocus) to a single 100ms step", () => {
    // unclamped this would advance 60s; clamped it advances exactly 100ms
    const c = advanceClock({ t: 0, f: 0 }, 60_000); // 60s gap
    expect(c.t).toBeCloseTo(0.1, 5);
  });

  it("keeps increasing instead of wrapping", () => {
    const c = advanceClock({ t: 9.92, f: 0 }, 100);
    expect(c.t).toBeCloseTo(10.02, 5);
    expect(c.f).toBe(601);
  });
});

describe("seekClock", () => {
  it("seeks to an absolute time and clamps before zero", () => {
    expect(seekClock(12.5)).toEqual({ t: 12.5, f: 750 });
    expect(seekClock(-1)).toEqual({ t: 0, f: 0 });
  });
});

describe("integerScale", () => {
  it("returns the largest integer scale that fits, preserving native res", () => {
    expect(integerScale(WIDTH * 3, HEIGHT * 3)).toBe(3);
    expect(integerScale(WIDTH * 2 + 10, HEIGHT * 2 + 10)).toBe(2);
  });

  it("never returns less than 1, even in a tiny container", () => {
    expect(integerScale(10, 10)).toBe(1);
  });
});

import { describe, it, expect } from "vitest";
import {
  FPS,
  LOOP_SECONDS,
  advanceClock,
  scrubToClock,
  clockToScrub,
  integerScale,
} from "./clock";
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

  it("wraps around LOOP_SECONDS so the timeline is bounded", () => {
    const c = advanceClock({ t: LOOP_SECONDS - 0.08, f: 0 }, 100); // 9.92 + 0.1 -> 0.02
    expect(c.t).toBeCloseTo(0.02, 5);
    expect(c.f).toBe(1); // floor(0.02 * 60) = 1
  });
});

describe("scrubToClock / clockToScrub", () => {
  it("maps a 0..1 fraction onto the loop and back", () => {
    const c = scrubToClock(0.5);
    expect(c.t).toBeCloseTo(LOOP_SECONDS / 2, 5);
    expect(c.f).toBe(Math.floor((LOOP_SECONDS / 2) * FPS));
    expect(clockToScrub({ t: LOOP_SECONDS / 2, f: 0 })).toBeCloseTo(0.5, 5);
  });

  it("clamps out-of-range fractions", () => {
    expect(scrubToClock(-1).t).toBe(0);
    expect(scrubToClock(2).t).toBeCloseTo(LOOP_SECONDS, 5);
  });

  it("maps the right edge to 1, not back to 0 (clamp, not wrap)", () => {
    expect(clockToScrub(scrubToClock(1))).toBeCloseTo(1, 5);
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

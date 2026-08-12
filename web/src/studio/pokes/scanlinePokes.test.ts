import { describe, expect, it } from "vitest";
import {
  DEFAULT_RANGE,
  LAST_LINE,
  clampRange,
  formatKeyframes,
  isScanlinePoke,
  pokeRange,
  keyframeAt,
  keyframeValueAt,
  parseKeyframes,
  pkHelper,
  removeKeyframe,
  setKeyframe,
} from "./scanlinePokes";

describe("keyframe expr round-trip", () => {
  it("formats sorted, compact and byte-stable", () => {
    expect(
      formatKeyframes([
        { y: 112, v: 58 },
        { y: 0, v: 128 },
      ]),
    ).toBe("{{0,128},{112,58}}");
    // same keyframes in any order render identically — the file is the source
    // of truth, so output must not depend on insertion order
    expect(
      formatKeyframes([
        { y: 0, v: 128 },
        { y: 112, v: 58 },
      ]),
    ).toBe("{{0,128},{112,58}}");
  });

  it("round-trips through parse", () => {
    const kf = [
      { y: 0, v: 128 },
      { y: 112, v: 58 },
      { y: 223, v: 128 },
    ];
    expect(parseKeyframes(formatKeyframes(kf))).toEqual(kf);
  });

  it("carries fractional values for float fields", () => {
    expect(formatKeyframes([{ y: 96, v: 1.5 }])).toBe("{{96,1.5}}");
    expect(parseKeyframes("{{96,1.5}}")).toEqual([{ y: 96, v: 1.5 }]);
  });

  it("rejects anything that is not a well-formed keyframe table", () => {
    for (const bad of ["0x13", "true", '"add"', "{}", "{{0}}", "{{0,1}", "{{0,1},}", "40"]) {
      expect(parseKeyframes(bad)).toBeNull();
    }
  });

  it("discriminates scanline pokes by their expr alone", () => {
    expect(isScanlinePoke({ lvalue: "win.w1.lo", expr: "{{0,128}}" })).toBe(true);
    expect(isScanlinePoke({ lvalue: "win.w1.lo", expr: "40" })).toBe(false);
    expect(isScanlinePoke({ lvalue: "color.op", expr: '"add"' })).toBe(false);
  });
});

describe("interpolation", () => {
  const kf = [
    { y: 0, v: 128 },
    { y: 112, v: 58 },
    { y: 223, v: 128 },
  ];

  it("returns the exact value on a keyframe", () => {
    expect(keyframeAt(kf, 0)).toBe(128);
    expect(keyframeAt(kf, 112)).toBe(58);
    expect(keyframeAt(kf, 223)).toBe(128);
  });

  it("interpolates linearly between keyframes", () => {
    expect(keyframeAt(kf, 56)).toBe(93); // midpoint of 128 -> 58
  });

  it("holds the end values outside the keyframed range", () => {
    expect(keyframeAt([{ y: 50, v: 9 }], 0)).toBe(9);
    expect(keyframeAt([{ y: 50, v: 9 }], 223)).toBe(9);
  });

  it("mirrors the generated pki rounding for integer fields", () => {
    // 0..112 ramp 128 -> 58: line 1 is 127.375, which the DSL would DROP if
    // written fractionally, so the helper must round it.
    expect(keyframeAt(kf, 1)).toBeCloseTo(127.375);
    expect(keyframeValueAt("win.w1.lo", kf, 1)).toBe(127);
    // float fields pass the raw lerp through
    expect(keyframeValueAt("m7.a", kf, 1)).toBeCloseTo(127.375);
  });

  it("picks the rounding helper per field", () => {
    expect(pkHelper("win.w1.lo")).toBe("pki");
    expect(pkHelper("m7.a")).toBe("pkf");
    expect(pkHelper("m7.cx")).toBe("pki"); // centre coords are integers
  });
});

describe("hook range", () => {
  it("defaults to the whole frame when the poke carries none", () => {
    expect(pokeRange({ lvalue: "win.w1.lo", expr: "{{0,1}}" })).toEqual([0, 223]);
    expect(DEFAULT_RANGE).toEqual([0, LAST_LINE]);
  });

  it("uses the poke's own range when it has one", () => {
    expect(pokeRange({ lvalue: "win.w1.lo", expr: "{{0,1}}", range: [96, 200] })).toEqual([
      96, 200,
    ]);
  });

  it("clamps to the visible frame", () => {
    expect(clampRange(-10, 999)).toEqual([0, 223]);
  });

  it("orders an inverted pair rather than generating a hook that covers nothing", () => {
    expect(clampRange(200, 30)).toEqual([30, 200]);
  });

  it("rounds and survives a half-typed (NaN) input", () => {
    expect(clampRange(10.6, 50.2)).toEqual([11, 50]);
    expect(clampRange(NaN, 50)).toEqual([0, 50]);
  });
});

describe("keyframe editing", () => {
  it("replaces the keyframe already on that line", () => {
    const kf = setKeyframe([{ y: 10, v: 1 }], 10, 5);
    expect(kf).toEqual([{ y: 10, v: 5 }]);
  });

  it("inserts in scanline order", () => {
    let kf = setKeyframe([], 100, 5);
    kf = setKeyframe(kf, 10, 1);
    kf = setKeyframe(kf, 50, 3);
    expect(kf.map((k) => k.y)).toEqual([10, 50, 100]);
  });

  it("removes by line and no-ops on a line with no keyframe", () => {
    const kf = [
      { y: 10, v: 1 },
      { y: 50, v: 3 },
    ];
    expect(removeKeyframe(kf, 10)).toEqual([{ y: 50, v: 3 }]);
    expect(removeKeyframe(kf, 99)).toEqual(kf);
  });
});

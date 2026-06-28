import { describe, it, expect } from "vitest";
import { DEFAULT_FX, parseFx, fxUniforms, type PresentFx } from "./fx";

describe("parseFx", () => {
  it("returns DEFAULT_FX (all off) for null / garbage", () => {
    expect(parseFx(null)).toEqual(DEFAULT_FX);
    expect(parseFx("not json")).toEqual(DEFAULT_FX);
    expect(DEFAULT_FX).toEqual({ crt: false, scanline: false, pixelGrid: false });
  });
  it("keeps known boolean flags and ignores extra keys", () => {
    expect(parseFx(JSON.stringify({ crt: true, scanline: false, pixelGrid: true, junk: 9 })))
      .toEqual({ crt: true, scanline: false, pixelGrid: true });
  });
  it("coerces missing/non-boolean flags to false", () => {
    expect(parseFx(JSON.stringify({ crt: 1, scanline: "yes" })))
      .toEqual({ crt: false, scanline: false, pixelGrid: false });
  });
});

describe("fxUniforms", () => {
  it("maps an all-off state to zeroed uniforms", () => {
    expect(fxUniforms(DEFAULT_FX)).toEqual({ uCrt: 0, uScanline: 0, uGrid: 0 });
  });
  it("maps enabled flags to 1", () => {
    const fx: PresentFx = { crt: true, scanline: true, pixelGrid: false };
    expect(fxUniforms(fx)).toEqual({ uCrt: 1, uScanline: 1, uGrid: 0 });
  });
});

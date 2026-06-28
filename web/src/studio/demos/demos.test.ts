import { describe, it, expect } from "vitest";
import { DEMOS } from "./demos";

describe("DEMOS", () => {
  it("ships dusk-parallax and mode7-floor", () => {
    expect(DEMOS.map((d) => d.id)).toEqual(["dusk-parallax", "mode7-floor"]);
  });

  it("dusk-parallax carries sky/hills/hero with correct RGBA sizes", () => {
    const d = DEMOS.find((x) => x.id === "dusk-parallax")!;
    expect(d.assets.map((a) => a.id)).toEqual(["sky", "hills", "hero"]);
    const dims = Object.fromEntries(d.assets.map((a) => [a.id, [a.width, a.height]]));
    // sky/hills are full screen height so the BG layers don't tile vertically.
    expect(dims).toEqual({ sky: [256, 224], hills: [256, 224], hero: [64, 8] });
    for (const a of d.assets) expect(a.data.length).toBe(a.width * a.height * 4);
    expect(d.source).toContain('bg[1].source = "sky"');
    expect(d.source).toContain('obj.sheet = "hero"');
  });

  it("mode7-floor carries the track source and mode 7 lua", () => {
    const d = DEMOS.find((x) => x.id === "mode7-floor")!;
    expect(d.assets.map((a) => a.id)).toEqual(["track"]);
    expect(d.assets[0].width).toBe(64);
    expect(d.assets[0].height).toBe(64);
    expect(d.source).toContain("mode = 7");
    expect(d.source).toContain("hdma(96, 223");
  });

  it("sky is opaque above the horizon and transparent below it", () => {
    const sky = DEMOS[0].assets.find((a) => a.id === "sky")!;
    const alphaAt = (x: number, y: number) => sky.data[(y * sky.width + x) * 4 + 3];
    expect(alphaAt(0, 0)).toBe(255); // up top: opaque sky
    expect(alphaAt(0, 200)).toBe(0); // below the horizon: transparent (hills show)
  });
});

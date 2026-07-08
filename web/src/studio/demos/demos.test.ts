import { describe, it, expect } from "vitest";
import { DEMOS } from "./demos";

describe("DEMOS", () => {
  it("ships dusk-parallax, mode7-floor, offset-per-tile, and mode3-gradient", () => {
    expect(DEMOS.map((d) => d.id)).toEqual([
      "dusk-parallax",
      "mode7-floor",
      "offset-per-tile",
      "mode3-gradient",
    ]);
  });

  it("dusk-parallax carries sky/hills/hero with correct RGBA sizes", () => {
    const d = DEMOS.find((x) => x.id === "dusk-parallax")!;
    expect(d.assets.map((a) => a.id)).toEqual(["sky", "hills", "hero"]);
    const dims = Object.fromEntries(d.assets.map((a) => [a.id, [a.width, a.height]]));
    // sky/hills are full screen height so the BG layers don't tile vertically.
    expect(dims).toEqual({ sky: [256, 224], hills: [256, 224], hero: [64, 8] });
    for (const a of d.assets) expect(a.data.length).toBe(a.width * a.height * 4);
    expect(d.source).toContain('bg[1].source = "sky"');
    expect(d.source).toContain("bg[2].map_base = 0x0800");
    expect(d.source).toContain("bg[2].char_base = 0x4000");
    expect(d.source).toContain("obj.char_base = 0x6000");
    expect(d.source).toContain('obj.sheet = "hero"');
    expect(d.source).toContain("obj[0].prio = 3");
    expect(d.source).toContain("obj[0].pal = 0");
  });

  it("mode7-floor carries the track source and mode 7 lua", () => {
    const d = DEMOS.find((x) => x.id === "mode7-floor")!;
    expect(d.assets.map((a) => a.id)).toEqual(["track"]);
    expect(d.assets[0].width).toBe(1024);
    expect(d.assets[0].height).toBe(1024);
    expect(d.source).toContain('bg[1].source = "track"');
    expect(d.source).toContain("mode = 7");
    expect(d.source).toContain("m7.a, m7.d");
    expect(d.source).toContain("hdma(96, 223");
  });

  it("mode3-gradient carries the 8bpp gradient source and mode 3 lua", () => {
    const d = DEMOS.find((x) => x.id === "mode3-gradient")!;
    expect(d.assets.map((a) => a.id)).toEqual(["gradient"]);
    expect(d.assets[0].width).toBe(256);
    expect(d.assets[0].height).toBe(224);
    expect(d.assets[0].data.length).toBe(256 * 224 * 4);
    expect(d.source).toContain("mode = 3");
    expect(d.source).toContain('bg[1].source = "gradient"');
    // >16 distinct colours -> the whole point of the 8bpp path
    const colors = new Set<string>();
    const g = d.assets[0].data;
    for (let i = 0; i < g.length; i += 4) colors.add(`${g[i]},${g[i + 1]},${g[i + 2]}`);
    expect(colors.size).toBeGreaterThan(16);
  });

  it("sky is opaque above the horizon and transparent below it", () => {
    const sky = DEMOS[0].assets.find((a) => a.id === "sky")!;
    const alphaAt = (x: number, y: number) => sky.data[(y * sky.width + x) * 4 + 3];
    expect(alphaAt(0, 0)).toBe(255); // up top: opaque sky
    expect(alphaAt(0, 200)).toBe(0); // below the horizon: transparent (hills show)
  });

  it("track fills the full Mode 7 field with a repeating tile pattern", () => {
    const track = DEMOS[1].assets[0];
    const rgbAt = (x: number, y: number) => Array.from(track.data.slice((y * track.width + x) * 4, (y * track.width + x) * 4 + 4));
    expect(rgbAt(0, 0)).toEqual([0, 0, 0, 255]);
    expect(rgbAt(8, 0)).toEqual([32, 0, 255, 255]);
    expect(rgbAt(64, 0)).toEqual([0, 0, 0, 255]);
    expect(rgbAt(1023, 1023)).toEqual([224, 224, 0, 255]);
  });
});

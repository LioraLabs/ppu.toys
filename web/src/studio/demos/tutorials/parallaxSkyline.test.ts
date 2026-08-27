import { describe, it, expect } from "vitest";
import { parallaxSkyline } from "./parallaxSkyline";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("parallax-skyline (tutorial 2/10)", () => {
  it("carries the far/near skyline sources with correct RGBA sizes", () => {
    expect(parallaxSkyline.id).toBe("parallax-skyline");
    expect(parallaxSkyline.assets.map((a) => [a.id, a.kind])).toEqual([
      ["skyline_far", "bg"],
      ["skyline_near", "bg"],
    ]);
    for (const a of parallaxSkyline.assets) {
      // full screen size so the layers do NOT tile vertically
      expect([a.width, a.height]).toEqual([256, 224]);
      expect(a.data.length).toBe(a.width * a.height * 4);
      expect(a.options).toEqual({ bit_depth: 4 });
    }
  });

  it("ships pokes.lua + main.lua + skyline.lua, source = tab-order concat", () => {
    expect(parallaxSkyline.files!.map((f) => f.name)).toEqual([
      "pokes.lua",
      "main.lua",
      "skyline.lua",
    ]);
    expect(parallaxSkyline.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
    expect(parallaxSkyline.source).toBe(parallaxSkyline.files!.map((f) => f.source).join("\n"));
    // main.lua's enforced opening
    expect(parallaxSkyline.files![1].source).toContain("function frame(t, f)\n  apply_pokes()\n");
    // the shared-scope lesson: main.lua USES globals skyline.lua DEFINES
    expect(parallaxSkyline.files![1].source).toContain("t * FAR_SPEED");
    expect(parallaxSkyline.files![1].source).toContain("band_speed(y)");
    expect(parallaxSkyline.files![2].source).toContain("FAR_SPEED = 12");
    expect(parallaxSkyline.files![2].source).toContain("function band_speed(y)");
  });

  it("teaches the two-layer VRAM layout and the hdma scroll split", () => {
    const src = parallaxSkyline.source;
    expect(src).toContain('bg[1].source = "skyline_near"');
    expect(src).toContain('bg[2].source = "skyline_far"');
    // the second layer's own VRAM addresses
    expect(src).toContain("bg[2].map_base = 0x0800; bg[2].char_base = 0x4000");
    // per-scanline scroll = the parallax-strip trick
    expect(src).toContain("hdma(0, 223, function(y)");
    expect(src).toContain("bg[1].scroll.x = t * band_speed(y)");
    // the band table's split row matches the art's foreground strip
    expect(src).toContain("{ y = 168, speed = 90 }");
    expect(src).toContain("-- Try:");
  });

  it("draws both images from one identical 14-colour set (one 4bpp sub-palette)", () => {
    // Outside mode 0 every BG source lands its palettes at CGRAM 0 and the
    // second import lands on top: equal colour SETS fitting one sub-palette
    // (15 at 4bpp) make that overwrite a no-op. See tilesheet-cavern.
    const colours = (a: (typeof parallaxSkyline.assets)[number]) => {
      const out = new Set<string>();
      for (let i = 0; i < a.data.length; i += 4) {
        if (a.data[i + 3]) out.add(`${a.data[i]},${a.data[i + 1]},${a.data[i + 2]}`);
      }
      return out;
    };
    const [far, near] = parallaxSkyline.assets;
    const fc = colours(far);
    expect(fc.size).toBe(14);
    expect([...fc].sort()).toEqual([...colours(near)].sort());
  });

  it("far is opaque everywhere; near is transparent where the far layer shows", () => {
    const [far, near] = parallaxSkyline.assets;
    const alphaAt = (a: typeof far, x: number, y: number) => a.data[(y * a.width + x) * 4 + 3];
    expect(alphaAt(far, 20, 20)).toBe(255); // sky
    expect(alphaAt(far, 128, 220)).toBe(255); // even under the near strip
    expect(alphaAt(near, 20, 20)).toBe(0); // sky shows through
    expect(alphaAt(near, 27, 120)).toBe(0); // the gap slit between buildings
    expect(alphaAt(near, 100, 150)).toBe(255); // a mid building
    expect(alphaAt(near, 128, 220)).toBe(255); // the foreground strip
  });
});

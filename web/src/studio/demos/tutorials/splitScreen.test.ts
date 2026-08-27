import { describe, it, expect } from "vitest";
import { splitScreen } from "./splitScreen";
import { demoFiles } from "../kit";

describe("split-screen tutorial", () => {
  it("assembles pokes.lua + main.lua under the tutorial id", () => {
    expect(splitScreen.id).toBe("split-screen");
    expect(splitScreen.label).toBe("split-screen");
    expect(demoFiles(splitScreen).map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    const main = demoFiles(splitScreen)[1].source;
    expect(main).toContain("function frame(t, f)\n  apply_pokes()");
  });

  it("ships the city as a 4bpp BG import shaped for the top band", () => {
    expect(splitScreen.assets.map((a) => [a.id, a.kind])).toEqual([["city", "bg"]]);
    const [city] = splitScreen.assets;
    expect(city.options).toEqual({ bit_depth: 4 });
    expect([city.width, city.height]).toEqual([256, 224]);
    expect(city.data.length).toBe(256 * 224 * 4);
    // content only in the rows where mode 1 shows: opaque above the split,
    // transparent below (those rows are the mode 7 floor's).
    for (let x = 0; x < 256; x += 16) {
      expect(city.data[(0 * 256 + x) * 4 + 3]).toBe(255);
      expect(city.data[(111 * 256 + x) * 4 + 3]).toBe(255);
      expect(city.data[(112 * 256 + x) * 4 + 3]).toBe(0);
      expect(city.data[(223 * 256 + x) * 4 + 3]).toBe(0);
    }
    // one 4bpp sub-palette holds 15 colours; the skyline uses 9, and every
    // channel is a multiple of 8 so nothing collapses on the rgb15 grid.
    const colours = new Set<string>();
    for (let i = 0; i < city.data.length; i += 4) {
      if (city.data[i + 3]) colours.add(`${city.data[i]},${city.data[i + 1]},${city.data[i + 2]}`);
    }
    expect(colours.size).toBe(9);
    for (const c of colours) for (const ch of c.split(",")) expect(Number(ch) % 8).toBe(0);
  });

  // These lines ARE the lesson — a silent edit that guts them should fail.
  it("keeps the per-scanline mode split intact in the lua", () => {
    const src = splitScreen.source;
    // frame-wide default: mode 1 (also what the import binds under)
    expect(src).toContain("mode = 1; brightness = 15");
    expect(src).toContain('bg[1].source = "city"');
    // the city's tiles park above the mode 7 words
    expect(src).toContain("bg[1].char_base = 0x4000");
    // THE trick: `mode` assigned INSIDE the hdma hook, per-scanline
    expect(src).toContain("hdma(split, 223, function(y)");
    expect(src).toMatch(/hdma\(split, 223, function\(y\)\n\s+mode = 7/);
    // the perspective divide that sells the floor (cribbed from mode7-road)
    expect(src).toContain("local d = 64 / (y - (split - 1))");
    expect(src).toContain("m7.a, m7.d = d, d");
    // the floor is poked, not imported — an m7 import can't bind in a mode 1 frame
    expect(src).toContain("m7pixel(0, px, py, c)");
    // floor palette sits clear of the import's sub-palette 0 (shared CGRAM)
    expect(src).toContain("cgram[16]");
    expect(src).toContain("cgram[17]");
    expect(src).toContain("cgram[18]");
  });
});

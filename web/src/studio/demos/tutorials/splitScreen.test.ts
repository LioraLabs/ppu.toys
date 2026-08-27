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
    expect(splitScreen.assets.map((a) => [a.id, a.kind])).toEqual([
      ["city", "bg"],
      ["floor", "m7"],
    ]);
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
    // frame-wide default: mode 1
    expect(src).toContain("mode = 1; brightness = 15");
    // the setup stage: both worlds placed by dma, each by its payload's kind
    expect(src).toContain('local city = dma("city", { char = 0x4000, map = 0x7000 })');
    expect(src).toContain('dma("floor")');
    // the city's tiles park above the mode 7 words
    expect(src).toContain("bg[1].char_base = city.char");
    expect(src).toContain("bg[1].map_base = city.map");
    // THE trick: `mode` assigned INSIDE the hdma hook, per-scanline
    expect(src).toContain("hdma(split, 223, function(y)");
    expect(src).toMatch(/hdma\(split, 223, function\(y\)\n\s+mode = 7/);
    // the perspective divide that sells the floor (cribbed from mode7-road)
    expect(src).toContain("local d = 64 / (y - (split - 1))");
    expect(src).toContain("m7.a, m7.d = d, d");
  });

  it("paints the floor from three colours the city already owns (shared CGRAM 1..3)", () => {
    // The m7 palette lands at CGRAM 1.. and the city's 4bpp palette at 0..;
    // the render works because the floor's colour set is a subset of the
    // city's, chosen so both placements write the same values into 1..3
    // (tutorial_split_screen.rs pins the exact entries via the importers).
    const [city, floor] = splitScreen.assets;
    expect([floor.width, floor.height]).toEqual([1024, 1024]);
    expect(floor.options).toEqual({});
    const colours = (a: (typeof splitScreen.assets)[number]) => {
      const out = new Set<string>();
      for (let i = 0; i < a.data.length; i += 4) {
        if (a.data[i + 3]) out.add(`${a.data[i]},${a.data[i + 1]},${a.data[i + 2]}`);
      }
      return out;
    };
    const fc = colours(floor);
    expect([...fc].sort()).toEqual(["16,16,32", "224,104,72", "24,16,64"]);
    const cc = colours(city);
    for (const c of fc) expect(cc.has(c), `floor colour ${c} missing from the city`).toBe(true);
  });
});

import { describe, it, expect } from "vitest";
import { stageLights } from "./stageLights";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("stage-lights tutorial", () => {
  it("ships pokes.lua first and main.lua opening with the frame contract", () => {
    expect(stageLights.id).toBe("stage-lights");
    expect(stageLights.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
    const main = stageLights.files!.find((f) => f.name === "main.lua")!;
    expect(main.source).toContain("function frame(t, f)\n  apply_pokes()\n");
    expect(stageLights.source).toBe(stageLights.files!.map((f) => f.source).join("\n"));
  });

  it("carries one fully-opaque 4bpp stage scene that fits a single sub-palette", () => {
    expect(stageLights.assets.map((a) => [a.id, a.kind])).toEqual([["stage", "bg"]]);
    const a = stageLights.assets[0];
    expect([a.width, a.height]).toEqual([256, 224]);
    expect(a.options).toEqual({ bit_depth: 4 });
    expect(a.data.length).toBe(256 * 224 * 4);
    const colours = new Set<string>();
    for (let i = 0; i < a.data.length; i += 4) {
      expect(a.data[i + 3]).toBe(255); // opaque everywhere: math sees only BG1
      colours.add(`${a.data[i]},${a.data[i + 1]},${a.data[i + 2]}`);
    }
    expect(colours.size).toBe(9); // one 4bpp sub-palette holds 15
  });

  // The tutorial's load-bearing lines: both windows, the combine logic, the
  // colour-math block, and the per-scanline hdma. A silent edit that guts the
  // lesson should fail here.
  it("teaches both windows + combine logic + colour math via hdma", () => {
    const src = stageLights.source;
    // both windows feed the COLOUR window, folded by combine logic
    expect(src).toContain("win.color.w1 = true");
    expect(src).toContain("win.color.w2 = true");
    expect(src).toContain('win.color.combine = "XOR"');
    // colour math: fixed-colour subtract, only outside the combined window
    expect(src).toContain('color.op = "sub"');
    expect(src).toContain('color.addend = "fixed"');
    expect(src).toContain("color.fixed = rgb(96, 96, 136)");
    expect(src).toContain("color.on.bg1 = true");
    expect(src).toContain('color.region = "outside"');
    // the hdma writes BOTH spans per scanline, empty span included
    expect(src).toContain("hdma(0, 223");
    expect(src).toContain("win.w1.lo = cx1 - hw; win.w1.hi = cx1 + hw");
    expect(src).toContain("win.w2.lo = cx2 - hw; win.w2.hi = cx2 + hw");
    expect(src).toContain("win.w1.lo = 1; win.w1.hi = 0");
    expect(src).toContain("win.w2.lo = 1; win.w2.hi = 0");
    // power-on defaults handled explicitly, and the forkable Try block
    expect(src).toContain("screen.main.obj = false");
    expect(src).toContain("-- Try:");
  });
});

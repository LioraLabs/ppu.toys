import { describe, it, expect } from "vitest";
import { firstLight } from "./firstLight";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("first-light (tutorial 1/10)", () => {
  it("ships as an asset-free demo with the generated pokes.lua first", () => {
    expect(firstLight.id).toBe("first-light");
    expect(firstLight.assets).toEqual([]);
    expect(firstLight.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    expect(firstLight.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
    expect(firstLight.source).toBe(firstLight.files!.map((f) => f.source).join("\n"));
    const main = firstLight.files![1].source;
    expect(main).toContain("function frame(t, f)\n  apply_pokes()\n");
  });

  // The lesson lives in these lines — a silent edit that guts one of the four
  // concepts (register, backdrop, brightness, hdma) should fail loudly.
  it("keeps the load-bearing tutorial lines", () => {
    const src = firstLight.source;
    expect(src).toContain("A register is a little memory slot"); // defines "register" in passing
    expect(src).toContain("cgram[0] = hsl("); // the backdrop colour, drifting
    expect(src).toContain("brightness = 15");
    expect(src).toContain('color.op = "add"; color.addend = "fixed"; color.on.backdrop = true');
    expect(src).toContain("hdma(0, 223, function(y)"); // the first per-scanline hook
    expect(src).toContain("color.fixed = rgb("); // per-scanline COLDATA = the gradient
    expect(src).toMatch(/-- Try: .*\n$/); // ends with concrete tweaks to fork
  });
});

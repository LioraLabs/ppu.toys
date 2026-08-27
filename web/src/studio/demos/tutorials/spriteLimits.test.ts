import { describe, it, expect } from "vitest";
import { spriteLimits } from "./spriteLimits";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("sprite-limits (tutorial 10/10)", () => {
  it("ships as an asset-free demo with the generated pokes.lua first", () => {
    expect(spriteLimits.id).toBe("sprite-limits");
    expect(spriteLimits.assets).toEqual([]); // pokes solid OBJ tiles via vram[], no import
    expect(spriteLimits.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    expect(spriteLimits.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
    expect(spriteLimits.source).toBe(spriteLimits.files!.map((f) => f.source).join("\n"));
    const main = spriteLimits.files![1].source;
    expect(main).toContain("function frame(t, f)\n  apply_pokes()\n");
  });

  // The lesson lives in these lines — a silent edit that guts the range cap,
  // the time cap, the rotation toggle, or the inspector pointer should fail.
  it("keeps the load-bearing tutorial lines", () => {
    const src = spriteLimits.source;
    expect(src).toContain("local ROTATE = true"); // the toggle the reader flips
    expect(src).toContain("if ROTATE then obj.first = f % 58 else obj.first = 0 end");
    expect(src).toContain("obj[s].x = 4 + i * 6; obj[s].y = 96"); // the dense band placement
    expect(src).toContain("10 * 4 = 40 slivers > 34"); // the tile-slot arithmetic
    expect(src).toContain("obj[s].large = true"); // large sprites eat 4 slivers each
    expect(src).toContain("STAT77 ($213E)"); // the flags + Sprites inspector pointer
    expect(src).toContain("RANGE OVER and TIME OVER badges");
    expect(src).toMatch(/-- Try: .*\n.*\n.*\n$/); // ends with concrete tweaks to fork
  });
});

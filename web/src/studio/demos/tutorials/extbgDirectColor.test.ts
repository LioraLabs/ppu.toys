import { describe, it, expect } from "vitest";
import { extbgDirectColor } from "./extbgDirectColor";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("extbg-direct-color (tutorial 9/10)", () => {
  it("ships as an asset-free demo with the generated pokes.lua first", () => {
    expect(extbgDirectColor.id).toBe("extbg-direct-color");
    expect(extbgDirectColor.assets).toEqual([]); // the poked bytes ARE the scene
    expect(extbgDirectColor.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    expect(extbgDirectColor.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
    expect(extbgDirectColor.source).toBe(extbgDirectColor.files!.map((f) => f.source).join("\n"));
    const main = extbgDirectColor.files![1].source;
    expect(main).toContain("function frame(t, f)\n  apply_pokes()\n");
  });

  // The lesson lives in these lines — a silent edit that guts either deep cut
  // (bit-7 priority, CGRAM bypass) or their combination should fail loudly.
  it("keeps the load-bearing tutorial lines", () => {
    const src = extbgDirectColor.source;
    expect(src).toContain("m7.extbg = true"); // SETINI.6: bit 7 = per-pixel priority
    expect(src).toContain("direct_color = true"); // CGWSEL.0: the byte IS the colour
    expect(src).toContain("local LIT, SHADE, BEAM = 0x80 + 47, 0x80 + 21, 0x80 + 29"); // bit-7 HIGH pixels
    expect(src).toContain("obj[0].tile = 0; obj[0].pal = 0; obj[0].prio = 2"); // the sandwich slot
    expect(src).toContain("local idx = r + g * 8 + b * 64"); // the BGR233 packing formula
    expect(src).toContain("m7pixel("); // pixels poked, no assets
    expect(src).toMatch(/-- Try: .*\n/); // ends with concrete tweaks to fork
  });
});

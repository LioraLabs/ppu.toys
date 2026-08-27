import { describe, it, expect } from "vitest";
import { mode7Road, road } from "./mode7Road";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("mode7-road (tutorial 3/10)", () => {
  it("ships the road m7 source with the generated pokes.lua first", () => {
    expect(mode7Road.id).toBe("mode7-road");
    expect(mode7Road.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    expect(mode7Road.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
    expect(mode7Road.source).toBe(mode7Road.files!.map((f) => f.source).join("\n"));
    expect(mode7Road.files![1].source).toContain("function frame(t, f)\n  apply_pokes()\n");

    expect(mode7Road.assets.map((a) => [a.id, a.kind])).toEqual([["road", "m7"]]);
    const [a] = mode7Road.assets;
    expect([a.width, a.height]).toEqual([1024, 1024]);
    expect(a.data.length).toBe(1024 * 1024 * 4);
    expect(a.options).toEqual({});
  });

  it("paints a road, not a checkerboard", () => {
    const a = road();
    const texel = (x: number, y: number) => [
      ...a.data.slice((y * 1024 + x) * 4, (y * 1024 + x) * 4 + 3),
    ];
    expect(texel(128, 0)).toEqual([240, 208, 48]); // yellow dash on the centreline...
    expect(texel(128, 20)).toEqual([88, 88, 94]); // ...with asphalt in the gaps
    expect(texel(36, 0)).toEqual([232, 232, 232]); // white edge line
    expect(texel(20, 0)).toEqual([146, 108, 66]); // dirt shoulder
    expect(texel(0, 0)).toEqual([40, 116, 40]); // grass
  });

  // The lesson lives in these lines — a silent edit that guts the affine
  // registers, the perspective divide, or the drop-your-own-photo hook
  // should fail loudly.
  it("keeps the load-bearing tutorial lines", () => {
    const src = mode7Road.source;
    expect(src).toContain("mode = 7");
    expect(src).toContain('dma("road")'); // the setup-stage placement (and the swap-your-photo hook)
    expect(src).toContain("hdma(HORIZON + 1, 223, function(y)");
    expect(src).toContain("local d = SCALE / (y - HORIZON)"); // THE perspective divide
    expect(src).toContain("m7.a, m7.d = d, d");
    expect(src).toContain("m7.cx, m7.cy = 128, 0");
    expect(src).toContain("bg[1].scroll.y = (t * SPEED) / d"); // the driving
    expect(src).toContain("Drag ANY png"); // the namesake drop-your-photo hook
    expect(src).toMatch(/-- Try: .*\n$/); // ends with concrete tweaks to fork
  });
});

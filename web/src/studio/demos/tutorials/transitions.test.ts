import { describe, it, expect } from "vitest";
import { transitions } from "./transitions";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("transitions tutorial toy", () => {
  it("ships pokes.lua first and a main.lua opening with apply_pokes()", () => {
    expect(transitions.id).toBe("transitions");
    expect(transitions.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
    expect(transitions.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    expect(transitions.source).toContain("function frame(t, f)\n  apply_pokes()\n");
  });

  it("carries the 8bpp vista scene with the fine detail mosaic destroys", () => {
    expect(transitions.assets.map((a) => [a.id, a.kind])).toEqual([["vista", "bg"]]);
    const [v] = transitions.assets;
    expect([v.width, v.height]).toEqual([256, 224]);
    expect(v.options).toEqual({ bit_depth: 8 });
    expect(v.data.length).toBe(256 * 224 * 4);
    // >16 colours -> the reason for the 8bpp / mode 3 pairing
    const colours = new Set<string>();
    for (let i = 0; i < v.data.length; i += 4)
      colours.add(`${v.data[i]},${v.data[i + 1]},${v.data[i + 2]}`);
    expect(colours.size).toBeGreaterThan(16);
    // water row 141: a fleck at x=0, none at x=2 — the sub-block detail the
    // mosaic phases (and the Rust mirror's tests) rely on
    const rgb = (x: number, y: number) =>
      Array.from(v.data.slice((y * 256 + x) * 4, (y * 256 + x) * 4 + 3));
    expect(rgb(0, 141)).not.toEqual(rgb(2, 141));
  });

  // The lua IS the tutorial — these lines are the lesson, so a silent edit
  // that guts one of the four transitions should fail here.
  it("ships the full transition schedule", () => {
    const s = transitions.source;
    expect(s).toContain("local CYCLE = 12"); // the loop
    expect(s).toContain("local tc = t % CYCLE"); // t is unbounded: modulo everything
    expect(s).toContain("local phase = floor(tc / 2)"); // six 2s phases
    expect(s).toContain("hdma(0, 223"); // the per-scanline wipe...
    expect(s).toContain("brightness = min(15, max(0, floor((edge - y) / 2)))"); // ...its ramp
    expect(s).toContain("brightness = floor(15 * abs(u - 1))"); // the plain fade
    expect(s).toContain("bg[1].mosaic = true"); // per-layer mosaic enable
    expect(s).toContain("mosaic = floor(15 * (1 - abs(u - 1)))"); // the mosaic fade
    expect(s).toContain("brightness = 15 - floor(11 * k)"); // the combo dim
    expect(s).toContain("force_blank = true"); // the hard cut
  });
});

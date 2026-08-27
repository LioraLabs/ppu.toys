import { describe, it, expect } from "vitest";
import { spriteParade } from "./spriteParade";
import { EMPTY_POKES } from "../../pokes/pokes";

describe("sprite-parade tutorial", () => {
  it("ships the demo shape: pokes.lua first, one main.lua, two assets", () => {
    expect(spriteParade.id).toBe("sprite-parade");
    expect(spriteParade.label).toBe("sprite-parade");
    expect(spriteParade.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    expect(spriteParade.files![0].source).toBe(EMPTY_POKES);
    expect(spriteParade.source).toBe(spriteParade.files!.map((f) => f.source).join("\n"));
    expect(spriteParade.assets.map((a) => [a.id, a.kind])).toEqual([
      ["parade", "obj"],
      ["street", "bg"],
    ]);
  });

  it("commits asset formats and dimensions the Lua relies on", () => {
    const [parade, street] = spriteParade.assets;
    // 16 cells wide is load-bearing: the OBJ name table is 16 tiles wide, so a
    // 2x2 block of sheet cells is one large 16x16 sprite (tiles +1 / +16).
    expect([parade.width, parade.height]).toEqual([128, 16]);
    expect(parade.options).toEqual({ cell_size: 8 });
    expect([street.width, street.height]).toEqual([256, 224]);
    expect(street.options).toEqual({ bit_depth: 4 });
    for (const a of spriteParade.assets) expect(a.data.length).toBe(a.width * a.height * 4);
  });

  // The whole tile-numbering scheme rests on two facts about the sheet: cell 0
  // is blank (it dedups onto the importer's reserved blank tile 0) and every
  // used cell is unique even under flips (the importer dedups flipped tiles).
  // Only then does sheet cell N = OBJ tile N, which the Lua's tile numbers
  // (and the 2x2 large-sprite blocks at 4/20 and 6/22) hard-code.
  // crates/ppu-core/tests/tutorial_sprite_parade.rs pins the same invariant
  // through the real importer.
  it("keeps cell 0 blank and cells 1..23 unique under flips", () => {
    const a = spriteParade.assets[0];
    const cell = (n: number): string => {
      const ox = (n % 16) * 8,
        oy = Math.floor(n / 16) * 8;
      let s = "";
      for (let y = 0; y < 8; y++)
        for (let x = 0; x < 8; x++) {
          const i = ((oy + y) * a.width + ox + x) * 4;
          s += a.data[i + 3] ? `#${a.data[i]},${a.data[i + 1]},${a.data[i + 2]};` : "_;";
        }
      return s;
    };
    const grid = (s: string) => s.split(";").slice(0, 64);
    const flipX = (g: string[]) => {
      const o: string[] = [];
      for (let y = 0; y < 8; y++) o.push(...g.slice(y * 8, y * 8 + 8).reverse());
      return o;
    };
    const flipY = (g: string[]) => {
      const o: string[] = [];
      for (let y = 7; y >= 0; y--) o.push(...g.slice(y * 8, y * 8 + 8));
      return o;
    };
    const canon = (s: string) => {
      const g = grid(s);
      return [g, flipX(g), flipY(g), flipX(flipY(g))].map((v) => v.join(";")).sort()[0];
    };
    const blank = canon(cell(0));
    expect(cell(0).includes("#")).toBe(false); // cell 0 fully transparent
    const seen = new Set([blank]);
    for (let n = 1; n <= 23; n++) {
      const c = canon(cell(n));
      expect(c, `cell ${n} is blank or collides under flips`).not.toBe(blank);
      expect(seen.has(c), `cell ${n} duplicates an earlier cell`).toBe(false);
      seen.add(c);
    }
    for (let n = 24; n < 32; n++) expect(cell(n).includes("#")).toBe(false);
  });

  it("fits one OBJ sub-palette so palette 1 stays free for the gold marcher", () => {
    const a = spriteParade.assets[0];
    const colours = new Set<string>();
    for (let i = 0; i < a.data.length; i += 4)
      if (a.data[i + 3]) colours.add(`${a.data[i]},${a.data[i + 1]},${a.data[i + 2]}`);
    expect(colours.size).toBe(14); // <= 15: one 4bpp sub-palette holds them all
  });

  // The tutorial's load-bearing lines: this demo is reference code people fork,
  // so a silent edit that guts a lesson should fail here.
  it("keeps the lesson lines in the Lua", () => {
    const src = spriteParade.files![1].source;
    expect(src.startsWith("-- ppu.toys :: sprite-parade")).toBe(true);
    expect(src).toContain("function frame(t, f)\n  apply_pokes()");
    expect(src).toContain('obj.sheet = "parade"'); // 1. bind the sheet
    expect(src).toContain("obj.char_base = 0x6000");
    expect(src).toContain("obj.size_sel = 0"); // 2. the small/large pair
    expect(src).toContain("obj[0].large = true"); // ...and a sprite going large
    expect(src).toContain("obj[4].flip_x = true"); // 4. flipping
    expect(src).toContain("cgram[128 + 16 + i]"); // 5. OBJ palettes at 128+
    expect(src).toContain("obj[5].pal = 1");
    expect(src).toContain("obj[7].prio = 0"); // 6. behind the fence...
    expect(src).toContain("obj[8].prio = 3"); // ...and in front of it
  });
});

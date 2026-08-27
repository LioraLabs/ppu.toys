import { describe, it, expect } from "vitest";
import { cavernCamera } from "./cavernCamera";

describe("cavern-camera (tutorial 5/10)", () => {
  it("carries one small sheet source on a single 4bpp sub-palette", () => {
    expect(cavernCamera.id).toBe("cavern-camera");
    expect(cavernCamera.assets.map((a) => [a.id, a.kind])).toEqual([["cave_tiles", "sheet"]]);
    const [tiles] = cavernCamera.assets;
    expect(tiles.options).toEqual({ bit_depth: 4 });
    expect([tiles.width, tiles.height]).toEqual([64, 8]); // 8 cells of 8x8
    expect(tiles.data.length).toBe(64 * 8 * 4);

    // Cell 0 is blank on purpose: a sheet reserves no blank tile, so the
    // author leaves one for the level's air.
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) expect(tiles.data[(y * 64 + x) * 4 + 3]).toBe(0);
    }

    // Everything fits ONE 4bpp sub-palette (15 colours + transparent).
    const colours = new Set<string>();
    for (let i = 0; i < tiles.data.length; i += 4) {
      if (tiles.data[i + 3])
        colours.add(`${tiles.data[i]},${tiles.data[i + 1]},${tiles.data[i + 2]}`);
    }
    expect(colours.size).toBe(12);
    expect(colours.size).toBeLessThanOrEqual(15);
  });

  // These lines ARE the lesson — an edit that guts them should fail loudly.
  it("ships the tilesheet-workflow lines the toy teaches", () => {
    const s = cavernCamera.source;
    // step 1: dma the sheet in (chars + palette, no map); step 2: explicit
    // map geometry — placement writes nothing back, the registers are yours
    expect(s).toContain('local sheet = dma("cave_tiles", { char = 0x1000 })');
    expect(s).toContain("bg[1].char_base = sheet.char");
    expect(s).toContain("bg[1].map_base = 0x0000");
    expect(s).toContain("bg[1].screen_size = 1");
    // step 3: one-line map entries (the editor's completion contract)
    expect(s).toContain("bg[1].map[col][MAP_TOP + row - 1] = { tile = tile, pal = 0 }");
    expect(s).toContain("if bg[1].map[col] == nil then bg[1].map[col] = {} end");
    // the string-legend parser and the lava cycle
    expect(s).toContain("local ch = string.sub(LEVEL[row], col + 1, col + 1)");
    expect(s).toContain("tile = LAVA_FIRST + phase % LAVA_FRAMES");
    // backdrop + the ping-pong camera
    expect(s).toContain("cgram[0] = rgb(16, 24, 40)");
    expect(s).toContain("bg[1].scroll.x = m");
    // the graduation pointer to the streaming/Tiled workflow
    expect(s).toContain("tilesheet-cavern");
  });

  it("authors a rectangular hand-typed level that fits the tilemap whole", () => {
    const body = /local LEVEL = \{\n([\s\S]*?)\n\}/.exec(cavernCamera.source)![1];
    const rows = [...body.matchAll(/"([.#%=~*]+)"/g)].map((m) => m[1]);
    expect(rows).toHaveLength(14); // 64x~14 map writes per frame stays light
    for (const r of rows) expect(r).toHaveLength(48); // <= 64 tiles: no streaming
    expect(rows[0]).toBe("#".repeat(48)); // solid ceiling row
    expect(rows[11]).toContain("~"); // the lava pools sit in the floor row
    expect(rows.some((r) => r.includes("="))).toBe(true); // a platform or two
  });

  it("prepends pokes.lua and opens frame() with apply_pokes()", () => {
    expect(cavernCamera.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    expect(cavernCamera.files![1].source).toContain("function frame(t, f)\n  apply_pokes()\n");
    expect(cavernCamera.source).toBe(cavernCamera.files!.map((f) => f.source).join("\n"));
  });
});

import { describe, it, expect } from "vitest";
import { DEMOS, demoFiles } from "./demos";
import { EMPTY_POKES } from "../pokes/pokes";

describe("DEMOS", () => {
  it("ships all bundled demos in order", () => {
    expect(DEMOS.map((d) => d.id)).toEqual([
      "dusk-parallax",
      "mode7-floor",
      "offset-per-tile",
      "mode3-gradient",
      "translucency",
      "spotlight",
      "glow",
      "sprite-storm",
      "mosaic",
      "mode7-extbg",
      "direct-color",
      "tilesheet-cavern",
    ]);
  });

  // PPU-95: the tilesheet demo binds BOTH source kinds at once, and their
  // palettes have to be the same set — mode 1 places every BG source's palettes
  // at CGRAM 0, so BG2's import lands on top of BG1's.
  it("tilesheet-cavern carries a sheet and an assembled source over one shared palette", () => {
    const d = DEMOS.find((x) => x.id === "tilesheet-cavern")!;
    expect(d.assets.map((a) => [a.id, a.kind])).toEqual([
      ["cavern_tiles", "sheet"],
      ["cavern_back", "bg"],
    ]);
    const [tiles, back] = d.assets;
    expect([tiles.width, tiles.height]).toEqual([64, 24]); // 8x3 cells of 8x8
    expect([back.width, back.height]).toEqual([256, 224]);
    for (const a of d.assets) expect(a.data.length).toBe(a.width * a.height * 4);

    // Cell 0 is blank on purpose: a sheet reserves no blank tile, so Tiled's
    // empty cell (gid 0 -> tile 0) needs the author to leave one.
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) expect(tiles.data[(y * 64 + x) * 4 + 3]).toBe(0);
    }

    const colours = (a: (typeof d.assets)[number]) => {
      const out = new Set<string>();
      for (let i = 0; i < a.data.length; i += 4) {
        if (a.data[i + 3]) out.add(`${a.data[i]},${a.data[i + 1]},${a.data[i + 2]}`);
      }
      return out;
    };
    const cs = colours(tiles);
    expect(cs.size).toBe(14); // one 4bpp sub-palette holds 15
    expect([...cs].sort()).toEqual([...colours(back)].sort());
  });

  // PPU-95: the level table is a Tiled Lua export and `gid - 1` is the adapter,
  // so a gid past the tileset's tilecount would silently draw the wrong cell.
  it("tilesheet-cavern's level table is a well-formed Tiled export", () => {
    const d = DEMOS.find((x) => x.id === "tilesheet-cavern")!;
    expect(d.source).toContain('type = "tilelayer"');
    expect(d.source).toContain('encoding = "lua"');
    expect(d.source).toContain("firstgid = 1");
    expect(d.source).toContain("tilecount = 24");
    const body = /data = \{([\s\S]*?)\n {6}\},/.exec(d.source)![1];
    const gids = body
      .split(",")
      .map((g) => g.trim())
      .filter(Boolean)
      .map(Number);
    expect(gids).toHaveLength(96 * 12); // width * height, row-major
    expect(gids.every((g) => Number.isInteger(g) && g >= 0 && g <= 24)).toBe(true);
    expect(gids).toContain(0); // Tiled's empty cell
  });

  // PPU-95: the demo is reference code people copy — these lines ARE the
  // workflow it documents, so a silent edit that guts them should fail.
  it("tilesheet-cavern's lua ships the streaming-camera workflow", () => {
    const d = DEMOS.find((x) => x.id === "tilesheet-cavern")!;
    // a sheet echoes back only char_base, so the map geometry is set explicitly
    expect(d.source).toContain('bg[1].source = "cavern_tiles"');
    expect(d.source).toContain("bg[1].char_base = 0x1000");
    expect(d.source).toContain("bg[1].map_base = 0x0000");
    expect(d.source).toContain("bg[1].screen_size = 1");
    expect(d.source).toContain("return gid - 1"); // the whole Tiled adapter
    expect(d.source).toContain("local mcol = (cam_tile + s) % MAP_COLS"); // the ring
    expect(d.source).toContain("bg[1].scroll.x = cam % 512"); // fine scroll
    expect(d.source).toContain("tile = anim.first + phase % anim.frames"); // animation
    // the assembled layer alongside, on plain scroll
    expect(d.source).toContain('bg[2].source = "cavern_back"');
    expect(d.source).toContain("bg[2].scroll.x = cam / 3");
    // map entries stay on ONE line: a constructor split across lines falls out
    // of the editor's map-entry completion scope.
    expect(d.source).toContain("bg[1].map[mcol][MAP_TOP + r] = { tile = tile, pal = 0 }");
  });

  it("dusk-parallax carries sky/hills/hero with correct RGBA sizes", () => {
    const d = DEMOS.find((x) => x.id === "dusk-parallax")!;
    expect(d.assets.map((a) => a.id)).toEqual(["sky", "hills", "hero"]);
    const dims = Object.fromEntries(d.assets.map((a) => [a.id, [a.width, a.height]]));
    // sky/hills are full screen height so the BG layers don't tile vertically.
    expect(dims).toEqual({ sky: [256, 224], hills: [256, 224], hero: [64, 8] });
    for (const a of d.assets) expect(a.data.length).toBe(a.width * a.height * 4);
    expect(d.source).toContain('bg[1].source = "sky"');
    expect(d.source).toContain("bg[2].map_base = 0x0800");
    expect(d.source).toContain("bg[2].char_base = 0x4000");
    expect(d.source).toContain("obj.char_base = 0x6000");
    expect(d.source).toContain('obj.sheet = "hero"');
    expect(d.source).toContain("obj[0].prio = 3");
    expect(d.source).toContain("obj[0].pal = 0");
    expect(d.files!.map((f) => f.name)).toEqual(["pokes.lua", "main.lua", "palette.lua"]);
    expect(d.source).toBe(d.files!.map((f) => f.source).join("\n"));
    expect(d.files![1].source).toContain("dusk_palette(t)");
    expect(d.files![1].source).toContain("apply_pokes()");
    expect(d.files![2].source).toContain("function dusk_palette");
    expect(d.files![2].source).toContain("SPEED = 12");
  });

  it("every demo ships a generated pokes.lua first, main.lua calling apply_pokes()", () => {
    for (const d of DEMOS) {
      expect(d.files![0]).toEqual({ name: "pokes.lua", source: EMPTY_POKES });
      const main = d.files!.find((f) => f.name === "main.lua")!;
      expect(main.source).toContain("function frame(t, f)\n  apply_pokes()\n");
    }
  });

  it("demoFiles presents a demo's ordered files, pokes.lua always first", () => {
    const single = DEMOS.find((x) => x.id === "mode7-floor")!;
    expect(demoFiles(single)).toBe(single.files!);
    expect(demoFiles(single).map((f) => f.name)).toEqual(["pokes.lua", "main.lua"]);
    const multi = DEMOS.find((x) => x.id === "dusk-parallax")!;
    expect(demoFiles(multi)).toBe(multi.files!);
  });

  it("mode7-floor carries the track source and mode 7 lua", () => {
    const d = DEMOS.find((x) => x.id === "mode7-floor")!;
    expect(d.assets.map((a) => a.id)).toEqual(["track"]);
    expect(d.assets[0].width).toBe(1024);
    expect(d.assets[0].height).toBe(1024);
    expect(d.source).toContain('bg[1].source = "track"');
    expect(d.source).toContain("mode = 7");
    expect(d.source).toContain("m7.a, m7.d");
    expect(d.source).toContain("hdma(96, 223");
  });

  it("mode3-gradient carries the 8bpp gradient source and mode 3 lua", () => {
    const d = DEMOS.find((x) => x.id === "mode3-gradient")!;
    expect(d.assets.map((a) => a.id)).toEqual(["gradient"]);
    expect(d.assets[0].width).toBe(256);
    expect(d.assets[0].height).toBe(224);
    expect(d.assets[0].data.length).toBe(256 * 224 * 4);
    expect(d.source).toContain("mode = 3");
    expect(d.source).toContain('bg[1].source = "gradient"');
    // >16 distinct colours -> the whole point of the 8bpp path
    const colors = new Set<string>();
    const g = d.assets[0].data;
    for (let i = 0; i < g.length; i += 4) colors.add(`${g[i]},${g[i + 1]},${g[i + 2]}`);
    expect(colors.size).toBeGreaterThan(16);
  });

  it("sky is opaque above the horizon and transparent below it", () => {
    const sky = DEMOS[0].assets.find((a) => a.id === "sky")!;
    const alphaAt = (x: number, y: number) => sky.data[(y * sky.width + x) * 4 + 3];
    expect(alphaAt(0, 0)).toBe(255); // up top: opaque sky
    expect(alphaAt(0, 200)).toBe(0); // below the horizon: transparent (hills show)
  });

  it("track fills the full Mode 7 field with a repeating tile pattern", () => {
    const track = DEMOS[1].assets[0];
    const rgbAt = (x: number, y: number) =>
      Array.from(track.data.slice((y * track.width + x) * 4, (y * track.width + x) * 4 + 4));
    expect(rgbAt(0, 0)).toEqual([0, 0, 0, 255]);
    expect(rgbAt(8, 0)).toEqual([32, 0, 255, 255]);
    expect(rgbAt(64, 0)).toEqual([0, 0, 0, 255]);
    expect(rgbAt(1023, 1023)).toEqual([224, 224, 0, 255]);
  });

  it("m6 demos carry their colour-math lua and assets", () => {
    const t = DEMOS.find((d) => d.id === "translucency")!;
    expect(t.assets.map((a) => a.id)).toEqual(["panel", "ribbons"]);
    expect(t.source).toContain('color.op = "add"; color.half = true; color.on.bg1 = true');
    expect(t.source).toContain("screen.sub.bg2 = true");
    expect(t.source).toContain('color.addend = "sub"');
    const s = DEMOS.find((d) => d.id === "spotlight")!;
    expect(s.source).toContain("CGWSEL = 0x40");
    expect(s.source).toContain("hdma(0, 223");
    expect(s.source).toContain("win.color.w1 = true");
    expect(s.source).toContain("win.w1.lo = cx - hw");
    const g = DEMOS.find((d) => d.id === "glow")!;
    expect(g.source).toContain('color.op = "add"; color.on.bg1 = true');
    expect(g.source).toContain('color.addend = "fixed"');
    expect(g.source).toContain("color.fixed = rgb(120, 60, 0)");
    for (const a of t.assets) expect(a.data.length).toBe(a.width * a.height * 4);
  });

  it("sprite-storm packs a rotating over-limit OBJ band with no image asset", () => {
    const d = DEMOS.find((x) => x.id === "sprite-storm")!;
    expect(d.assets).toEqual([]); // pokes solid OBJ tiles via vram[], no import
    expect(d.source).toContain("obj.size_sel = 7"); // 16x32 non-square path
    expect(d.source).toContain("obj.first = f % N"); // OAM-start rotation -> flicker
    expect(d.source).toContain("obj[i].large = (i % 12 == 0)");
  });

  it("mosaic carries the ramp asset and animates the block size", () => {
    const d = DEMOS.find((x) => x.id === "mosaic")!;
    expect(d.assets.map((a) => a.id)).toEqual(["ramp"]);
    expect(d.assets[0].width).toBe(256);
    expect(d.assets[0].height).toBe(224);
    expect(d.assets[0].data.length).toBe(256 * 224 * 4);
    expect(d.source).toContain("bg[1].mosaic = true");
    expect(d.source).toContain("mosaic = floor(f / 8) % 16");
  });

  it("mode7-extbg pokes a split-priority floor + a between-layers sprite, no asset", () => {
    const d = DEMOS.find((x) => x.id === "mode7-extbg")!;
    expect(d.assets).toEqual([]); // authored via m7pixel / m7.map / vram[], no import
    expect(d.source).toContain("m7.extbg = true");
    expect(d.source).toContain("m7pixel(1, fx, fy, 0x81)"); // high-priority floor pixel
    expect(d.source).toContain("obj[0].prio = 2"); // sprite sits between the floor levels
  });

  it("direct-color builds an 8bpp Mode 7 field via the CGRAM bypass, no asset", () => {
    const d = DEMOS.find((x) => x.id === "direct-color")!;
    expect(d.assets).toEqual([]); // indices poked directly, colour = index (no palette)
    expect(d.source).toContain("direct_color = true");
    expect(d.source).toContain("m7pixel(idx, fx, fy, idx)");
  });
});

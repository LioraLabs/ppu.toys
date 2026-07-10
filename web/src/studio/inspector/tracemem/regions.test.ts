import { describe, expect, it } from "vitest";
import { cgramOwners, vramRegions } from "./regions";
import type { OamSprite, RegisterView } from "../../../ppu/core";

function regs(entries: Record<string, number>): RegisterView[] {
  return Object.entries(entries).map(([name, value], i) => ({ addr: i, name, value, changed: false }));
}
const sprite = (over: Partial<OamSprite>): OamSprite => ({
  x: 0, y: 0, tile: 0, pal: 0, prio: 0, large: false, flipX: false, flipY: false, on: true,
  ...over,
});
const noOam: OamSprite[] = [];

describe("vramRegions", () => {
  it("derives mode-1 BG regions from live binding registers + map scan", () => {
    const vram = new Uint16Array(0x8000);
    vram[0x1000] = 5; // BG1 map: max tile index 5 -> 6 tiles
    const r = vramRegions(
      regs({ BGMODE: 0x01, BG1SC: 0x10, BG2SC: 0x20, BG3SC: 0x30, BG12NBA: 0x42, BG34NBA: 0x06, OBSEL: 0x00 }),
      vram,
    );
    const bg1map = r.find((x) => x.id === "bg1-map")!;
    expect(bg1map.start).toBe(0x1000); // (0x10>>2)<<10
    expect(bg1map.end).toBe(0x1400); // 32x32 -> 0x400 words
    expect(bg1map.usage).toBe("32×32 map");
    const bg1char = r.find((x) => x.id === "bg1-char")!;
    expect(bg1char.start).toBe(0x2000); // BG12NBA low nibble 2
    expect(bg1char.end).toBe(0x2000 + 6 * 16); // 6 tiles x 16 words (4bpp)
    expect(bg1char.usage).toBe("6 tiles");
    const bg2char = r.find((x) => x.id === "bg2-char")!;
    expect(bg2char.start).toBe(0x4000); // BG12NBA high nibble 4
    const bg3char = r.find((x) => x.id === "bg3-char")!;
    expect(bg3char.start).toBe(0x6000); // BG34NBA low nibble 6
    expect(bg3char.end).toBe(0x6000 + 1 * 8); // empty map -> 1 tile, 2bpp = 8 words
    expect(r.find((x) => x.id === "bg4-map")).toBeUndefined(); // BG4 absent in mode 1
  });

  it("honors screen size and 16px-tile char extents", () => {
    const vram = new Uint16Array(0x8000);
    const r = vramRegions(
      regs({ BGMODE: 0x11, BG1SC: 0x03, BG12NBA: 0x00, OBSEL: 0x00 }), // size 3 + BG1 16px tiles
      vram,
    );
    const bg1map = r.find((x) => x.id === "bg1-map")!;
    expect(bg1map.end - bg1map.start).toBe(0x1000); // 64x64
    expect(bg1map.usage).toBe("64×64 map");
    const bg1char = r.find((x) => x.id === "bg1-char")!;
    expect(bg1char.end).toBe((0 + 17 + 1) * 16); // 16px tile spans +17 char names
  });

  it("derives both OBJ tables from OBSEL", () => {
    const r = vramRegions(regs({ BGMODE: 0x01, OBSEL: 0x0b }), new Uint16Array(0x8000));
    const a = r.find((x) => x.id === "obj-a")!;
    const b = r.find((x) => x.id === "obj-b")!;
    expect(a.start).toBe(0x6000); // (0x0b & 7) << 13
    expect(a.end).toBe(0x7000); // 256 tiles x 16 words
    expect(b.start).toBe(0x6000 + 2 * 0x1000); // name_select 1 -> gap (1+1)<<12
    expect(b.end).toBe(0x8000); // clamped
  });

  it("mode 7 is one interleaved region + OBJ", () => {
    const r = vramRegions(regs({ BGMODE: 0x07, OBSEL: 0x00 }), new Uint16Array(0x8000));
    const m7 = r.find((x) => x.id === "m7")!;
    expect([m7.start, m7.end]).toEqual([0, 0x4000]);
    expect(r.some((x) => x.id === "obj-a")).toBe(true);
    expect(r.some((x) => x.id.startsWith("bg"))).toBe(false);
  });

  it("is sorted by start address", () => {
    const r = vramRegions(regs({ BGMODE: 0x01, BG1SC: 0x40, OBSEL: 0x00 }), new Uint16Array(0x8000));
    const starts = r.map((x) => x.start);
    expect(starts).toEqual([...starts].sort((a, b) => a - b));
  });
});

describe("cgramOwners", () => {
  it("labels BG rows from live tilemap palette bits (mode 1, 4bpp)", () => {
    const vram = new Uint16Array(0x8000);
    vram[0x0000] = 2 << 10; // BG1 map at 0 uses pal 2 -> CGRAM 32..47 -> row 2
    const o = cgramOwners(regs({ BGMODE: 0x01, BG1SC: 0x00, BG12NBA: 0x00, OBSEL: 0x00 }), vram, noOam);
    expect(o[2].label).toContain("BG1");
    expect(o[2].used).toBe(true);
    expect(o[3].label).toBe("—"); // pal 3 unused
    expect(o[3].used).toBe(false);
  });

  it("applies the mode-0 per-BG 32-color band", () => {
    const vram = new Uint16Array(0x8000);
    // all four maps at base 0, all entries pal 0; BG2 band = 1*32 -> row 2
    const o = cgramOwners(regs({ BGMODE: 0x00, BG1SC: 0x00, BG2SC: 0x00, BG3SC: 0x00, BG4SC: 0x00, OBSEL: 0x00 }), vram, noOam);
    expect(o[2].label).toContain("BG2");
    expect(o[4].label).toContain("BG3");
    expect(o[6].label).toContain("BG4");
  });

  it("8bpp BG1 owns the whole table", () => {
    const o = cgramOwners(regs({ BGMODE: 0x03, BG1SC: 0x00, OBSEL: 0x00 }), new Uint16Array(0x8000), noOam);
    expect(o[0].label).toContain("BG1");
    expect(o[15].label).toContain("BG1"); // 8bpp reads all 256 entries
    expect(o[15].label).toContain("OBJ");
    expect(o[15].used).toBe(true); // a BG owner marks the row used
  });

  it("OBJ rows track live OAM palette usage", () => {
    const oam = [sprite({ pal: 3, on: true }), sprite({ pal: 5, on: false })];
    const o = cgramOwners(regs({ BGMODE: 0x01, BG1SC: 0x00, BG12NBA: 0x00, OBSEL: 0x00 }), new Uint16Array(0x8000), oam);
    expect(o[8 + 3].used).toBe(true); // on-sprite pal 3
    expect(o[8 + 5].used).toBe(false); // that sprite is off; no BG owner up here in mode 1
  });
});

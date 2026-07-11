import { describe, it, expect } from "vitest";
import { buildPreviewModel } from "./preview";
import type { SourceMeta } from "../../ppu/core";

const bgMeta = (): SourceMeta => ({ width: 16, height: 8, report: { mode: "tile", report: { colors_used: 4, palettes_used: 1, tile_cells: 2, unique_tiles: 2, vram_words: 40, overflows: [] } } });

describe("buildPreviewModel", () => {
  it("bg: 2x1 grid, each cell labeled char#/pal from decoded tilemap", () => {
    // craft a bg payload with 2 tiles, tilemap[0]=tile0/pal0, [1]=tile1/pal2
    const u16 = (v: number) => [v & 0xff, v >> 8 & 0xff];
    const tm = new Array(0x400).fill(0); tm[0] = 0x0000; tm[1] = 0x0801; // (1,0): tile1 pal2
    const payload = new Uint8Array([
      1, 0, 2, 8, 1, 1, ...u16(0x1f), ...u16(2),
      ...new Array(16).fill(0).flatMap(() => u16(0)), // 2 tiles * 8 words
      0, ...tm.flatMap(u16),
    ]);
    const m = buildPreviewModel("bg", bgMeta(), payload);
    expect(m.cols).toBe(2);
    expect(m.rows).toBe(1);
    expect(m.cells).toHaveLength(2);
    expect(m.cells[0]).toMatchObject({ top: "t0", bot: "p0" });
    expect(m.cells[1]).toMatchObject({ top: "t1", bot: "p2" });
    expect(m.budget).toContain("2/1024 tiles");
    expect(m.image).not.toBeNull(); // quantized RGBA present
  });

  it("obj: cell grid from meta.cells, labels tile#/pal", () => {
    const meta: SourceMeta = { width: 32, height: 16, report: { mode: "obj", report: { colors_used: 8, palettes_used: 1, tile_cells: 8, unique_tiles: 8, vram_words: 128, overflows: [] } }, cells: [ { tile: 5, pal: 1, flip_x: false, flip_y: false }, { tile: 9, pal: 3, flip_x: false, flip_y: false } ] };
    const m = buildPreviewModel("obj", meta, new Uint8Array([1]), 16);
    expect(m.cols).toBe(2); // 32 / 16
    expect(m.rows).toBe(1);
    expect(m.cells[0]).toMatchObject({ top: "t5", bot: "p1" });
    expect(m.cells[1]).toMatchObject({ top: "t9", bot: "p3" });
  });

  it("degrades when payload undecodable (mock stub): grid by dims, index labels, no image", () => {
    const m = buildPreviewModel("bg", bgMeta(), new Uint8Array([1]));
    expect(m.cols).toBe(2);
    expect(m.cells[0].top).toBe("#0");
    expect(m.image).toBeNull();
  });
});

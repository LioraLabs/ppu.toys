import { describe, it, expect } from "vitest";
import { sourceReportView } from "./report";
import type { SourceReport } from "../../ppu/core";

describe("sourceReportView", () => {
  it("bg tile report: tiles/1024, sub-palettes/8, colors, no warns", () => {
    const r: SourceReport = { mode: "tile", report: { colors_used: 12, palettes_used: 3, tile_cells: 40, unique_tiles: 40, vram_words: 700, overflows: [] } };
    const v = sourceReportView(r);
    expect(v.budget).toEqual(["40/1024 tiles", "3/8 sub-palettes", "12 colors"]);
    expect(v.warns).toEqual([]);
  });

  it("m7 report: tiles/256, colors/256, overflow warn", () => {
    const r: SourceReport = { mode: "m7", report: { colors: 40, unique_tiles: 300, tile_capacity: 256, overflow_tiles: 44, map_tiles_w: 20, map_tiles_h: 15 } };
    const v = sourceReportView(r);
    expect(v.budget).toEqual(["300/256 tiles", "40/256 colors", "20×15 map"]);
    expect(v.warns).toEqual(["44 tiles over capacity"]);
  });

  it("obj report: tiles/512, OBJ sub-palettes/8, surfaces overflow kinds", () => {
    const r: SourceReport = { mode: "obj", report: { colors_used: 60, palettes_used: 9, tile_cells: 4, unique_tiles: 20, vram_words: 320, overflows: [{ kind: "Palettes", needed: 9, remapped_tiles: 2 }] } };
    const v = sourceReportView(r);
    expect(v.budget).toEqual(["20/512 tiles", "9/8 OBJ sub-palettes", "60 colors"]);
    expect(v.warns).toEqual(["Palettes: needs 9, 2 tiles remapped"]);
  });
});

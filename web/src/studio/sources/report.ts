import type { ImportOverflow, SourceReport } from "../../ppu/core";

const BG_TILE_CAP = 1024; // 10-bit tilemap tile field
const OBJ_TILE_CAP = 512; // 9-bit OBJ name field

function overflowLine(o: ImportOverflow): string {
  switch (o.kind) {
    case "Cropped": return `cropped to ${o.max_px}px`;
    case "Colors": return `Colors: ${o.unique} unique > ${o.budget} budget`;
    case "Palettes": return `Palettes: needs ${o.needed}, ${o.remapped_tiles} tiles remapped`;
    case "Tiles": return `Tiles: ${o.unique} unique, kept ${o.kept}`;
    case "TileSize16": return "16px tiles not supported";
  }
}

/** Budget + warnings for a SourceReport, per-kind, in the same vocabulary the
 *  VramTab / MemoryLayersOverlay import-health lines use. */
export function sourceReportView(report: SourceReport): { budget: string[]; warns: string[] } {
  if (report.mode === "m7") {
    const r = report.report;
    return {
      budget: [`${r.unique_tiles}/${r.tile_capacity} tiles`, `${r.colors}/256 colors`, `${r.map_tiles_w}×${r.map_tiles_h} map`],
      warns: r.overflow_tiles > 0 ? [`${r.overflow_tiles} tiles over capacity`] : [],
    };
  }
  const r = report.report;
  const isObj = report.mode === "obj";
  return {
    budget: [
      `${r.unique_tiles}/${isObj ? OBJ_TILE_CAP : BG_TILE_CAP} tiles`,
      `${r.palettes_used}/8 ${isObj ? "OBJ sub-palettes" : "sub-palettes"}`,
      `${r.colors_used} colors`,
    ],
    warns: r.overflows.map(overflowLine),
  };
}

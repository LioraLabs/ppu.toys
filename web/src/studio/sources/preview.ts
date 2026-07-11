import type { SourceKind, SourceMeta } from "../../ppu/core";
import { bgCell, decodeSourcePayload, quantizedRgba } from "./payload";
import { sourceReportView } from "./report";

export interface CellLabel { top: string; bot: string }
export interface PreviewImage { pixels: Uint8ClampedArray; width: number; height: number }
export interface PreviewModel {
  cols: number;
  rows: number;
  cellPx: number;      // source-pixel size of one grid cell (8 for bg/m7, cellSize for obj)
  width: number;
  height: number;
  cells: CellLabel[];  // row-major, cols*rows
  image: PreviewImage | null; // quantized RGBA, or null when undecodable
  budget: string[];
  warns: string[];
}

export function buildPreviewModel(kind: SourceKind, meta: SourceMeta, payload: Uint8Array, cellSize = 8): PreviewModel {
  const decoded = decodeSourcePayload(payload);
  const view = sourceReportView(meta.report);
  const cellPx = kind === "obj" ? cellSize : 8;
  const cols = Math.max(1, Math.ceil(meta.width / cellPx));
  const rows = Math.max(1, Math.ceil(meta.height / cellPx));
  const cells: CellLabel[] = [];

  if (kind === "obj" && meta.cells && meta.cells.length) {
    for (let i = 0; i < cols * rows; i++) {
      const c = meta.cells[i];
      cells.push(c ? { top: `t${c.tile}`, bot: `p${c.pal}` } : { top: "—", bot: "" });
    }
  } else if (kind === "bg" && decoded?.kind === "bg") {
    for (let ty = 0; ty < rows; ty++) for (let tx = 0; tx < cols; tx++) {
      const c = bgCell(decoded, cols, rows, tx, ty);
      cells.push({ top: `t${c.tile}`, bot: `p${c.pal}` });
    }
  } else if (kind === "m7" && decoded?.kind === "m7") {
    for (let i = 0; i < cols * rows; i++) cells.push({ top: `t${decoded.map[i] ?? 0}`, bot: "" });
  } else {
    // degraded: label by linear grid index
    for (let i = 0; i < cols * rows; i++) cells.push({ top: `#${i}`, bot: "" });
  }

  const image = decoded ? quantizedRgba(decoded, meta.width, meta.height) : null;
  return { cols, rows, cellPx, width: meta.width, height: meta.height, cells, image, budget: view.budget, warns: view.warns };
}

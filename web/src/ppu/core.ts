/** One source file of a multi-file sketch. Order is semantic (execution order). */
export interface SourceFile {
  name: string;
  source: string;
}

export interface LuaError {
  message: string;
  line?: number;
  /** Source file the error is attributed to (multi-file sketches). */
  file?: string;
}

export interface RegisterView {
  addr: number;
  name: string;
  value: number;
  changed: boolean;
}

export interface OamSprite {
  x: number;
  y: number;
  tile: number;
  pal: number; // 0..7
  prio: number; // 0..3
  large: boolean; // OAM high-table size bit: false = small, true = large
  flipX: boolean;
  flipY: boolean;
  on: boolean;
}

/** Per-frame OBJ overflow diagnostic ($213E STAT77), from render_frame_stats.
 *  rangeOver/timeOver are set if ANY scanline overflowed; maxSprites/maxTiles are
 *  the busiest line's in-range sprite count / attempted tile-sliver count. */
export interface ObjOverflow {
  rangeOver: boolean;
  timeOver: boolean;
  maxSprites: number;
  maxTiles: number;
}

export interface AssetInfo {
  id: string;
  width: number;
  height: number;
}

export type ImportOverflow =
  | { kind: "Cropped"; max_px: number }
  | { kind: "Colors"; unique: number; budget: number }
  | { kind: "Palettes"; needed: number; remapped_tiles: number }
  | { kind: "Tiles"; unique: number; kept: number }
  | { kind: "TileSize16" };

export interface TileImportBudget {
  colors_used: number;
  palettes_used: number;
  tile_cells: number;
  unique_tiles: number;
  vram_words: number;
  overflows: ImportOverflow[];
}

export interface Mode7ImportBudget {
  colors: number;
  unique_tiles: number;
  tile_capacity: number;
  overflow_tiles: number;
  map_tiles_w: number;
  map_tiles_h: number;
}

export type ImportReport =
  | { mode: "tile"; layer: number; report: TileImportBudget }
  | { mode: "m7"; layer: number; report: Mode7ImportBudget }
  | { mode: "obj"; report: TileImportBudget };

export interface FrameResult {
  framebuffer: Uint8ClampedArray; // 256*224*4 RGBA
  registers: RegisterView[];
  cgram: Uint16Array;
  oam: OamSprite[]; // 128 sprite entries -> SPRITES inspector
  objOverflow: ObjOverflow; // $213E STAT77 per-frame flags + busiest-line counts
}

/** The one hard seam. Headless — no canvas. Both the mock and the real WASM
 *  module implement this; JS owns presentation. */
export interface PpuCore {
  /** Single-file sugar for setSources([{ name: "main.lua", source: src }]). */
  setSource(src: string): { ok: boolean; error?: LuaError };
  /** Compile + run chunks in list order into ONE shared global scope (PICO-8
   *  semantics); frame()/init() resolve after all chunks. Errors carry `file`. */
  setSources(files: SourceFile[]): { ok: boolean; error?: LuaError };
  frame(t: number, f: number): FrameResult;
  uploadTexture(slot: string, imageData: ImageData): void;
  setLayerVisible(id: string, visible: boolean): void;
  /** Enumerate uploaded sources resident in VRAM -> VRAM inspector. */
  listAssets(): AssetInfo[];
  /** Mirrored PPU VRAM words from the most recent frame. */
  vram(): Uint16Array;
  /** Per-import budget/overflow reports from the most recent frame. */
  importReports(): ImportReport[];
}

export const WIDTH = 256;
export const HEIGHT = 224;

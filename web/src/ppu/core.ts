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

export type SourceKind = "bg" | "m7" | "obj";

/** Format-commit options for convertSource. bg: bit_depth (default 4);
 *  obj: cell_size — the OBJ size one obj[i].tile addresses (default 8);
 *  m7: none in v1 (the payload's options block is extensible for a later 7bpp+priority variant). */
export interface ConvertSourceOptions {
  bit_depth?: 2 | 4 | 8;
  tile_size?: 8;
  cell_size?: 8 | 16 | 32 | 64;
}

/** One source cell's resolved OBJ attributes (obj sources). */
export interface ObjCellMeta {
  tile: number;
  pal: number;
  flip_x: boolean;
  flip_y: boolean;
}

/** Authoring-time budget snapshot — ImportReport minus the bind-time `layer`. */
export type SourceReport =
  | { mode: "tile"; report: TileImportBudget }
  | { mode: "m7"; report: Mode7ImportBudget }
  | { mode: "obj"; report: TileImportBudget };

/** Travels alongside a payload, never inside it. */
export interface SourceMeta {
  width: number;
  height: number;
  report: SourceReport;
  cells?: ObjCellMeta[];
}

export interface ConvertSourceResult {
  payload: Uint8Array;
  meta: SourceMeta;
}

export interface FrameResult {
  framebuffer: Uint8ClampedArray; // 256*224*4 RGBA
  registers: RegisterView[];
  cgram: Uint16Array;
  oam: OamSprite[]; // 128 sprite entries -> SPRITES inspector
  objOverflow: ObjOverflow; // $213E STAT77 per-frame flags + busiest-line counts
}

/** Compositor intermediates for the most recent frame(): the two per-screen
 *  composites are PRE color-math and PRE brightness; mathMask is one byte per
 *  pixel — bit0 = color math applied, bit1 = clip-to-black region, bit2 =
 *  prevent-math region. */
export interface CompositorScreens {
  main: Uint8ClampedArray; // 256*224*4 RGBA
  sub: Uint8ClampedArray; // 256*224*4 RGBA
  mathMask: Uint8Array; // 256*224
}

/** Plane ids for layer views — same ids as setLayerVisible. */
export type PlaneId = "bg1" | "bg2" | "bg3" | "bg4" | "obj";

/** Trace chain for a BG selection: source registers -> tilemap entry + tile
 *  pixel data (stored/unflipped, row-major) -> resolved color. `pixel` is
 *  present for screen-pixel selections only. Mode-7 rows reuse the shape:
 *  entry = the interleaved VRAM word, pixels = the 8x8 char, pal/flips = 0. */
export interface BgTrace {
  regs: {
    mode: number;
    layer: number; // 1-based, bg[n]
    mapBase: number;
    charBase: number;
    tileSize: number;
    screenSize: number;
    bpp: number;
    scrollX: number;
    scrollY: number;
    mosaic: number; // effective block edge, 1 = off
    directColor: boolean;
    visible: boolean;
  };
  tile: {
    tx: number;
    ty: number;
    mapAddr: number;
    entry: number;
    tile: number;
    pal: number;
    prio: boolean;
    flipX: boolean;
    flipY: boolean;
    charAddr: number;
    pixels: number[]; // tileSize*tileSize palette indices (mode 7: 8x8)
    paletteBase: number; // CGRAM base of the sub-palette (0 for 8bpp/direct)
  };
  pixel?: {
    x: number;
    y: number;
    fx: number;
    fy: number;
    index: number;
    cgramIndex?: number; // absent for direct color / transparent
    bgr555: number;
    rgb: [number, number, number];
  };
}

/** Trace chain for an OAM sprite: OAM entry -> OBSEL char base -> stored
 *  (unflipped) pixel grid -> sub-palette. */
export interface ObjTrace {
  index: number;
  oam: OamSprite;
  charBase: number;
  charAddr: number;
  width: number;
  height: number;
  pixels: number[]; // width*height palette indices, row-major
  paletteBase: number; // 128 + pal*16
  palette: number[]; // the 16 BGR555 words of the sub-palette
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
  /** Compositor intermediates of the most recent frame() (M9 view seam). */
  screens(): CompositorScreens;
  /** Render ONE plane in isolation: 256*224*4 RGBA, alpha 0 = transparent.
   *  Ignores TM/TS/windows/priority; honors mode, visibility and mosaic. */
  layerView(plane: PlaneId): Uint8ClampedArray;
  /** Trace a BG plane (1..4) at screen pixel (x, y); null when the layer is
   *  absent in that scanline's mode. */
  traceBgPixel(layer: number, x: number, y: number): BgTrace | null;
  /** Trace a BG plane at tilemap cell (tx, ty); y picks the register row. */
  traceBgTile(layer: number, tx: number, ty: number, y: number): BgTrace | null;
  /** Trace OAM sprite index (0..127). */
  traceObj(index: number): ObjTrace | null;
  /** Pure quantize+pack: image -> versioned source payload + meta. No engine mutation. */
  convertSource(kind: SourceKind, options: ConvertSourceOptions, imageData: ImageData): ConvertSourceResult;
  /** Decode + register a payload for rendering under `name` (source-store stub, M10). */
  addSource(name: string, payload: Uint8Array): { ok: boolean; error?: string };
}

export const WIDTH = 256;
export const HEIGHT = 224;

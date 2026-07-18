/** Mock-data root: both stories (component props) and MSW handlers draw from
 *  these fixtures. Pure data only — no transport/ppuCore/router/msw imports. */

import type { Me, Profile, ToyFull, WallCard, WallPage } from "../api/apiClient";
import type {
  CompositorScreens,
  FrameResult,
  ImportReport,
  OamSprite,
  ObjOverflow,
  RegisterView,
  SourceMeta,
} from "../ppu/core";
import { HEIGHT, WIDTH } from "../ppu/core";
import type { OpenContext, OpenSketchState } from "../studio/sketches/openSketch";
import type { Sketch, SketchMeta } from "../studio/sketches/sketchStore";

export function makeWallCard(overrides?: Partial<WallCard>): WallCard {
  return {
    id: "abc123",
    title: "Dusk",
    author: { handle: "ada", avatar: null },
    thumbUrl: "/blobs/thumb/abc123",
    clipUrl: "/blobs/clip/abc123",
    heartCount: 3,
    hearted: false,
    ...overrides,
  };
}

export const wallCard: WallCard = makeWallCard();

/** A second card, so wall/profile lists have more than one entry. */
export const wallCard2: WallCard = makeWallCard({ id: "def456", title: "Ember" });

export function makeMe(overrides?: Partial<Me>): Me {
  return {
    id: "1",
    handle: "ada",
    isAdmin: false,
    ...overrides,
  };
}

export const me: Me = makeMe();

export function makeWallPage(overrides?: Partial<WallPage>): WallPage {
  return {
    toys: [wallCard, wallCard2],
    nextPage: null,
    ...overrides,
  };
}

export const wallPage: WallPage = makeWallPage();

export function makeProfile(overrides?: Partial<Profile>): Profile {
  return {
    user: { handle: "ada", avatar: null },
    toys: [wallCard, wallCard2],
    ...overrides,
  };
}

export const profile: Profile = makeProfile();

export function makeToyFull(overrides?: Partial<ToyFull>): ToyFull {
  return {
    id: "abc123",
    title: "Dusk",
    description: "A quiet sunset scene.",
    state: "published",
    files: [{ name: "main.lua", source: "-- code" }],
    sources: [],
    heartCount: 3,
    hearted: false,
    forkedFrom: null,
    author: { id: "1", handle: "ada", avatar: null },
    ...overrides,
  };
}

export const toyFull: ToyFull = makeToyFull();

/** A readable set of named registers covering the fields the inspector
 *  decodes by name (see studio/inspector/format.ts). Mode 1, full brightness,
 *  BG1+BG2+BG3+OBJ on main screen, additive color math on BG1+OBJ. */
export const frameRegisters: RegisterView[] = [
  { addr: 0x2100, name: "INIDISP", value: 0x0f, changed: false }, // full brightness, not force-blank
  { addr: 0x2101, name: "OBSEL", value: 0x00, changed: false }, // 8x8/16x16 size pair
  { addr: 0x2105, name: "BGMODE", value: 0x01, changed: true }, // mode 1
  { addr: 0x2106, name: "MOSAIC", value: 0x00, changed: false },
  { addr: 0x2107, name: "BG1SC", value: 0x00, changed: false },
  { addr: 0x2108, name: "BG2SC", value: 0x04, changed: false },
  { addr: 0x2109, name: "BG3SC", value: 0x08, changed: false },
  { addr: 0x210b, name: "BG12NBA", value: 0x00, changed: false },
  { addr: 0x210c, name: "BG34NBA", value: 0x00, changed: false },
  { addr: 0x212c, name: "TM", value: 0x17, changed: true }, // BG1,BG2,BG3,OBJ on main
  { addr: 0x212d, name: "TS", value: 0x00, changed: false },
  { addr: 0x2126, name: "WH0", value: 0x30, changed: false },
  { addr: 0x2127, name: "WH1", value: 0xa0, changed: false },
  { addr: 0x2128, name: "WH2", value: 0x50, changed: false },
  { addr: 0x2129, name: "WH3", value: 0xc0, changed: false },
  { addr: 0x2130, name: "CGWSEL", value: 0x00, changed: false },
  { addr: 0x2131, name: "CGADSUB", value: 0x21, changed: false }, // add, BG1+OBJ
  { addr: 0x2132, name: "COLDATA", value: 0x00, changed: false },
  { addr: 0x2133, name: "SETINI", value: 0x00, changed: false },
];

/** 256-entry BGR555 palette (0bBBBBB_GGGGG_RRRRR) with a visibly varied ramp
 *  so swatches in the CGRAM viewer render distinct colours. Index 0x81
 *  (OBJ palette 0, entry 1) is a bright, saturated colour for the sprite chip. */
export const frameCgram: Uint16Array = (() => {
  const cgram = new Uint16Array(256);
  for (let i = 0; i < 256; i++) {
    const r5 = i & 0x1f;
    const g5 = (i >> 1) & 0x1f;
    const b5 = (i >> 3) & 0x1f;
    cgram[i] = (b5 << 10) | (g5 << 5) | r5;
  }
  cgram[0x81] = (4 << 10) | (28 << 5) | 30; // bright, saturated
  return cgram;
})();

function makeOamSprite(overrides?: Partial<OamSprite>): OamSprite {
  return {
    x: 0,
    y: 0,
    tile: 0,
    pal: 0,
    prio: 0,
    large: false,
    flipX: false,
    flipY: false,
    on: false,
    ...overrides,
  };
}

const frameOam: OamSprite[] = Array.from({ length: 128 }, (_, i) => {
  const activeSprites: OamSprite[] = [
    { x: 24, y: 40, tile: 0x00, pal: 0, prio: 2, large: false, flipX: false, flipY: false, on: true },
    { x: 96, y: 64, tile: 0x04, pal: 1, prio: 1, large: true, flipX: true, flipY: false, on: true },
    { x: 160, y: 40, tile: 0x08, pal: 2, prio: 3, large: false, flipX: false, flipY: true, on: true },
    { x: 200, y: 120, tile: 0x0c, pal: 3, prio: 0, large: true, flipX: true, flipY: true, on: true },
    { x: 40, y: 160, tile: 0x10, pal: 4, prio: 2, large: false, flipX: false, flipY: false, on: true },
    { x: 120, y: 180, tile: 0x14, pal: 5, prio: 1, large: false, flipX: true, flipY: false, on: true },
  ];
  return i < activeSprites.length ? activeSprites[i] : makeOamSprite();
});

const frameObjOverflow: ObjOverflow = {
  rangeOver: false,
  timeOver: false,
  maxSprites: 12,
  maxTiles: 34,
};

export function makeFrameResult(overrides?: Partial<FrameResult>): FrameResult {
  return {
    framebuffer: new Uint8ClampedArray(WIDTH * HEIGHT * 4),
    registers: frameRegisters,
    cgram: frameCgram,
    oam: frameOam,
    objOverflow: frameObjOverflow,
    ...overrides,
  };
}

export const frameResult: FrameResult = makeFrameResult();

/** 32K-word VRAM image for the VramTab tile decoder + tilemap render (BG1,
 *  char base 0 / map base 0 per frameRegisters mode 1). The general fill is a
 *  deterministic multiplicative-hash gradient across the whole address space
 *  (visibly varied everywhere); words 0..63 — the BG1 tilemap's first two
 *  rows, per tilemapEntry's bit layout — are overwritten with crafted entries
 *  cycling tile 0..31, palette 0..7, and prio/flipX/flipY, so both the
 *  tilemap swatches and the tile-0..3 pixel decode (which reads those same
 *  words as character data at char base 0) show non-trivial, varied output. */
export const frameVram: Uint16Array = (() => {
  const vram = new Uint16Array(0x8000);
  for (let i = 0; i < 0x8000; i++) {
    vram[i] = ((i * 0x9e3779b1 + (i >> 3) * 0x85ebca6b) ^ (i << 2)) & 0xffff;
  }
  for (let i = 0; i < 64; i++) {
    const tile = i % 32;
    const pal = i % 8;
    const prio = i % 2 === 1 ? 1 : 0;
    const flipX = i % 3 === 0 ? 1 : 0;
    const flipY = i % 5 === 0 ? 1 : 0;
    vram[i] = tile | (pal << 10) | (prio << 13) | (flipX << 14) | (flipY << 15);
  }
  return vram;
})();

/** Two import-report entries covering both render paths the Import tab
 *  handles: a "tile" report with a non-empty overflow list (renders the
 *  warn path) and an "obj" report with no overflows (renders clean). */
export const frameImportReports: ImportReport[] = [
  {
    mode: "tile",
    layer: 0,
    report: {
      colors_used: 15,
      palettes_used: 4,
      tile_cells: 512,
      unique_tiles: 520,
      vram_words: 8192,
      overflows: [{ kind: "Tiles", unique: 520, kept: 512 }],
    },
  },
  {
    mode: "obj",
    report: {
      colors_used: 12,
      palettes_used: 2,
      tile_cells: 128,
      unique_tiles: 96,
      vram_words: 2048,
      overflows: [],
    },
  },
];

/** Compositor main/sub screens + math mask for the Compose tab: main is a
 *  warm gradient, sub a distinguishable cool gradient (visibly different
 *  compose previews), and mathMask's bit0 is set across the left half of the
 *  frame so the math-region tint toggle has a visible effect. */
export const frameScreens: CompositorScreens = (() => {
  const main = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
  const sub = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
  const mathMask = new Uint8Array(WIDTH * HEIGHT);
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const p = y * WIDTH + x;
      const i = p * 4;
      main[i] = Math.floor((x / WIDTH) * 255);
      main[i + 1] = Math.floor((y / HEIGHT) * 255);
      main[i + 2] = 64;
      main[i + 3] = 255;
      sub[i] = 32;
      sub[i + 1] = Math.floor((1 - x / WIDTH) * 255);
      sub[i + 2] = Math.floor((1 - y / HEIGHT) * 255);
      sub[i + 3] = 255;
      if (x < WIDTH / 2) mathMask[p] = 1;
    }
  }
  return { main, sub, mathMask };
})();

// ── Studio widgets & dialogs (sketch library / source / publish) ─────────────
// Additive fixtures for the `web/src/studio/` widgets bucket. Type-only imports
// from the stores keep this module pure data (no store/transport/ppuCore code).

/** A library row (SketchMeta = a Sketch minus its payloads). */
export function makeSketchMeta(overrides?: Partial<SketchMeta>): SketchMeta {
  return {
    id: "sk-dusk",
    name: "Dusk study",
    createdAt: 1_700_000_000_000,
    updatedAt: Date.now() - 5 * 60_000, // "5m ago"
    ...overrides,
  };
}

/** A short library list with distinct ages so LibraryPanel's rows + timeAgo
 *  labels render varied output. The first id matches `librarySketch` below so
 *  the story can show the open-row highlight + disabled Delete. */
export const sketchMetaList: SketchMeta[] = [
  makeSketchMeta(),
  makeSketchMeta({ id: "sk-ember", name: "Ember gradient", updatedAt: Date.now() - 3 * 3600_000 }),
  makeSketchMeta({ id: "sk-mode7", name: "Mode 7 floor", updatedAt: Date.now() - 2 * 86_400_000 }),
];

/** A full Sketch matching the first list row — used as the open context so the
 *  library highlights it and disables its Delete button. */
export const librarySketch: Sketch = {
  id: "sk-dusk",
  name: "Dusk study",
  createdAt: 1_700_000_000_000,
  updatedAt: Date.now() - 5 * 60_000,
  files: [
    { name: "pokes.lua", source: "function apply_pokes()\nend\n" },
    { name: "main.lua", source: "function frame(t, f)\n  apply_pokes()\nend\n" },
  ],
  sources: [],
};

/** Build an OpenSketchState around a given context (default: `librarySketch`
 *  open). The store's `useOpenSketch` shape, as pure data for the seam story. */
export function makeOpenSketchState(context?: OpenContext): OpenSketchState {
  return {
    context: context ?? { kind: "sketch", sketch: librarySketch },
    dirty: false,
    session: 1,
  };
}

export const libraryOpenState: OpenSketchState = makeOpenSketchState();

/** A valid v1 Mode-7 source payload (2×2 tiles = 16×16px) so SourcePreview
 *  decodes a real quantized image with no wasm. Byte layout mirrors
 *  studio/sources/payload.ts `decodeSourcePayload` (kind 1). */
export const sourcePayloadM7: Uint8Array = (() => {
  const bytes: number[] = [];
  const u8 = (v: number) => bytes.push(v & 0xff);
  const u16 = (v: number) => bytes.push(v & 0xff, (v >> 8) & 0xff); // little-endian
  u8(1); // version
  u8(1); // kind = m7
  u8(0); // optsLen (no options block)
  // palette: 15 BGR555 colors (index i renders palette[i-1]; 0 = transparent)
  const pal: number[] = Array.from({ length: 15 }, (_, i) => {
    const r5 = (i * 2) & 0x1f;
    const g5 = (i * 3 + 4) & 0x1f;
    const b5 = (31 - i) & 0x1f;
    return (b5 << 10) | (g5 << 5) | r5;
  });
  u8(pal.length);
  pal.forEach(u16);
  const tileCount = 4;
  u16(tileCount);
  for (let t = 0; t < tileCount; t++) {
    for (let p = 0; p < 64; p++) {
      // deterministic per-tile pattern across indices 0..15 (0 = transparent)
      u8(((p + t * 5) % 16));
    }
  }
  u8(2); // tilesW
  u8(2); // tilesH
  [0, 1, 2, 3].forEach(u8); // map
  return new Uint8Array(bytes);
})();

export function makeSourceMeta(overrides?: Partial<SourceMeta>): SourceMeta {
  return {
    width: 16,
    height: 16,
    report: {
      mode: "m7",
      report: {
        colors: 15,
        unique_tiles: 4,
        tile_capacity: 256,
        overflow_tiles: 0,
        map_tiles_w: 2,
        map_tiles_h: 2,
      },
    },
    ...overrides,
  };
}

export const sourceMetaM7: SourceMeta = makeSourceMeta();

/** Ensure-saved stub for PublishDialog's `save` prop: resolves to a toy id
 *  without touching the cloud, so the dialog stories run offline. */
export const publishSave = async (_meta?: { title?: string; description?: string }): Promise<string> =>
  "toy-fixture-123";

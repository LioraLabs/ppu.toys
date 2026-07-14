// Vitest global setup (wired via vite.config.ts `test.setupFiles`). Runs before
// each test file's imports, so the shared `ppuCore` is live before the transport
// singleton constructs its first frame against it.
//
// The real WASM core can't init under node/jsdom (wasm-pack's `web` target fetches
// the module by URL), so tests run against this stub: a PpuCore that returns
// correctly-shaped, zeroed data. It exists ONLY so singleton-based tests have a
// core; components/logic that need meaningful output inject their own doubles.
import {
  PpuCore,
  FrameResult,
  CompositorScreens,
  ConvertSourceResult,
  OamSprite,
  ObjOverflow,
  SourceKind,
  ConvertSourceOptions,
  PlaneId,
  WIDTH,
  HEIGHT,
} from "../ppu/core";
import { setPpuCore } from "../ppu/instance";

function emptyOam(): OamSprite[] {
  return Array.from({ length: 128 }, () => ({
    x: 0, y: 0, tile: 0, pal: 0, prio: 0, large: false, flipX: false, flipY: false, on: false,
  }));
}

const noObjOverflow: ObjOverflow = { rangeOver: false, timeOver: false, maxSprites: 0, maxTiles: 0 };

/** A do-nothing PpuCore for tests: zeroed buffers of the exact PPU dimensions. */
class StubPpuCore implements PpuCore {
  setSource() { return { ok: true }; }
  setSources() { return { ok: true }; }
  frame(): FrameResult {
    return {
      framebuffer: new Uint8ClampedArray(WIDTH * HEIGHT * 4),
      registers: [],
      cgram: new Uint16Array(256),
      oam: emptyOam(),
      objOverflow: noObjOverflow,
    };
  }
  setLayerVisible() {}
  vram() { return new Uint16Array(0x8000); }
  importReports() { return []; }
  screens(): CompositorScreens {
    return {
      main: new Uint8ClampedArray(WIDTH * HEIGHT * 4),
      sub: new Uint8ClampedArray(WIDTH * HEIGHT * 4),
      mathMask: new Uint8Array(WIDTH * HEIGHT),
    };
  }
  layerView(_plane: PlaneId) { return new Uint8ClampedArray(WIDTH * HEIGHT * 4); }
  traceBgPixel() { return null; }
  traceBgTile() { return null; }
  traceObj() { return null; }
  convertSource(_kind: SourceKind, _options: ConvertSourceOptions, imageData: ImageData): ConvertSourceResult {
    return {
      payload: new Uint8Array(),
      meta: {
        width: imageData.width,
        height: imageData.height,
        report: {
          mode: "tile",
          report: { colors_used: 0, palettes_used: 0, tile_cells: 0, unique_tiles: 0, vram_words: 0, overflows: [] },
        },
      },
    };
  }
  addSource() { return { ok: true }; }
}

setPpuCore(new StubPpuCore());

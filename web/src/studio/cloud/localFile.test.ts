// @vitest-environment jsdom
import "fake-indexeddb/auto";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import {
  PPU_FILE_VERSION,
  PPU_SOURCE_FILE_VERSION,
  serializeSourceToFile,
  parseSourceFile,
  serializeToFile,
  parseFile,
  saveLocalFile,
  openLocalFile,
  _resetLocalFileForTests,
} from "./localFile";
import { openSketchStore } from "../sketches/openSketch";
import { _resetSketchStoreForTests, loadSketch } from "../sketches/sketchStore";

const skyBytes = new Uint8Array([1, 2, 3, 4, 5]);

const SKY_META = {
  width: 8,
  height: 8,
  report: {
    mode: "tile" as const,
    report: {
      colors_used: 0,
      palettes_used: 0,
      tile_cells: 0,
      unique_tiles: 0,
      vram_words: 0,
      overflows: [],
    },
  },
};

beforeEach(() => {
  (globalThis as { indexedDB: IDBFactory }).indexedDB = new IDBFactory();
  _resetSketchStoreForTests();
  openSketchStore._resetForTests();
  _resetLocalFileForTests();
  delete (window as unknown as { showSaveFilePicker?: unknown }).showSaveFilePicker;
});

function buildState() {
  openSketchStore.editFile("main.lua", "-- hello");
  openSketchStore.addSource({
    name: "sky",
    kind: "bg",
    options: { bit_depth: 4 },
    payload: skyBytes,
    meta: SKY_META,
  });
  openSketchStore.setOrigin({ id: "toy-123", revision: 4, authorId: "u1" });
  return openSketchStore.state();
}

describe("serializeToFile / parseFile", () => {
  it("round-trips: parse(serialize(state)) equals the state's files/sources/origin/name", () => {
    const state = buildState();
    const text = serializeToFile(state);
    expect(JSON.parse(text).version).toBe(PPU_FILE_VERSION);
    const parsed = parseFile(text);

    expect(parsed.name).toBe(state.context.sketch.name);
    expect(parsed.files).toEqual(state.context.sketch.files);
    expect(parsed.sources.map((s) => s.name)).toEqual(["sky"]);
    expect(parsed.sources).toEqual(state.context.sketch.sources);
    expect(parsed.origin).toEqual({ id: "toy-123", revision: 4, authorId: "u1" });
  });

  it("rejects an unknown version tag", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.version = "ppu.toys/0";
    expect(() => parseFile(JSON.stringify(body))).toThrow(/version/i);
  });

  it("rejects a body with no version tag (e.g. a CLI ppu.json manifest)", () => {
    expect(() => parseFile(JSON.stringify({}))).toThrow(/not a ppu\.toys/i);
  });

  it("rejects a body missing files", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    delete body.files;
    expect(() => parseFile(JSON.stringify(body))).toThrow(/files/i);
  });

  it("rejects files with no main.lua entry", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.files = [];
    expect(() => parseFile(JSON.stringify(body))).toThrow(/main\.lua/i);
  });

  it("rejects a source with malformed base64 payload", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.sources[0].payload = "!!!";
    expect(() => parseFile(JSON.stringify(body))).toThrow(/base64|payload/i);
  });

  it("rejects a source missing meta", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    delete body.sources[0].meta;
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
  });

  it("rejects a source whose meta is missing report", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.sources[0].meta = { width: 8, height: 8 };
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
  });

  it("rejects a report with an unknown mode or no budget object", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.sources[0].meta = { width: 8, height: 8, report: { mode: "bg" } };
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
    body.sources[0].meta = { width: 8, height: 8, report: { mode: "tile" } };
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
    body.sources[0].kind = "toString";
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
  });

  it("rejects a non-m7 report whose report.overflows isn't an array", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.sources[0].meta = { width: 8, height: 8, report: { mode: "tile", report: {} } };
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
    body.sources[0].meta = {
      width: 8,
      height: 8,
      report: { mode: "tile", report: { overflows: 5 } },
    };
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
  });

  it("accepts a valid m7 report shape that has no overflows field", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.sources[0].meta = {
      width: 8,
      height: 8,
      report: {
        mode: "m7",
        report: {
          unique_tiles: 1,
          tile_capacity: 256,
          colors: 16,
          map_tiles_w: 1,
          map_tiles_h: 1,
          overflow_tiles: 0,
        },
      },
    };
    expect(() => parseFile(JSON.stringify(body))).not.toThrow();
  });

  it("rejects a source with an unknown kind", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.sources[0].kind = "nope";
    expect(() => parseFile(JSON.stringify(body))).toThrow(/malformed/i);
  });

  it("rejects unparsable JSON", () => {
    expect(() => parseFile("{not json")).toThrow(/JSON/i);
  });

  it("rejects a malformed origin", () => {
    const body = JSON.parse(serializeToFile(buildState()));
    body.origin = { id: "toy-123" };
    expect(() => parseFile(JSON.stringify(body))).toThrow(/origin/i);
  });
});

describe("openLocalFile rejection leaves the open sketch untouched", () => {
  it("rejects and does not touch the open sketch", async () => {
    const before = openSketchStore.state().context.sketch.id;
    const bad = JSON.stringify({ version: "ppu.toys/0" });
    await expect(openLocalFile(new File([bad], "x.ppu.json"))).rejects.toThrow();
    expect(openSketchStore.state().context.sketch.id).toBe(before);
  });

  it("rejects a source missing meta and leaves the open sketch untouched", async () => {
    const before = openSketchStore.state().context.sketch.id;
    const body = JSON.parse(serializeToFile(buildState()));
    delete body.sources[0].meta;
    await expect(openLocalFile(new File([JSON.stringify(body)], "x.ppu.json"))).rejects.toThrow(
      /malformed/i,
    );
    expect(openSketchStore.state().context.sketch.id).toBe(before);
  });

  it("rejects a source with an unknown kind and leaves the open sketch untouched", async () => {
    const before = openSketchStore.state().context.sketch.id;
    const body = JSON.parse(serializeToFile(buildState()));
    body.sources[0].kind = "nope";
    await expect(openLocalFile(new File([JSON.stringify(body)], "x.ppu.json"))).rejects.toThrow(
      /malformed/i,
    );
    expect(openSketchStore.state().context.sketch.id).toBe(before);
  });
});

describe("saveLocalFile: picker path (Chromium)", () => {
  it("opens the picker once per sketch, writes silently on later saves", async () => {
    const state = buildState();
    let written = "";
    const writable = { write: vi.fn(async (t: string) => void (written = t)), close: vi.fn() };
    const handle = { createWritable: vi.fn(async () => writable) };
    const picker = vi.fn(async () => handle);
    (window as unknown as { showSaveFilePicker: typeof picker }).showSaveFilePicker = picker;

    await saveLocalFile(state);
    await saveLocalFile(state);

    expect(picker).toHaveBeenCalledTimes(1);
    expect(writable.write).toHaveBeenCalledTimes(2);
    expect(() => parseFile(written)).not.toThrow();

    // a different sketch never reuses the first file's handle
    const otherState = {
      ...state,
      context: {
        ...state.context,
        sketch: { ...state.context.sketch, id: "some-other-id" },
      },
    };
    await saveLocalFile(otherState);
    expect(picker).toHaveBeenCalledTimes(2);
  });

  it("resolves false (nothing written) when the picker is cancelled (AbortError)", async () => {
    const state = buildState();
    const abort = Object.assign(new Error("cancelled"), { name: "AbortError" });
    const picker = vi.fn(async () => {
      throw abort;
    });
    (window as unknown as { showSaveFilePicker: typeof picker }).showSaveFilePicker = picker;

    await expect(saveLocalFile(state)).resolves.toBe(false);
  });

  it("evicts a handle that fails to write, re-prompting on the next save", async () => {
    const state = buildState();
    const writable = { write: vi.fn(async () => void 0), close: vi.fn(async () => void 0) };
    let createWritable = vi.fn(async () => {
      throw new Error("NotFoundError");
    });
    const handle = { createWritable: () => createWritable() };
    const picker = vi.fn(async () => handle);
    (window as unknown as { showSaveFilePicker: typeof picker }).showSaveFilePicker = picker;

    await expect(saveLocalFile(state)).rejects.toThrow();
    expect(picker).toHaveBeenCalledTimes(1);

    createWritable = vi.fn(async () => writable);
    await saveLocalFile(state);
    expect(picker).toHaveBeenCalledTimes(2);
  });
});

describe("saveLocalFile: download path (no picker)", () => {
  it("downloads via a temporary anchor", async () => {
    const state = buildState();
    const createObjectURL = vi.fn(() => "blob:x");
    const revokeObjectURL = vi.fn();
    URL.createObjectURL = createObjectURL;
    URL.revokeObjectURL = revokeObjectURL;
    const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      expect(this.download.endsWith(".ppu.json")).toBe(true);
    });

    await saveLocalFile(state);
    await new Promise((r) => setTimeout(r, 0));

    expect(clickSpy).toHaveBeenCalledTimes(1);
    expect(createObjectURL).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledTimes(1);

    clickSpy.mockRestore();
  });
});

describe("openLocalFile", () => {
  it("mints and opens a new sketch with files, sources, and origin intact", async () => {
    const state = buildState();
    const text = serializeToFile(state);

    // fresh store: open the file into a brand-new sketch
    openSketchStore._resetForTests();
    await openLocalFile(new File([text], "x.ppu.json"));

    const opened = openSketchStore.state();
    expect(opened.context.kind).toBe("sketch");
    expect(opened.context.sketch.files).toEqual(state.context.sketch.files);
    const sources = opened.context.sketch.sources;
    expect(sources.map((s) => s.name)).toEqual(["sky"]);
    expect(Array.from(sources[0].payload)).toEqual(Array.from(skyBytes));
    expect(opened.context.sketch.origin).toEqual({ id: "toy-123", revision: 4, authorId: "u1" });

    // persisted: a reload shows the origin
    const loaded = await loadSketch(opened.context.sketch.id);
    expect(loaded!.origin).toEqual({ id: "toy-123", revision: 4, authorId: "u1" });
  });
});

describe("source files", () => {
  const sky = {
    name: "sky",
    kind: "bg" as const,
    options: { bit_depth: 4 as const },
    payload: skyBytes,
    meta: SKY_META,
  };

  it("round-trips one source through .ppusrc.json", () => {
    const text = serializeSourceToFile(sky);
    expect(JSON.parse(text).version).toBe(PPU_SOURCE_FILE_VERSION);
    expect(JSON.parse(text).payload).toBe("AQIDBAU=");
    const back = parseSourceFile(text);
    expect(back.name).toBe("sky");
    expect(back.kind).toBe("bg");
    expect(back.options).toEqual({ bit_depth: 4 });
    expect(Array.from(back.payload)).toEqual([1, 2, 3, 4, 5]);
    expect(back.meta).toEqual(SKY_META);
  });

  it("rejects a whole-sketch file, a bad version, and a bad payload", () => {
    expect(() => parseSourceFile(JSON.stringify({ version: PPU_FILE_VERSION }))).toThrow(
      /Unknown source file version/,
    );
    expect(() => parseSourceFile("{}")).toThrow(/Not a ppu.toys source file/);
    expect(() => parseSourceFile(serializeSourceToFile(sky).replace('"AQIDBAU="', '"@@"'))).toThrow(
      /invalid base64/,
    );
  });
});

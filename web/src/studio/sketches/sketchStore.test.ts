import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import {
  createSketch,
  saveSketch,
  loadSketch,
  listSketches,
  renameSketch,
  duplicateSketch,
  deleteSketch,
  onSketchesChanged,
  _resetSketchStoreForTests,
} from "./sketchStore";
import type { SketchSource } from "./sketchStore";

function src(name: string, byte = 1): SketchSource {
  return {
    name, kind: "bg", options: { bit_depth: 4 }, payload: new Uint8Array([byte]),
    meta: { width: 8, height: 8, report: { mode: "tile", report: {
      colors_used: 0, palettes_used: 0, tile_cells: 0, unique_tiles: 0, vram_words: 0, overflows: [],
    } } },
  };
}

beforeEach(() => {
  // fresh in-memory IndexedDB per test; drop the module's cached connection
  (globalThis as { indexedDB: IDBFactory }).indexedDB = new IDBFactory();
  _resetSketchStoreForTests();
  // monotonically increasing clock so updatedAt ordering is deterministic
  let now = 1_000;
  vi.spyOn(Date, "now").mockImplementation(() => now++);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("sketchStore CRUD", () => {
  it("creates and loads a sketch, round-tripping files and source bytes", async () => {
    const made = await createSketch(
      "dusk",
      [{ name: "main.lua", source: "-- hi" }],
      [src("sky")],
    );
    const loaded = await loadSketch(made.id);
    expect(loaded).toBeDefined();
    expect(loaded!.name).toBe("dusk");
    expect(loaded!.createdAt).toBe(loaded!.updatedAt);
    expect(loaded!.files).toEqual([{ name: "main.lua", source: "-- hi" }]);
    expect(Array.from(loaded!.sources[0].payload)).toEqual([1]);
    expect(loaded!.sources[0].kind).toBe("bg");
  });

  it("loadSketch returns undefined for an unknown id", async () => {
    expect(await loadSketch("nope")).toBeUndefined();
  });

  it("saveSketch upserts and bumps updatedAt", async () => {
    const made = await createSketch("a", [{ name: "main.lua", source: "1" }]);
    const saved = await saveSketch({ ...made, files: [{ name: "main.lua", source: "2" }] });
    expect(saved.updatedAt).toBeGreaterThan(made.updatedAt);
    const loaded = await loadSketch(made.id);
    expect(loaded!.files[0].source).toBe("2");
  });

  it("lists metadata only, newest-updated first", async () => {
    const a = await createSketch("a", []);
    await createSketch("b", []);
    await saveSketch(a); // touching a makes it newest
    const list = await listSketches();
    expect(list.map((s) => s.name)).toEqual(["a", "b"]);
    expect(list[0]).not.toHaveProperty("files");
    expect(list[0]).not.toHaveProperty("sources");
  });

  it("renames in place", async () => {
    const made = await createSketch("old", []);
    await renameSketch(made.id, "new");
    expect((await loadSketch(made.id))!.name).toBe("new");
  });

  it("duplicates with a new id, '(copy)' name, and copied payload", async () => {
    const made = await createSketch(
      "orig",
      [{ name: "main.lua", source: "-- src" }],
      [src("a")],
      "dusk-parallax",
    );
    const dup = await duplicateSketch(made.id);
    expect(dup!.id).not.toBe(made.id);
    expect(dup!.name).toBe("orig (copy)");
    expect(dup!.forkedFrom).toBe("dusk-parallax");
    const loaded = await loadSketch(dup!.id);
    expect(loaded!.files).toEqual(made.files);
    expect(Array.from(loaded!.sources[0].payload)).toEqual([1]);
  });

  it("drops legacy raw-PNG assets on read, opening with empty sources", async () => {
    const made = await createSketch("legacy", [{ name: "main.lua", source: "-- x" }]);
    // simulate a pre-M10 record: put back a shape carrying `assets`, no `sources`
    await saveSketch({
      ...made, sources: undefined as never,
      assets: [{ name: "sky.png", png: new Uint8Array([1, 2, 3]) }],
    } as never);
    const loaded = await loadSketch(made.id);
    expect(loaded!.sources).toEqual([]);
    expect(loaded as unknown as { assets?: unknown }).not.toHaveProperty("assets");
  });

  it("deletes", async () => {
    const made = await createSketch("gone", []);
    await deleteSketch(made.id);
    expect(await loadSketch(made.id)).toBeUndefined();
    expect(await listSketches()).toHaveLength(0);
  });

  it("notifies listeners on every mutation and supports unsubscribe", async () => {
    let calls = 0;
    const off = onSketchesChanged(() => calls++);
    const made = await createSketch("x", []); // emit 1
    await saveSketch(made); // emit 2
    await renameSketch(made.id, "y"); // delegates to saveSketch — emit 3
    await duplicateSketch(made.id); // delegates to createSketch — emit 4
    await deleteSketch(made.id); // emit 5
    expect(calls).toBe(5);
    off();
    await createSketch("z", []);
    expect(calls).toBe(5); // unsubscribed
  });
});

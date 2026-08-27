import { beforeEach, describe, expect, it } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import { openContextFiles, openSketchStore } from "./openSketch";
import { _resetSketchStoreForTests } from "./sketchStore";

describe("openSketchStore", () => {
  beforeEach(() => {
    globalThis.indexedDB = new IDBFactory();
    _resetSketchStoreForTests();
    openSketchStore._resetForTests();
  });

  it("boots with an empty toy instead of bundled Lua", () => {
    expect(openContextFiles(openSketchStore.state())).toEqual([
      expect.objectContaining({ name: "pokes.lua" }),
      { name: "main.lua", source: "" },
    ]);
  });

  it("creates a locally editable empty toy", async () => {
    await openSketchStore.newSketch();
    expect(openSketchStore.state().context.kind).toBe("sketch");
    const files = openContextFiles(openSketchStore.state());
    expect(files[files.length - 1]).toEqual({
      name: "main.lua",
      source: "",
    });
  });
});

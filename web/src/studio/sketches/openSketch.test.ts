import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import { openContextFiles, openSketchStore } from "./openSketch";
import { _resetSketchStoreForTests } from "./sketchStore";

describe("openSketchStore", () => {
  beforeEach(() => {
    globalThis.indexedDB = new IDBFactory();
    _resetSketchStoreForTests();
    openSketchStore._resetForTests();
  });
  afterEach(() => vi.unstubAllGlobals());

  it("boots with a usable fallback toy", () => {
    expect(openContextFiles(openSketchStore.state())).toEqual([
      expect.objectContaining({ name: "pokes.lua" }),
      { name: "main.lua", source: expect.stringContaining("function frame(t, f)") },
    ]);
  });

  it("loads the server starter for the Studio and new toys", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            name: "server starter",
            files: [{ name: "main.lua", source: "function frame() brightness = 8 end" }],
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      ),
    );
    await openSketchStore.initializeStarter();
    expect(openSketchStore.state().context.sketch.name).toBe("server starter");
    const files = openContextFiles(openSketchStore.state());
    expect(files[files.length - 1].source).toContain("brightness = 8");

    await openSketchStore.newSketch();
    expect(openSketchStore.state().context.kind).toBe("sketch");
    expect(openSketchStore.state().context.sketch.name).toBe("server starter");
  });

  it("keeps main.lua: no delete, no rename away", () => {
    openSketchStore.addFile(); // a second real file, so the last-file guard isn't what holds
    openSketchStore.deleteFile("main.lua");
    expect(openSketchStore.renameFile("main.lua", "entry.lua")).toBe(false);
    expect(openContextFiles(openSketchStore.state()).map((f) => f.name)).toContain("main.lua");
  });
});

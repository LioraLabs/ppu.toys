import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import { DEMOS } from "../demos/demos";
import {
  openSketchStore,
  AUTOSAVE_MS,
  NEW_SKETCH_SOURCE,
  openContextLabel,
  openContextFiles,
} from "./openSketch";
import { listSketches, loadSketch, _resetSketchStoreForTests } from "./sketchStore";

const demo = DEMOS[0];

/** The open sketch, or throw — keeps assertions terse. */
function openSketch() {
  const ctx = openSketchStore.state().context;
  if (ctx.kind !== "sketch") throw new Error("expected a sketch context");
  return ctx.sketch;
}

beforeEach(() => {
  (globalThis as { indexedDB: IDBFactory }).indexedDB = new IDBFactory();
  _resetSketchStoreForTests();
  openSketchStore._resetForTests();
  // fake ONLY setTimeout/clearTimeout: the debounce is ours, but fake-indexeddb
  // needs real setImmediate to complete its transactions under await.
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("initial state", () => {
  it("starts on the first demo, clean", () => {
    const s = openSketchStore.state();
    expect(s.context).toEqual({ kind: "demo", demoId: demo.id });
    expect(s.dirty).toBe(false);
    expect(openContextLabel(s)).toBe(demo.label);
  });
});

describe("lazy demo fork", () => {
  it("does NOT fork when the editor pushes the pristine demo source on mount", () => {
    openSketchStore.editFile("main.lua", demo.source);
    expect(openSketchStore.state().context.kind).toBe("demo");
    expect(openSketchStore.state().dirty).toBe(false);
  });

  it("forks into '<demo> (copy)' on the first real edit, keeping the session", () => {
    const before = openSketchStore.state().session;
    openSketchStore.editFile("main.lua", demo.source + "\n-- edit");
    const s = openSketchStore.state();
    expect(s.session).toBe(before); // editor must NOT remount mid-typing
    expect(s.dirty).toBe(true);
    const sk = openSketch();
    expect(sk.name).toBe(`${demo.label} (copy)`);
    expect(sk.forkedFrom).toBe(demo.id);
    expect(sk.files).toEqual([{ name: "main.lua", source: demo.source + "\n-- edit" }]);
  });

  it("routes subsequent edits into the same sketch — exactly one row persisted", async () => {
    openSketchStore.editFile("main.lua", "-- a");
    openSketchStore.editFile("main.lua", "-- b");
    await openSketchStore.flush();
    const list = await listSketches();
    expect(list).toHaveLength(1);
    const loaded = await loadSketch(list[0].id);
    expect(loaded!.files[0].source).toBe("-- b");
  });

  it("forks with the pristine demo source when an asset upload is the first change", () => {
    openSketchStore.addAsset({ name: "sky.png", png: new Uint8Array([1, 2, 3]) });
    const sk = openSketch();
    expect(sk.forkedFrom).toBe(demo.id);
    expect(sk.files).toEqual([{ name: "main.lua", source: demo.source }]);
    expect(sk.assets.map((a) => a.name)).toEqual(["sky.png"]);
  });
});

describe("autosave", () => {
  it("debounces: rapid edits coalesce into one pending timer, one stored sketch", async () => {
    openSketchStore.editFile("main.lua", "-- a");
    openSketchStore.editFile("main.lua", "-- b");
    expect(vi.getTimerCount()).toBe(1);
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS);
    await openSketchStore.flush(); // settle the in-flight save deterministically
    expect(openSketchStore.state().dirty).toBe(false);
    expect(await listSketches()).toHaveLength(1);
  });

  it("each edit resets the countdown", async () => {
    openSketchStore.editFile("main.lua", "-- a");
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS - 1);
    openSketchStore.editFile("main.lua", "-- b");
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS - 1);
    expect(openSketchStore.state().dirty).toBe(true); // timer restarted, not fired
    await vi.advanceTimersByTimeAsync(1);
    await openSketchStore.flush();
    expect(openSketchStore.state().dirty).toBe(false);
  });

  it("round-trips uploaded asset bytes through the flush", async () => {
    await openSketchStore.newSketch();
    openSketchStore.addAsset({ name: "hills.png", png: new Uint8Array([9, 8, 7]) });
    await openSketchStore.flush();
    const id = (await listSketches())[0].id;
    const loaded = await loadSketch(id);
    expect(loaded!.assets.map((a) => a.name)).toEqual(["hills.png"]);
    expect(Array.from(loaded!.assets[0].png)).toEqual([9, 8, 7]);
  });
});

describe("open / new / switch", () => {
  it("newSketch creates a blank sketch, opens it clean, bumps the session", async () => {
    const before = openSketchStore.state().session;
    await openSketchStore.newSketch();
    const s = openSketchStore.state();
    expect(s.session).toBe(before + 1);
    expect(s.dirty).toBe(false);
    expect(openSketch().files).toEqual([{ name: "main.lua", source: NEW_SKETCH_SOURCE }]);
    expect(await listSketches()).toHaveLength(1);
  });

  it("openSketch restores a stored sketch; re-pushing its own source stays clean", async () => {
    openSketchStore.editFile("main.lua", "-- mine");
    await openSketchStore.flush();
    const id = (await listSketches())[0].id;
    await openSketchStore.openDemo(DEMOS[1].id);
    const before = openSketchStore.state().session;
    await openSketchStore.openSketch(id);
    const s = openSketchStore.state();
    expect(s.session).toBe(before + 1);
    expect(s.dirty).toBe(false);
    expect(openSketch().files[0].source).toBe("-- mine");
    expect(openContextLabel(s)).toBe(`${demo.label} (copy)`);
    // the editor's mount push of the identical source must not dirty it
    openSketchStore.editFile("main.lua", "-- mine");
    expect(openSketchStore.state().dirty).toBe(false);
  });

  it("openDemo flushes pending edits before switching away", async () => {
    openSketchStore.editFile("main.lua", "-- keep me");
    const id = openSketch().id;
    await openSketchStore.openDemo(DEMOS[1].id);
    expect(openSketchStore.state().context).toEqual({ kind: "demo", demoId: DEMOS[1].id });
    expect(openSketchStore.state().dirty).toBe(false);
    const loaded = await loadSketch(id);
    expect(loaded!.files[0].source).toBe("-- keep me");
  });

  it("renaming the open sketch survives subsequent edits and the flush", async () => {
    openSketchStore.editFile("main.lua", "-- a");
    openSketchStore.rename("my toy");
    expect(openContextLabel(openSketchStore.state())).toBe("my toy");
    openSketchStore.editFile("main.lua", "-- b"); // must not resurrect the old name
    await openSketchStore.flush();
    const list = await listSketches();
    expect(list).toHaveLength(1);
    expect(list[0].name).toBe("my toy");
    const loaded = await loadSketch(list[0].id);
    expect(loaded!.files[0].source).toBe("-- b");
  });
});

describe("openContextFiles", () => {
  it("presents a demo as a single main.lua", () => {
    expect(openContextFiles(openSketchStore.state())).toEqual([
      { name: "main.lua", source: demo.source },
    ]);
  });
});

describe("file operations", () => {
  it("addFile appends a uniquely named empty file and returns the name", async () => {
    await openSketchStore.newSketch();
    const name = openSketchStore.addFile();
    expect(name).toBe("file2.lua");
    expect(openSketch().files.map((f) => f.name)).toEqual(["main.lua", "file2.lua"]);
    expect(openSketch().files[1].source).toBe("");
    expect(openSketchStore.state().dirty).toBe(true);
  });

  it("addFile skips names already taken", async () => {
    await openSketchStore.newSketch();
    openSketchStore.editFile("file2.lua", "-- taken");
    expect(openSketchStore.addFile()).toBe("file3.lua");
  });

  it("addFile on a demo forks it, keeping the demo file first (session preserved)", () => {
    const before = openSketchStore.state().session;
    const name = openSketchStore.addFile();
    expect(name).toBe("file2.lua");
    const sk = openSketch();
    expect(sk.forkedFrom).toBe(demo.id);
    expect(sk.files).toEqual([
      { name: "main.lua", source: demo.source },
      { name: "file2.lua", source: "" },
    ]);
    expect(openSketchStore.state().session).toBe(before);
  });

  it("renameFile renames and reports success", async () => {
    await openSketchStore.newSketch();
    expect(openSketchStore.renameFile("main.lua", "palette.lua")).toBe(true);
    expect(openSketch().files.map((f) => f.name)).toEqual(["palette.lua"]);
  });

  it("renameFile rejects empty, unknown, and duplicate targets", async () => {
    await openSketchStore.newSketch();
    openSketchStore.addFile(); // file2.lua
    expect(openSketchStore.renameFile("main.lua", "  ")).toBe(false);
    expect(openSketchStore.renameFile("nope.lua", "x.lua")).toBe(false);
    expect(openSketchStore.renameFile("main.lua", "file2.lua")).toBe(false);
    expect(openSketch().files.map((f) => f.name)).toEqual(["main.lua", "file2.lua"]);
  });

  it("renameFile on a demo forks with the renamed file", () => {
    expect(openSketchStore.renameFile("main.lua", "scene.lua")).toBe(true);
    expect(openSketch().files).toEqual([{ name: "scene.lua", source: demo.source }]);
  });

  it("deleteFile removes a file but refuses the last one", async () => {
    await openSketchStore.newSketch();
    openSketchStore.addFile();
    openSketchStore.deleteFile("file2.lua");
    expect(openSketch().files.map((f) => f.name)).toEqual(["main.lua"]);
    openSketchStore.deleteFile("main.lua"); // last file: no-op
    expect(openSketch().files).toHaveLength(1);
  });

  it("moveFile reorders (order is execution order) and persists through flush", async () => {
    await openSketchStore.newSketch();
    openSketchStore.addFile(); // file2.lua
    openSketchStore.addFile(); // file3.lua
    openSketchStore.moveFile(2, 0);
    expect(openSketch().files.map((f) => f.name)).toEqual([
      "file3.lua", "main.lua", "file2.lua",
    ]);
    await openSketchStore.flush();
    const list = await listSketches();
    const loaded = await loadSketch(list[0].id);
    expect(loaded!.files.map((f) => f.name)).toEqual(["file3.lua", "main.lua", "file2.lua"]);
  });

  it("moveFile ignores out-of-range and identity moves", async () => {
    await openSketchStore.newSketch();
    openSketchStore.addFile();
    openSketchStore.moveFile(0, 0);
    openSketchStore.moveFile(0, 5);
    openSketchStore.moveFile(-1, 0);
    expect(openSketch().files.map((f) => f.name)).toEqual(["main.lua", "file2.lua"]);
  });
});

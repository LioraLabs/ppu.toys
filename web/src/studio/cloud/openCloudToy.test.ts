// @vitest-environment jsdom
import "fake-indexeddb/auto";
import { describe, it, expect, beforeEach } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import { openCloudToy } from "./openCloudToy";
import { cloudDraft } from "./cloudDraft";
import { openSketchStore } from "../sketches/openSketch";
import { _resetSketchStoreForTests } from "../sketches/sketchStore";
import { encodeBase64 } from "../../api/base64";
import type { ToyFull } from "../../api/apiClient";

const skyBytes = new Uint8Array([1, 2, 3, 4, 5]);

function toy(): ToyFull {
  return {
    id: "toy-123",
    title: "my toy",
    description: "",
    state: "draft",
    files: [
      { name: "pokes.lua", source: "" },
      { name: "main.lua", source: "x" },
    ],
    sources: [
      {
        name: "sky",
        kind: "bg",
        builtinId: null,
        options: { bit_depth: 4 },
        meta: {
          width: 8,
          height: 8,
          report: {
            mode: "tile",
            report: {
              colors_used: 0, palettes_used: 0, tile_cells: 0, unique_tiles: 0,
              vram_words: 0, overflows: [],
            },
          },
        },
        payload: encodeBase64(skyBytes),
      },
      {
        name: "builtin-thing",
        kind: "bg",
        builtinId: "some-builtin",
        options: {},
        meta: null,
        payload: null,
      },
    ],
    heartCount: 0,
    hearted: false,
    forkedFrom: null,
    author: { id: "u1", handle: "someone", avatar: null },
  };
}

beforeEach(() => {
  (globalThis as { indexedDB: IDBFactory }).indexedDB = new IDBFactory();
  _resetSketchStoreForTests();
  openSketchStore._resetForTests();
  cloudDraft._resetForTests();
});

describe("openCloudToy", () => {
  it("mints a local sketch from the toy's files + payload-bearing sources, opens it, and binds the cloud draft", async () => {
    const t = toy();
    await openCloudToy(t);

    const state = openSketchStore.state();
    expect(state.context.kind).toBe("sketch");
    if (state.context.kind !== "sketch") return;

    const sources = state.context.sketch.sources;
    expect(sources.map((s) => s.name)).toEqual(["sky"]);
    expect(Array.from(sources[0].payload)).toEqual(Array.from(skyBytes));

    expect(cloudDraft.current(state.session)).toBe(t.id);
  });
});

import { describe, expect, it, vi } from "vitest";
import { serializeWorkspace, type ConvertAsset } from "./serialize";
import { DEMOS } from "../demos/demos";
import { newSketchObject, type SketchSource } from "../sketches/sketchStore";
import type { OpenSketchState } from "../sketches/openSketch";
import { encodeBase64 } from "../../api/base64";
import type { SourceMeta } from "../../ppu/core";

const FAKE_META: SourceMeta = {
  width: 4,
  height: 4,
  report: { mode: "tile", report: { colors_used: 0, palettes_used: 0, tile_cells: 0, unique_tiles: 0, vram_words: 0, overflows: [] } },
};

// Injectable converter: never touches ImageData/the core (neither exists in
// the vitest node env), unlike the browser-only defaultConvert.
const fakeConvert: ConvertAsset = (a) => ({
  payload: new Uint8Array([a.kind === "m7" ? 7 : 1]),
  meta: FAKE_META,
});

const dusk = DEMOS.find((d) => d.id === "dusk-parallax")!;

function userSource(name: string, payload: Uint8Array): SketchSource {
  return { name, kind: "bg", options: {}, payload, meta: FAKE_META };
}

describe("serializeWorkspace", () => {
  it("demo context: every bundled asset becomes a payload-bearing source", () => {
    const state: OpenSketchState = { context: { kind: "demo", demoId: "dusk-parallax" }, dirty: false, session: 0 };
    const result = serializeWorkspace(state, fakeConvert);

    expect(result.sources.map((s) => s.name).sort()).toEqual(["hero", "hills", "sky"]);
    for (const asset of dusk.assets) {
      const src = result.sources.find((s) => s.name === asset.id)!;
      expect(src).toBeDefined();
      expect(src.builtinId).toBeNull();
      expect(src.kind).toBe(asset.kind);
      expect(typeof src.payload).toBe("string");
      expect(src.payload!.length).toBeGreaterThan(0);
    }
    expect(result.sources.every((s) => typeof s.payload === "string" && s.payload.length > 0)).toBe(true);
    expect(result.files).toEqual(dusk.files);
    expect(result.files[0].name).toBe("pokes.lua");
  });

  it("forked-demo sketch: demo assets replay AND user-added sources are included", () => {
    const sketch = newSketchObject("x", dusk.files!, [userSource("custom", new Uint8Array([9]))], "dusk-parallax");
    const state: OpenSketchState = { context: { kind: "sketch", sketch }, dirty: false, session: 0 };
    const result = serializeWorkspace(state, fakeConvert);

    expect(result.sources.map((s) => s.name).sort()).toEqual(["custom", "hero", "hills", "sky"]);
    expect(result.sources.every((s) => typeof s.payload === "string" && s.payload.length > 0)).toBe(true);
  });

  it("user source overrides a same-named demo asset", () => {
    const sketch = newSketchObject("x", dusk.files!, [userSource("sky", new Uint8Array([42]))], "dusk-parallax");
    const state: OpenSketchState = { context: { kind: "sketch", sketch }, dirty: false, session: 0 };
    const result = serializeWorkspace(state, fakeConvert);

    const sky = result.sources.find((s) => s.name === "sky")!;
    expect(sky.payload).toBe(encodeBase64(new Uint8Array([42])));
    expect(result.sources.every((s) => typeof s.payload === "string" && s.payload.length > 0)).toBe(true);
  });

  it("self-contained sketch (no forkedFrom): sources are exactly its own, no demo lookup", () => {
    const s1 = userSource("s1", new Uint8Array([1]));
    const s2 = userSource("s2", new Uint8Array([2]));
    const sketch = newSketchObject("x", [{ name: "main.lua", source: "" }], [s1, s2]);
    const state: OpenSketchState = { context: { kind: "sketch", sketch }, dirty: false, session: 0 };
    const spy = vi.fn(fakeConvert);
    const result = serializeWorkspace(state, spy);

    expect(result.sources.map((s) => s.name).sort()).toEqual(["s1", "s2"]);
    expect(result.sources.every((s) => typeof s.payload === "string" && s.payload.length > 0)).toBe(true);
    expect(spy).not.toHaveBeenCalled();
  });
});

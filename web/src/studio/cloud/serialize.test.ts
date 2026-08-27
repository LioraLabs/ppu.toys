import { describe, expect, it } from "vitest";
import { encodeBase64 } from "../../api/base64";
import { newSketchObject } from "../sketches/sketchStore";
import { serializeWorkspace } from "./serialize";

describe("serializeWorkspace", () => {
  it("serializes only the open toy's files and sources", () => {
    const payload = new Uint8Array([1, 2, 3]);
    const sketch = newSketchObject(
      "toy",
      [{ name: "main.lua", source: "x = 1" }],
      [
        {
          name: "art",
          kind: "bg",
          options: {},
          payload,
          meta: {
            width: 1,
            height: 1,
            report: {
              mode: "tile",
              report: {
                colors_used: 0,
                palettes_used: 0,
                tile_cells: 0,
                unique_tiles: 0,
                vram_words: 0,
                overflows: [],
              },
            },
          },
        },
      ],
    );

    expect(
      serializeWorkspace({ context: { kind: "sketch", sketch }, dirty: false, session: 0 }),
    ).toEqual({
      files: [{ name: "main.lua", source: "x = 1" }],
      sources: [expect.objectContaining({ name: "art", payload: encodeBase64(payload) })],
    });
  });
});

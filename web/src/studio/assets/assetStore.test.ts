import { describe, it, expect } from "vitest";
import { assetId, registerAsset, type Asset } from "./assetStore";

function fakeImage(w: number, h: number): ImageData {
  return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4), colorSpace: "srgb" } as ImageData;
}

describe("assetId", () => {
  it("slugifies a filename and strips the extension", () => {
    expect(assetId("Sky Background.png", [])).toBe("sky_background");
  });
  it("falls back to 'asset' when the name has no usable chars", () => {
    expect(assetId("!!!.png", [])).toBe("asset");
  });
  it("dedupes against taken ids with numeric suffixes", () => {
    expect(assetId("sky.png", ["sky"])).toBe("sky_2");
    expect(assetId("sky.png", ["sky", "sky_2"])).toBe("sky_3");
  });
});

describe("registerAsset", () => {
  it("generates an id, calls upload with it, and returns the asset", () => {
    const uploads: [string, ImageData][] = [];
    const upload = (slot: string, image: ImageData) => uploads.push([slot, image]);
    const img = fakeImage(16, 32);
    const asset = registerAsset(upload, [], { name: "hills.png", imageData: img, preview: "data:," });
    expect(asset.id).toBe("hills");
    expect(asset.width).toBe(16);
    expect(asset.height).toBe(32);
    expect(asset.preview).toBe("data:,");
    expect(uploads).toEqual([["hills", img]]);
  });
  it("gives two same-named uploads distinct ids", () => {
    const upload = () => {};
    const a: Asset[] = [];
    const first = registerAsset(upload, a, { name: "sky.png", imageData: fakeImage(1, 1), preview: "" });
    a.push(first);
    const second = registerAsset(upload, a, { name: "sky.png", imageData: fakeImage(1, 1), preview: "" });
    expect(first.id).toBe("sky");
    expect(second.id).toBe("sky_2");
  });
});

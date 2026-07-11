import { describe, it, expect } from "vitest";
import { assetId } from "./assetStore";

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

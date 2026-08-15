import { describe, it, expect } from "vitest";
import { KIND_LABEL } from "./AssetsPanel";

// PPU-94: the asset badge used to be a ternary chain whose fallthrough arm
// returned "M7", so a tilesheet asset would have badged itself M7 with nothing
// failing. The Record makes a missing kind a compile error; this pins that the
// four kinds actually badge as themselves.
describe("KIND_LABEL", () => {
  it("badges every source kind as itself, sheet included", () => {
    expect(KIND_LABEL).toEqual({ bg: "BG", m7: "M7", obj: "OBJ", sheet: "SHEET" });
  });
});

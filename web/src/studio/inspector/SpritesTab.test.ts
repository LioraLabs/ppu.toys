import { describe, it, expect } from "vitest";
import { objOverflowBadges } from "./SpritesTab";

describe("objOverflowBadges", () => {
  it("emits both badges when both flags set", () => {
    expect(
      objOverflowBadges({ rangeOver: true, timeOver: true, maxSprites: 40, maxTiles: 68 }),
    ).toEqual(["RANGE OVER", "TIME OVER"]);
  });
  it("emits only the active flag", () => {
    expect(
      objOverflowBadges({ rangeOver: true, timeOver: false, maxSprites: 33, maxTiles: 33 }),
    ).toEqual(["RANGE OVER"]);
  });
  it("emits nothing when clear or undefined", () => {
    expect(objOverflowBadges({ rangeOver: false, timeOver: false, maxSprites: 5, maxTiles: 5 })).toEqual([]);
    expect(objOverflowBadges(undefined)).toEqual([]);
  });
});

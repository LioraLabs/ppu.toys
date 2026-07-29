import { describe, expect, it } from "vitest";
import { INSPECTOR_TABS } from "./tabs";

describe("inspector tab model", () => {
  it("lists the workspace tabs first, aux tabs appended", () => {
    expect(INSPECTOR_TABS.map((t) => t.id)).toEqual([
      "trace",
      "memory",
      "compose",
      "windows",
      "m7",
      "registers",
      "sprites",
      "vram",
    ]);
    expect(INSPECTOR_TABS.filter((t) => t.aux).map((t) => t.id)).toEqual([
      "registers",
      "sprites",
      "vram",
    ]);
  });
});

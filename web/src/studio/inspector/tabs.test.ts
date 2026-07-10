import { describe, expect, it } from "vitest";
import { INSPECTOR_TABS, overlayForTab } from "./tabs";

describe("inspector tab model", () => {
  it("lists the four Workspace tabs first, aux tabs appended", () => {
    expect(INSPECTOR_TABS.map((t) => t.id)).toEqual([
      "trace",
      "memory",
      "compose",
      "windows",
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

  it("Expand routes Trace/Memory to the Memory & Layers overlay", () => {
    expect(overlayForTab("trace")).toBe("memory-layers");
    expect(overlayForTab("memory")).toBe("memory-layers");
  });

  it("Expand routes Compose/Windows to the Compositor overlay", () => {
    expect(overlayForTab("compose")).toBe("compositor");
    expect(overlayForTab("windows")).toBe("compositor");
  });

  it("aux tabs have no overlay", () => {
    expect(overlayForTab("registers")).toBeNull();
    expect(overlayForTab("sprites")).toBeNull();
    expect(overlayForTab("vram")).toBeNull();
  });
});

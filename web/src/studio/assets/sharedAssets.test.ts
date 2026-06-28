import { describe, it, expect } from "vitest";
import { assetStore } from "./sharedAssets";
import type { Asset } from "./assetStore";

function asset(id: string): Asset {
  return { id, name: id + ".png", width: 8, height: 8, preview: "data:," };
}

describe("shared asset store", () => {
  it("starts empty-or-current, adds, and notifies subscribers", () => {
    const seen: number[] = [];
    const unsub = assetStore.subscribe(() => seen.push(assetStore.list().length));
    const start = assetStore.list().length;
    assetStore.add(asset("hero"));
    expect(assetStore.list().length).toBe(start + 1);
    expect(seen.length).toBeGreaterThan(0);
    unsub();
  });

  it("set() upserts by id (replace, not duplicate)", () => {
    const a = { id: "track", name: "track.png", width: 8, height: 8, preview: "data:," };
    assetStore.set(a);
    const n = assetStore.list().filter((x) => x.id === "track").length;
    assetStore.set({ ...a, width: 16 });
    expect(assetStore.list().filter((x) => x.id === "track").length).toBe(n);
    expect(assetStore.list().find((x) => x.id === "track")!.width).toBe(16);
  });
});

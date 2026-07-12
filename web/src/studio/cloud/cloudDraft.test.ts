import { describe, it, expect, beforeEach } from "vitest";
import { cloudDraft } from "./cloudDraft";

beforeEach(() => {
  cloudDraft._resetForTests();
});

describe("cloudDraft", () => {
  it("starts with no bound draft", () => {
    expect(cloudDraft.current(0)).toBeNull();
  });

  it("binds an id to a session; a stale session reads null", () => {
    cloudDraft.set("abc", 0);
    expect(cloudDraft.current(0)).toBe("abc");
    expect(cloudDraft.current(1)).toBeNull();
  });

  it("clear resets the binding", () => {
    cloudDraft.set("abc", 0);
    cloudDraft.clear();
    expect(cloudDraft.current(0)).toBeNull();
  });

  it("subscribe fires on set and clear", () => {
    let calls = 0;
    const unsub = cloudDraft.subscribe(() => calls++);
    cloudDraft.set("abc", 0);
    expect(calls).toBe(1);
    cloudDraft.clear();
    expect(calls).toBe(2);
    unsub();
    cloudDraft.set("def", 0);
    expect(calls).toBe(2);
  });

  it("clear is a no-op emit-wise when already clear", () => {
    let calls = 0;
    const unsub = cloudDraft.subscribe(() => calls++);
    cloudDraft.clear();
    expect(calls).toBe(0);
    unsub();
  });
});

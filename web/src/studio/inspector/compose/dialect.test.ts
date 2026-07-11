import { afterEach, describe, expect, it, vi } from "vitest";
import { DIALECT_STORAGE_KEY, loadDialect, parseDialect, pokeDialect } from "./dialect";

/** Minimal localStorage stand-in (node env has none). */
function fakeStorage(): Pick<Storage, "getItem" | "setItem"> {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, String(v)),
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
  pokeDialect.set("friendly"); // module-level store: reset for the next test
});

describe("parseDialect", () => {
  it("friendly is the default for null/garbage; raw only on the exact token", () => {
    expect(parseDialect(null)).toBe("friendly");
    expect(parseDialect("banana")).toBe("friendly");
    expect(parseDialect("raw")).toBe("raw");
    expect(parseDialect("friendly")).toBe("friendly");
  });
});

describe("pokeDialect store", () => {
  it("defaults to friendly (node has no storage — the try/catch fallback)", () => {
    expect(pokeDialect.get()).toBe("friendly");
  });

  it("set flips the value and notifies subscribers", () => {
    let calls = 0;
    const unsub = pokeDialect.subscribe(() => calls++);
    pokeDialect.set("raw");
    expect(pokeDialect.get()).toBe("raw");
    expect(calls).toBe(1);
    unsub();
    pokeDialect.set("friendly");
    expect(calls).toBe(1); // unsubscribed
  });

  it("persists: set writes the storage key, loadDialect round-trips it", () => {
    vi.stubGlobal("localStorage", fakeStorage());
    pokeDialect.set("raw");
    expect(localStorage.getItem(DIALECT_STORAGE_KEY)).toBe("raw");
    expect(loadDialect()).toBe("raw");
    pokeDialect.set("friendly");
    expect(loadDialect()).toBe("friendly");
  });

  it("loadDialect survives absent/garbage storage (friendly)", () => {
    expect(loadDialect()).toBe("friendly"); // no global at all
    vi.stubGlobal("localStorage", { getItem: () => "banana" });
    expect(loadDialect()).toBe("friendly");
  });
});

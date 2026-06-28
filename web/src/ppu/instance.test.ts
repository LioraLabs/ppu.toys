import { describe, it, expect } from "vitest";
import { coreKind } from "./instance";

describe("coreKind", () => {
  it("defaults to mock before bootstrap selects the wasm core", () => {
    expect(coreKind()).toBe("mock");
  });
});

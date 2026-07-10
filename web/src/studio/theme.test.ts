import { describe, expect, it } from "vitest";
import { nextTheme, parseTheme } from "./theme";

describe("theme helpers", () => {
  it("parseTheme defaults to dark for unknown/absent values", () => {
    expect(parseTheme(null)).toBe("dark");
    expect(parseTheme(undefined)).toBe("dark");
    expect(parseTheme("banana")).toBe("dark");
    expect(parseTheme("dark")).toBe("dark");
  });

  it("parseTheme accepts light", () => {
    expect(parseTheme("light")).toBe("light");
  });

  it("nextTheme toggles between the two themes", () => {
    expect(nextTheme("dark")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
  });
});

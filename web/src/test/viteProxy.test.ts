import { describe, expect, it } from "vitest";
import { shouldBypassApiProxy } from "../viteProxy";

describe("Cosmos API proxy", () => {
  it("serves source modules locally while proxying backend API routes", () => {
    expect(shouldBypassApiProxy("/api/apiClient.ts")).toBe(true);
    expect(shouldBypassApiProxy("/api/apiClient.ts?t=123")).toBe(true);
    expect(shouldBypassApiProxy("/api/me")).toBe(false);
    expect(shouldBypassApiProxy("/api/toys?id=1")).toBe(false);
  });
});

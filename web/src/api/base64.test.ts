import { describe, it, expect } from "vitest";
import { decodeBase64 } from "./base64";

describe("decodeBase64", () => {
  it("decodes to the original bytes", () => {
    // btoa(String.fromCharCode(1,2,3,255)) === "AQID/w=="
    expect(Array.from(decodeBase64("AQID/w=="))).toEqual([1, 2, 3, 255]);
  });
  it("decodes empty string to empty bytes", () => {
    expect(decodeBase64("").length).toBe(0);
  });
});

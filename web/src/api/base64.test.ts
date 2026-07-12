import { describe, it, expect } from "vitest";
import { decodeBase64, encodeBase64 } from "./base64";

describe("decodeBase64", () => {
  it("decodes to the original bytes", () => {
    // btoa(String.fromCharCode(1,2,3,255)) === "AQID/w=="
    expect(Array.from(decodeBase64("AQID/w=="))).toEqual([1, 2, 3, 255]);
  });
  it("decodes empty string to empty bytes", () => {
    expect(decodeBase64("").length).toBe(0);
  });
});

describe("encodeBase64", () => {
  it("round-trips an empty array", () => {
    const a = new Uint8Array([]);
    expect(decodeBase64(encodeBase64(a))).toEqual(a);
  });

  it("round-trips a small array", () => {
    const a = new Uint8Array([1, 2, 3, 255, 0]);
    expect(decodeBase64(encodeBase64(a))).toEqual(a);
  });

  it("round-trips a 100_000-byte array without overflowing the call stack", () => {
    const a = new Uint8Array(100_000);
    for (let i = 0; i < a.length; i++) a[i] = i % 256;
    expect(decodeBase64(encodeBase64(a))).toEqual(a);
  });
});

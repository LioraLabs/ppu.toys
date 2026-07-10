import { describe, expect, it } from "vitest";
import { transport } from "../../transport/transport";
import { screensFor } from "./screens";

/** Runs against the REAL singletons (mock core + shared transport), which is
 *  the point: it verifies the wiring the tabs and overlay share. */

describe("screensFor", () => {
  it("caches per frame object and hands back core buffers", () => {
    const frame = transport.getSnapshot().frame;
    const s = screensFor(frame);
    expect(screensFor(frame)).toBe(s);
    expect(s.main.length).toBe(256 * 224 * 4);
    expect(s.sub.length).toBe(256 * 224 * 4);
    expect(s.mathMask.length).toBe(256 * 224);
  });
});

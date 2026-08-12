import { describe, expect, it } from "vitest";
import { HEIGHT, WIDTH, WIN_STRIDE } from "../../../ppu/core";
import { REG } from "./model";
import {
  WIN_SCANLINE_REGS,
  boundsAt,
  dimOutsideSweptMask,
  edgeTrace,
  makeSweep,
  readAt,
  regAt,
  rowCount,
  sweptMask,
  variesAcrossFrame,
  varyingRegs,
} from "./winScanlines";

/** The frame's own (scanline-0) registers a real Compositor would read. */
const base = (addr: number) => (addr === REG.WH1 ? 200 : addr === REG.TM ? 0x1f : 0);

/** An hdma-swept iris: W1 spans a circle's chord on each row. */
function iris(cx = 128, cy = 112, r = 70): Uint8Array {
  const rows = new Uint8Array(HEIGHT * WIN_STRIDE);
  for (let y = 0; y < HEIGHT; y++) {
    const o = y * WIN_STRIDE;
    const inside = r * r - (y - cy) * (y - cy);
    const hw = inside < 0 ? -1 : Math.floor(Math.sqrt(inside));
    rows[o] = hw < 0 ? 1 : cx - hw;
    rows[o + 1] = hw < 0 ? 0 : cx + hw;
    rows[o + 2] = 1; // W2 held as an empty span (lo > hi) so the mask is the iris alone
    rows[o + 4] = 0x02; // W12SEL: BG1 follows W1, flat across the frame
  }
  return rows;
}

/** A static window: every row identical. */
function flat(lo = 64, hi = 192): Uint8Array {
  const rows = new Uint8Array(HEIGHT * WIN_STRIDE);
  for (let y = 0; y < HEIGHT; y++) {
    rows[y * WIN_STRIDE] = lo;
    rows[y * WIN_STRIDE + 1] = hi;
  }
  return rows;
}

describe("winScanlines buffer layout", () => {
  it("declares the eleven registers the Rust core packs, in stride order", () => {
    expect(WIN_SCANLINE_REGS).toHaveLength(WIN_STRIDE);
    expect(WIN_SCANLINE_REGS.slice(0, 4)).toEqual([REG.WH0, REG.WH1, REG.WH2, REG.WH3]);
    expect(WIN_SCANLINE_REGS[WIN_STRIDE - 1]).toBe(REG.TSW);
  });

  it("counts whole rows and clamps out-of-range scanlines into the buffer", () => {
    const rows = iris();
    expect(rowCount(rows)).toBe(HEIGHT);
    expect(regAt(rows, -5, REG.WH0)).toBe(regAt(rows, 0, REG.WH0));
    expect(regAt(rows, 9999, REG.WH0)).toBe(regAt(rows, HEIGHT - 1, REG.WH0));
  });

  it("reports null for a register the feed does not carry, and for an empty feed", () => {
    expect(regAt(iris(), 50, REG.CGWSEL)).toBeNull();
    expect(regAt(new Uint8Array(0), 0, REG.WH0)).toBeNull();
    expect(rowCount(new Uint8Array(0))).toBe(0);
  });
});

describe("the scanline lens", () => {
  it("reads window registers from the selected row", () => {
    const rows = iris();
    // at the circle's centre the chord is widest; at the top it is empty
    expect(readAt(rows, 112, base)(REG.WH0)).toBe(58);
    expect(readAt(rows, 112, base)(REG.WH1)).toBe(198);
    expect(readAt(rows, 0, base)(REG.WH0)).toBe(1); // lo > hi, the empty span
  });

  it("falls through to the frame snapshot for non-window registers", () => {
    expect(readAt(iris(), 50, base)(REG.TM)).toBe(0x1f);
  });

  it("falls through entirely when there is no feed yet (pre-first-frame)", () => {
    const at = readAt(new Uint8Array(0), 0, base);
    expect(at(REG.WH1)).toBe(200); // the frame's own scanline-0 value
    expect(at(REG.TM)).toBe(0x1f);
  });

  it("bounds track the scrubber", () => {
    const rows = iris();
    expect(boundsAt(rows, 112, base).wh1 - boundsAt(rows, 112, base).wh0).toBeGreaterThan(
      boundsAt(rows, 60, base).wh1 - boundsAt(rows, 60, base).wh0,
    );
  });
});

describe("hdma detection", () => {
  it("flags only the registers the hook actually moves", () => {
    const rows = iris();
    expect(variesAcrossFrame(rows, REG.WH0)).toBe(true);
    expect(variesAcrossFrame(rows, REG.WH1)).toBe(true);
    expect(variesAcrossFrame(rows, REG.W12SEL)).toBe(false);
    expect(variesAcrossFrame(rows, REG.WH2)).toBe(false); // held flat, not swept
    expect(varyingRegs(rows)).toEqual([REG.WH0, REG.WH1]);
  });

  it("reports a static window as not swept — row 0 IS the whole frame", () => {
    expect(varyingRegs(flat())).toEqual([]);
    expect(makeSweep(flat(), 0).varying.size).toBe(0);
  });

  it("reports an empty feed as not swept rather than throwing", () => {
    expect(varyingRegs(new Uint8Array(0))).toEqual([]);
  });
});

describe("edge traces", () => {
  it("returns one x per scanline, curving with the sweep", () => {
    const xs = edgeTrace(iris(), REG.WH0, base);
    expect(xs).toHaveLength(HEIGHT);
    expect(xs[112]).toBe(58);
    expect(xs[0]).not.toBe(xs[112]);
  });

  it("degenerates to the frame's flat value with no feed", () => {
    const xs = edgeTrace(new Uint8Array(0), REG.WH1, base);
    expect(xs).toHaveLength(HEIGHT);
    expect(new Set(xs)).toEqual(new Set([200]));
  });
});

describe("swept mask", () => {
  it("is a full-frame mask whose inside-width follows each row's bounds", () => {
    const mask = sweptMask(iris(), base, 0, false);
    expect(mask).toHaveLength(WIDTH * HEIGHT);
    const widthAt = (y: number) =>
      mask.subarray(y * WIDTH, (y + 1) * WIDTH).reduce((n, v) => n + v, 0);
    // the iris is widest at its centre row and empty above the circle
    expect(widthAt(112)).toBeGreaterThan(widthAt(60));
    expect(widthAt(0)).toBe(0);
  });

  it("is uniform down the frame for a static window", () => {
    const mask = sweptMask(flat(), base, 0, false);
    const row0 = mask.subarray(0, WIDTH);
    for (const y of [1, 100, HEIGHT - 1]) {
      expect(mask.subarray(y * WIDTH, (y + 1) * WIDTH)).toEqual(row0);
    }
    expect(row0[64]).toBe(1);
    expect(row0[192]).toBe(1);
    expect(row0[63]).toBe(0);
  });

  it("inverts with the outside flag", () => {
    const inside = sweptMask(flat(), base, 0, false);
    const outside = sweptMask(flat(), base, 0, true);
    for (let p = 0; p < inside.length; p++) expect(outside[p]).toBe(inside[p] ? 0 : 1);
  });
});

describe("dimOutsideSweptMask", () => {
  it("keeps masked-in pixels and dims the rest per scanline", () => {
    const fb = new Uint8ClampedArray(WIDTH * HEIGHT * 4).fill(200);
    const mask = new Uint8Array(WIDTH * HEIGHT);
    mask[0] = 1; // only the very first pixel is inside
    const out = dimOutsideSweptMask(fb, mask);
    expect([out[0], out[1], out[2], out[3]]).toEqual([200, 200, 200, 255]);
    expect([out[4], out[5], out[6]]).toEqual([60, 60, 84]); // x0.3 / x0.3 / x0.42
  });
});

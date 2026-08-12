import { describe, expect, it } from "vitest";
import { WIN_SHAPES, WIN_SHAPE_DEFAULTS, winSnippet } from "./winSnippet";

describe("winSnippet", () => {
  it("wraps every shape in an hdma hook over the chosen range", () => {
    for (const s of WIN_SHAPES) {
      const out = winSnippet(s.id, { ...WIN_SHAPE_DEFAULTS, y0: 32, y1: 191 });
      expect(out).toContain("hdma(32, 191, function(y)");
      expect(out.trimEnd().endsWith("end)")).toBe(true);
      expect(out).toContain("win.w1.");
    }
  });

  it("generates the spotlight demo's iris maths", () => {
    const out = winSnippet("iris", { ...WIN_SHAPE_DEFAULTS, cx: 128, cy: 112, r: 70 });
    expect(out).toContain("local dy = y - 112");
    expect(out).toContain("local inside = 70*70 - dy*dy");
    expect(out).toContain("local hw = floor(sqrt(inside))");
    expect(out).toContain("win.w1.lo, win.w1.hi = 128 - hw, 128 + hw");
    // the out-of-circle rows use an empty span, not a zero-width one
    expect(out).toContain("win.w1.lo, win.w1.hi = 1, 0");
  });

  it("formats fractional params to 3dp with trailing zeros trimmed", () => {
    expect(winSnippet("wipe", { ...WIN_SHAPE_DEFAULTS, slope: 0.5 })).toContain("* 0.5");
    expect(winSnippet("wipe", { ...WIN_SHAPE_DEFAULTS, slope: -2 })).toContain("* -2");
    expect(winSnippet("wipe", { ...WIN_SHAPE_DEFAULTS, slope: 0.25 })).toContain("* 0.25");
  });

  it("bands the bars shape on the chosen period", () => {
    const out = winSnippet("bars", { ...WIN_SHAPE_DEFAULTS, period: 16 });
    expect(out).toContain("if floor(y / 16) % 2 == 0 then");
    expect(out).toContain("win.w1.lo, win.w1.hi = 0, 255");
  });

  it("names the panel and the call site so a pasted snippet explains itself", () => {
    const out = winSnippet("iris", WIN_SHAPE_DEFAULTS);
    expect(out.split("\n")[0]).toBe(
      "-- Windows editor: iris (call inside frame(), after apply_pokes())",
    );
    expect(out).toContain("win.color.w1 = true");
  });
});

import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { ppuCompletions } from "./completions";

function complete(doc: string): CompletionResult | null {
  const state = EditorState.create({ doc });
  const ctx = new CompletionContext(state, doc.length, true);
  return ppuCompletions(ctx) as CompletionResult | null;
}

describe("ppuCompletions", () => {
  it("offers flat DSL globals on a bare word", () => {
    const res = complete("bri");
    expect(res).not.toBeNull();
    const labels = res!.options.map((o) => o.label);
    expect(labels).toContain("brightness");
    expect(labels).toContain("direct_color");
    expect(labels).toContain("force_blank");
    expect(labels).toContain("mode");
    expect(labels).toContain("hdma");
    expect(labels).toContain("rgb");
  });

  it("offers math.* members after `math.`", () => {
    const res = complete("math.s");
    expect(res).not.toBeNull();
    const labels = res!.options.map((o) => o.label);
    expect(labels).toContain("sin");
    expect(labels).toContain("sqrt");
    expect(labels).not.toContain("brightness");
  });

  it("offers obj.* members after `obj.`", () => {
    const res = complete("obj.");
    const labels = res!.options.map((o) => o.label);
    expect(labels).toContain("sheet");
    expect(labels).toContain("size_sel");
    expect(labels).toContain("name_select");
    expect(labels).toContain("char_base");
  });

  it("offers m7.* members after `m7.`", () => {
    const res = complete("m7.");
    expect(res).not.toBeNull();
    const labels = res!.options.map((o) => o.label);
    expect(labels).toContain("extbg");
    expect(labels).toContain("cx");
    expect(labels).not.toContain("brightness");
  });

  it("returns null when there is no word to complete", () => {
    const state = EditorState.create({ doc: "x = 1 " });
    const ctx = new CompletionContext(state, 6, false);
    expect(ppuCompletions(ctx)).toBeNull();
  });
});

describe("M8 DSL audit", () => {
  it("offers window/color-math/screen registers and vram as globals", () => {
    const labels = complete("W")!.options.map((o) => o.label);
    for (const g of ["TM", "TS", "WH0", "WH1", "WH2", "WH3", "W12SEL", "W34SEL",
      "WOBJSEL", "WBGLOG", "WOBJLOG", "TMW", "TSW", "CGWSEL", "CGADSUB", "COLDATA",
      "coldata", "m7pixel", "vram"]) {
      expect(labels).toContain(g);
    }
  });

  it("annotates completions with their hardware register", () => {
    const opts = complete("m")!.options;
    const detail = (label: string) => opts.find((o) => o.label === label)?.detail ?? "";
    expect(detail("mosaic")).toContain("$2106");
    expect(detail("brightness")).toContain("$2100");
    expect(detail("cgram")).toContain("$2121");
    expect(detail("hdma")).toContain("HDMA");
  });

  it("offers bg[n]. layer members", () => {
    const labels = complete("bg[1].")!.options.map((o) => o.label);
    for (const m of ["scroll", "source", "visible", "tile_size", "map_base",
      "screen_size", "char_base", "mosaic", "map"]) {
      expect(labels).toContain(m);
    }
    expect(labels).not.toContain("brightness");
  });

  it("offers sprite members on indexed obj[n]. but sheet/OBSEL on plain obj.", () => {
    const sprite = complete("obj[12].")!.options.map((o) => o.label);
    for (const m of ["x", "y", "tile", "pal", "prio", "flip_x", "flip_y", "on", "large"]) {
      expect(sprite).toContain(m);
    }
    expect(sprite).not.toContain("sheet");
    const plain = complete("obj.")!.options.map((o) => o.label);
    expect(plain).toContain("sheet");
    expect(plain).toContain("priority_rotate");
    expect(plain).toContain("oam_addr");
    expect(plain).not.toContain("large");
  });

  it("completes the partial word after bg[n].", () => {
    const res = complete("bg[2].sc")!;
    expect(res.options.map((o) => o.label)).toContain("scroll");
    // `from` must sit right after the dot so "sc" is replaced, not appended
    expect(res.from).toBe("bg[2].".length);
  });

  it("does NOT treat user identifiers ending in a DSL name as member access", () => {
    // myobj. / subbg[1]. / xm7. are user variables, not obj/bg/m7
    expect(complete("myobj.")!.options.map((o) => o.label)).not.toContain("sheet");
    expect(complete("subbg[1].")!.options.map((o) => o.label)).not.toContain("scroll");
    expect(complete("xm7.")!.options.map((o) => o.label)).not.toContain("extbg");
    // ...while the real bases still complete mid-expression
    expect(complete("x = obj.")!.options.map((o) => o.label)).toContain("sheet");
  });

  it("offers the obj.first priority-rotation sugar", () => {
    expect(complete("obj.")!.options.map((o) => o.label)).toContain("first");
  });
});

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

  it("offers obj.sheet after `obj.`", () => {
    const res = complete("obj.");
    expect(res).not.toBeNull();
    expect(res!.options.map((o) => o.label)).toContain("sheet");
  });

  it("returns null when there is no word to complete", () => {
    const state = EditorState.create({ doc: "x = 1 " });
    const ctx = new CompletionContext(state, 6, false);
    expect(ppuCompletions(ctx)).toBeNull();
  });
});

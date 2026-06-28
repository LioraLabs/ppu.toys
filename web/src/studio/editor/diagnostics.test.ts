import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { luaErrorToDiagnostics, luaErrorsToDiagnostics } from "./diagnostics";

const DOC = "function frame(t, f)\n  brightness = bad\nend\n";

describe("luaErrorToDiagnostics", () => {
  it("returns no diagnostics when there is no error", () => {
    const state = EditorState.create({ doc: DOC });
    expect(luaErrorToDiagnostics(state, undefined)).toEqual([]);
  });

  it("maps a LuaError line to that line's range", () => {
    const state = EditorState.create({ doc: DOC });
    const diags = luaErrorToDiagnostics(state, { message: "boom", line: 2 });
    expect(diags).toHaveLength(1);
    expect(diags[0].severity).toBe("error");
    expect(diags[0].message).toBe("boom");
    const line2 = state.doc.line(2);
    expect(diags[0].from).toBe(line2.from);
    expect(diags[0].to).toBe(line2.to);
  });

  it("clamps out-of-range / missing lines to the whole document", () => {
    const state = EditorState.create({ doc: DOC });
    const diags = luaErrorToDiagnostics(state, { message: "no line" });
    expect(diags).toHaveLength(1);
    expect(diags[0].from).toBe(0);
    expect(diags[0].to).toBe(state.doc.length);
  });
});

describe("luaErrorsToDiagnostics", () => {
  it("merges compile + runtime errors without dropping either", () => {
    const state = EditorState.create({ doc: DOC });
    const diags = luaErrorsToDiagnostics(state, [
      { message: "compile boom", line: 1 },
      { message: "runtime boom", line: 2 },
    ]);
    expect(diags.map((d) => d.message)).toEqual(["compile boom", "runtime boom"]);
  });

  it("skips undefined entries and dedupes identical errors", () => {
    const state = EditorState.create({ doc: DOC });
    const diags = luaErrorsToDiagnostics(state, [
      undefined,
      { message: "same", line: 2 },
      { message: "same", line: 2 },
    ]);
    expect(diags).toHaveLength(1);
    expect(diags[0].message).toBe("same");
  });
});

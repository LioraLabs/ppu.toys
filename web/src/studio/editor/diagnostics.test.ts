import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { luaErrorToDiagnostics, luaErrorsToDiagnostics, routeErrorsByFile } from "./diagnostics";

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

describe("routeErrorsByFile", () => {
  const files = ["main.lua", "palette.lua"];

  it("routes errors to their owning file", () => {
    const routed = routeErrorsByFile(files, "main.lua", [
      { message: "boom", line: 2, file: "palette.lua" },
    ]);
    expect(routed.get("palette.lua")).toEqual([{ message: "boom", line: 2, file: "palette.lua" }]);
    expect(routed.has("main.lua")).toBe(false);
  });

  it("falls back to the active file for missing or unknown file attribution", () => {
    const routed = routeErrorsByFile(files, "palette.lua", [
      { message: "no file" },
      { message: "ghost", file: "deleted.lua" },
    ]);
    expect(routed.get("palette.lua")!.map((e) => e.message)).toEqual(["no file", "ghost"]);
  });

  it("skips undefined entries and groups several errors per file", () => {
    const routed = routeErrorsByFile(files, "main.lua", [
      undefined,
      { message: "compile", file: "main.lua" },
      { message: "runtime", file: "main.lua" },
    ]);
    expect(routed.get("main.lua")!.map((e) => e.message)).toEqual(["compile", "runtime"]);
    expect(routed.size).toBe(1);
  });
});

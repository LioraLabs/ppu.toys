import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import {
  bindWarningsByFile,
  luaErrorToDiagnostics,
  luaErrorsToDiagnostics,
  routeErrorsByFile,
} from "./diagnostics";

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

describe("bindWarningsByFile", () => {
  const files = [
    { name: "main.lua", source: 'mode = 1\ndma("sky", { char = 0x1000 })\n' },
    { name: "sprites.lua", source: "dma('hero', { char = 0x4000 })\n" },
  ];

  it("attributes a mismatch to the file+line that names the slot, as a warning", () => {
    const out = bindWarningsByFile(files, [
      { mode: "mismatch", layer: 0, slot: "sky", expected: "bg 4bpp", found: "bg 8bpp" },
    ]);
    const [w] = out.get("main.lua")!;
    expect(w.line).toBe(2);
    expect(w.severity).toBe("warning");
    expect(w.message).toContain('dma("sky") not placed');
    expect(w.message).toContain("needs bg 4bpp, found bg 8bpp");
  });

  it("matches single-quoted slots and keeps the dma label when layer is absent", () => {
    const out = bindWarningsByFile(files, [
      { mode: "mismatch", slot: "hero", expected: "obj", found: "no source with this name" },
    ]);
    const [w] = out.get("sprites.lua")!;
    expect(w.line).toBe(1);
    expect(w.message).toContain('dma("hero") not placed');
  });

  it("skips a slot named in no file (runtime-built names stay inspector-only)", () => {
    const out = bindWarningsByFile(files, [
      { mode: "mismatch", layer: 1, slot: "dynamic", expected: "bg 4bpp", found: "obj" },
    ]);
    expect(out.size).toBe(0);
  });
});

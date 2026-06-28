import type { EditorState } from "@codemirror/state";
import type { Diagnostic } from "@codemirror/lint";
import type { LuaError } from "../../ppu/core";

/** Map a compile-time LuaError onto an editor diagnostic. A valid 1-based line is
 *  highlighted whole; a missing/out-of-range line falls back to the whole doc. */
export function luaErrorToDiagnostics(
  state: EditorState,
  error: LuaError | undefined,
): Diagnostic[] {
  if (!error) return [];
  const lineCount = state.doc.lines;
  if (error.line && error.line >= 1 && error.line <= lineCount) {
    const line = state.doc.line(error.line);
    return [{ from: line.from, to: line.to, severity: "error", message: error.message }];
  }
  return [{ from: 0, to: state.doc.length, severity: "error", message: error.message }];
}

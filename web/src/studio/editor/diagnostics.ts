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

/** Map several LuaErrors (e.g. compile + runtime) onto editor diagnostics.
 *  Undefined entries are skipped; diagnostics with the same range+message are
 *  deduped so a compile error that equals the runtime error shows once. */
export function luaErrorsToDiagnostics(
  state: EditorState,
  errors: (LuaError | undefined)[],
): Diagnostic[] {
  const out: Diagnostic[] = [];
  const seen = new Set<string>();
  for (const error of errors) {
    for (const d of luaErrorToDiagnostics(state, error)) {
      const key = `${d.from}:${d.to}:${d.message}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(d);
    }
  }
  return out;
}

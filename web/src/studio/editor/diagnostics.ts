import type { EditorState } from "@codemirror/state";
import type { Diagnostic } from "@codemirror/lint";
import type { ImportReport, LuaError, SourceFile } from "../../ppu/core";

/** Map a compile-time LuaError onto an editor diagnostic. A valid 1-based line is
 *  highlighted whole; a missing/out-of-range line falls back to the whole doc. */
export function luaErrorToDiagnostics(
  state: EditorState,
  error: LuaError | undefined,
): Diagnostic[] {
  if (!error) return [];
  const severity = error.severity ?? "error";
  const lineCount = state.doc.lines;
  if (error.line && error.line >= 1 && error.line <= lineCount) {
    const line = state.doc.line(error.line);
    return [{ from: line.from, to: line.to, severity, message: error.message }];
  }
  return [{ from: 0, to: state.doc.length, severity, message: error.message }];
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

/** Turn bind-time source mismatches into per-file editor warnings, attributed
 *  by SEARCHING each file for the quoted slot name (`"slot"` or `'slot'`) —
 *  the bind has no line info of its own. First textual hit wins (file order =
 *  execution order); a slot named nowhere (name built at runtime) is skipped
 *  here and remains visible in the inspector's import health lines. */
export function bindWarningsByFile(
  files: SourceFile[],
  mismatches: Extract<ImportReport, { mode: "mismatch" }>[],
): Map<string, LuaError[]> {
  const out = new Map<string, LuaError[]>();
  for (const m of mismatches) {
    const message = `dma("${m.slot}") not placed — needs ${m.expected}, found ${m.found}`;
    for (const f of files) {
      const idx = [`"${m.slot}"`, `'${m.slot}'`]
        .map((q) => f.source.indexOf(q))
        .filter((i) => i >= 0)
        .sort((a, b) => a - b)[0];
      if (idx === undefined) continue;
      const line = f.source.slice(0, idx).split("\n").length; // 1-based
      const entry: LuaError = { message, line, file: f.name, severity: "warning" };
      const list = out.get(f.name);
      if (list) list.push(entry);
      else out.set(f.name, [entry]);
      break;
    }
  }
  return out;
}

/** Route errors to their owning file (multi-file sketches): an error whose
 *  `file` matches a listed file belongs to that file's tab; missing/unknown
 *  attribution follows the ACTIVE file — better shown where the user is than
 *  dropped. Consumers: inline diagnostics for the open tab, error dots for
 *  the rest. */
export function routeErrorsByFile(
  fileNames: string[],
  activeFile: string,
  errors: (LuaError | undefined)[],
): Map<string, LuaError[]> {
  const out = new Map<string, LuaError[]>();
  for (const e of errors) {
    if (!e) continue;
    const owner = e.file && fileNames.includes(e.file) ? e.file : activeFile;
    const list = out.get(owner);
    if (list) list.push(e);
    else out.set(owner, [e]);
  }
  return out;
}

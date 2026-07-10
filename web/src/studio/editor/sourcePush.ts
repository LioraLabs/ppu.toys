import type { LuaError, SourceFile } from "../../ppu/core";

/** Debounce window between the last edit and the setSources push (~200ms,
 *  M9 live-run model). Error grace lives in the engine: a failed compile
 *  keeps the last good program running. */
export const SOURCE_PUSH_MS = 200;

export interface SourcePusher {
  /** Debounced push — coalesces bursts of keystrokes. */
  push(files: SourceFile[]): void;
  /** Immediate push (session open, ▶ Run); cancels any pending debounce. */
  pushNow(files: SourceFile[]): void;
  dispose(): void;
}

function sameFiles(a: SourceFile[] | null, b: SourceFile[]): boolean {
  return (
    a !== null &&
    a.length === b.length &&
    a.every((f, i) => f.name === b[i].name && f.source === b[i].source)
  );
}

/** Push the whole multi-file program to a PpuCore.setSources-shaped sink,
 *  debounced and content-deduped (re-emits of an already-pushed program —
 *  autosave flushes, tab switches — don't recompile). `onResult` receives the
 *  compile error, or undefined when the program compiled clean. */
export function createSourcePusher(
  sink: (files: SourceFile[]) => { ok: boolean; error?: LuaError },
  onResult: (error: LuaError | undefined) => void,
  ms: number = SOURCE_PUSH_MS,
): SourcePusher {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let last: SourceFile[] | null = null;
  const cancel = () => {
    if (timer) clearTimeout(timer);
    timer = null;
  };
  const run = (files: SourceFile[]) => {
    if (sameFiles(last, files)) return;
    last = files.map((f) => ({ ...f }));
    onResult(sink(files).error);
  };
  return {
    push(files) {
      cancel();
      timer = setTimeout(() => {
        timer = null;
        run(files);
      }, ms);
    },
    pushNow(files) {
      cancel();
      run(files);
    },
    dispose: cancel,
  };
}

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { LuaError, SourceFile } from "../../ppu/core";
import { createSourcePusher, SOURCE_PUSH_MS } from "./sourcePush";

const A: SourceFile = { name: "a.lua", source: "-- a" };
const B: SourceFile = { name: "b.lua", source: "-- b" };

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function makeSink(error?: LuaError) {
  const calls: SourceFile[][] = [];
  const sink = (files: SourceFile[]) => {
    calls.push(files);
    return { ok: !error, error };
  };
  return { calls, sink };
}

describe("createSourcePusher", () => {
  it("debounces a burst of edits into one setSources call with the latest files", () => {
    const { calls, sink } = makeSink();
    const p = createSourcePusher(sink, () => {});
    p.push([A]);
    p.push([{ ...A, source: "-- a2" }]);
    vi.advanceTimersByTime(SOURCE_PUSH_MS);
    expect(calls).toEqual([[{ name: "a.lua", source: "-- a2" }]]);
  });

  it("a reorder pushes the files in the NEW execution order", () => {
    const { calls, sink } = makeSink();
    const p = createSourcePusher(sink, () => {});
    p.pushNow([A, B]);
    p.push([B, A]); // drag-reorder
    vi.advanceTimersByTime(SOURCE_PUSH_MS);
    expect(calls).toHaveLength(2);
    expect(calls[1].map((f) => f.name)).toEqual(["b.lua", "a.lua"]);
  });

  it("dedupes content-identical re-emits (autosave flush, tab switch)", () => {
    const { calls, sink } = makeSink();
    const p = createSourcePusher(sink, () => {});
    p.pushNow([A, B]);
    p.push([{ ...A }, { ...B }]); // same content, new identities
    vi.advanceTimersByTime(SOURCE_PUSH_MS);
    expect(calls).toHaveLength(1);
  });

  it("pushNow cancels a pending debounce and pushes immediately", () => {
    const { calls, sink } = makeSink();
    const p = createSourcePusher(sink, () => {});
    p.push([A]);
    p.pushNow([B]);
    vi.advanceTimersByTime(SOURCE_PUSH_MS * 2);
    expect(calls).toEqual([[B]]);
  });

  it("reports the sink's compile error (and its clearing) through onResult", () => {
    const results: (LuaError | undefined)[] = [];
    const err: LuaError = { message: "boom", line: 1, file: "a.lua" };
    let fail = true;
    const p = createSourcePusher(
      () => (fail ? { ok: false, error: err } : { ok: true }),
      (e) => results.push(e),
    );
    p.pushNow([A]);
    fail = false;
    p.pushNow([B]);
    expect(results).toEqual([err, undefined]);
  });

  it("dispose cancels a pending push", () => {
    const { calls, sink } = makeSink();
    const p = createSourcePusher(sink, () => {});
    p.push([A]);
    p.dispose();
    vi.advanceTimersByTime(SOURCE_PUSH_MS * 2);
    expect(calls).toHaveLength(0);
  });
});

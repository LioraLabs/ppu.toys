import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { history, undo, undoDepth } from "@codemirror/commands";
import { createDocStates } from "./docStates";

/** Apply an insertion as a user edit so it lands in undo history. */
function type(state: EditorState, text: string): EditorState {
  return state.update({
    changes: { from: state.doc.length, insert: text },
    userEvent: "input.type",
  }).state;
}

describe("createDocStates", () => {
  it("creates a state from the seed doc on first acquire, then ignores the seed", () => {
    const docs = createDocStates([history()]);
    const a = docs.acquire("a", "-- a");
    expect(a.doc.toString()).toBe("-- a");
    docs.store("a", type(a, "\nx = 1"));
    expect(docs.acquire("a", "STALE SEED").doc.toString()).toBe("-- a\nx = 1");
  });

  it("preserves per-file undo history across tab swaps", () => {
    const docs = createDocStates([history()]);
    let a = docs.acquire("a", "-- a");
    a = type(a, "\nx = 1");
    docs.store("a", a);
    // switch to b, edit it, switch back
    let b = docs.acquire("b", "-- b");
    b = type(b, "\ny = 2");
    docs.store("b", b);
    let back = docs.acquire("a", "");
    expect(undoDepth(back)).toBeGreaterThan(0);
    undo({ state: back, dispatch: (tr) => (back = tr.state) });
    expect(back.doc.toString()).toBe("-- a");
    // b's history untouched by a's undo
    expect(docs.acquire("b", "").doc.toString()).toBe("-- b\ny = 2");
  });

  it("keeps distinct docs fully independent", () => {
    const docs = createDocStates([history()]);
    docs.store("a", type(docs.acquire("a", "aaa"), "!"));
    expect(docs.acquire("b", "bbb").doc.toString()).toBe("bbb");
    expect(undoDepth(docs.acquire("b", ""))).toBe(0);
  });
});

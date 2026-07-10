import { EditorState, type Extension } from "@codemirror/state";

/** Per-file EditorState registry: ONE CodeMirror view swaps whole
 *  EditorStates between tabs, so each file keeps its own undo history and
 *  selection (history lives inside EditorState). Keys are stable doc
 *  identities — the pane keeps a key across renames and never reuses one
 *  after a delete. */
export interface DocStates {
  /** State for `key`, created from `doc` the first time the key is seen. */
  acquire(key: string, doc: string): EditorState;
  /** Save the live state back under `key` (call before switching away). */
  store(key: string, state: EditorState): void;
}

export function createDocStates(extensions: Extension): DocStates {
  const states = new Map<string, EditorState>();
  return {
    acquire(key, doc) {
      let s = states.get(key);
      if (!s) {
        s = EditorState.create({ doc, extensions });
        states.set(key, s);
      }
      return s;
    },
    store(key, state) {
      states.set(key, state);
    },
  };
}

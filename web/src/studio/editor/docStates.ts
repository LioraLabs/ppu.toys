import { EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

/** Per-file EditorState registry: ONE CodeMirror view swaps whole
 *  EditorStates between tabs, so each file keeps its own undo history and
 *  selection (history lives inside EditorState). Keys are stable doc
 *  identities — the pane keeps a key across renames and never reuses one
 *  after a delete. */
export interface DocStates {
  /** State for `key`, created from `doc` the first time the key is seen.
   *  When `generated` is set, the state is read-only and gets REBUILT (not
   *  reused) whenever `doc` differs from the cached content — a generated
   *  file's truth lives outside the editor (e.g. the inspector writes
   *  pokes.lua), so external changes must always show through. */
  acquire(key: string, doc: string, generated?: boolean): EditorState;
  /** Save the live state back under `key` (call before switching away). */
  store(key: string, state: EditorState): void;
}

/** Appended to the shared extensions for generated (read-only) docs: blocks
 *  edits at both the state (readOnly) and view (editable) levels — vim's
 *  editing commands become no-ops, selection/yank still work. */
const READ_ONLY_EXTENSIONS: Extension = [
  EditorState.readOnly.of(true),
  EditorView.editable.of(false),
];

export function createDocStates(extensions: Extension): DocStates {
  const states = new Map<string, EditorState>();
  return {
    acquire(key, doc, generated = false) {
      let s = states.get(key);
      if (generated) {
        if (!s || s.doc.toString() !== doc) {
          s = EditorState.create({ doc, extensions: [extensions, READ_ONLY_EXTENSIONS] });
          states.set(key, s);
        }
        return s;
      }
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

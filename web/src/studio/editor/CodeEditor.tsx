import { useEffect, useRef } from "react";
import { type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { StreamLanguage } from "@codemirror/language";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { autocompletion } from "@codemirror/autocomplete";
import { lintGutter, setDiagnostics } from "@codemirror/lint";
import { vim } from "@replit/codemirror-vim";
import { basicSetup } from "codemirror";
import type { LuaError } from "../../ppu/core";
import { ppuCompletions } from "./completions";
import { luaErrorsToDiagnostics } from "./diagnostics";
import { createDocStates, type DocStates } from "./docStates";
import { ppuTheme } from "./theme";

export interface CodeEditorProps {
  /** Stable identity of the active doc — survives renames, never reused
   *  after a delete (the pane owns key allocation). */
  docKey: string;
  /** Seed content for a doc this editor instance has not seen yet. */
  doc: string;
  /** True for a machine-generated, read-only doc (pokes.lua) — the state is
   *  rebuilt from `doc` whenever it changes externally instead of treating
   *  the editor buffer as the source of truth. */
  generated?: boolean;
  /** Called on every document change with the new source. */
  onChange: (src: string) => void;
  /** Errors already routed to THIS doc (compile + runtime), see
   *  routeErrorsByFile. */
  errors: (LuaError | undefined)[];
}

/** ONE CodeMirror view for the whole pane; per-file EditorStates swap through
 *  it so tab switches preserve undo history (docStates). Source pushing and
 *  error routing live in EditorPane — this component only edits and displays. */
export function CodeEditor({ docKey, doc, generated = false, onChange, errors }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const docsRef = useRef<DocStates | null>(null);
  const keyRef = useRef(docKey);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const initial = useRef({ docKey, doc, generated });

  useEffect(() => {
    if (!host.current) return;
    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged) onChangeRef.current(u.state.doc.toString());
    });
    const extensions: Extension[] = [
      vim({ status: true }), // first: takes key precedence
      basicSetup,
      StreamLanguage.define(lua),
      autocompletion({ override: [ppuCompletions] }),
      lintGutter(),
      ppuTheme,
      updateListener,
    ];
    const docs = createDocStates(extensions);
    docsRef.current = docs;
    keyRef.current = initial.current.docKey;
    const view = new EditorView({
      parent: host.current,
      state: docs.acquire(initial.current.docKey, initial.current.doc, initial.current.generated),
    });
    viewRef.current = view;
    return () => {
      viewRef.current = null;
      docsRef.current = null;
      view.destroy();
    };
    // one view per mount; live props are read via refs
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // tab switch: save the outgoing state, swap in the incoming one. A
  // generated doc also refreshes IN PLACE (same key) when its source
  // changes externally — e.g. the inspector rewrites pokes.lua while its
  // tab is open — since its truth lives outside the editor buffer.
  useEffect(() => {
    const view = viewRef.current;
    const docs = docsRef.current;
    if (!view || !docs) return;
    if (keyRef.current === docKey) {
      if (generated && view.state.doc.toString() !== doc) {
        view.setState(docs.acquire(docKey, doc, generated));
      }
      return;
    }
    docs.store(keyRef.current, view.state);
    keyRef.current = docKey;
    view.setState(docs.acquire(docKey, doc, generated));
  }, [docKey, doc, generated]);

  // (re)display the routed diagnostics for the active doc
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch(setDiagnostics(view.state, luaErrorsToDiagnostics(view.state, errors)));
  }, [errors, docKey]);

  return <div ref={host} className="cm-host" style={{ height: "100%" }} />;
}

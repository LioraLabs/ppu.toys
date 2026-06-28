import { useEffect, useRef } from "react";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { StreamLanguage } from "@codemirror/language";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { autocompletion } from "@codemirror/autocomplete";
import { lintGutter, setDiagnostics } from "@codemirror/lint";
import { vim } from "@replit/codemirror-vim";
import { basicSetup } from "codemirror";
import type { LuaError } from "../../ppu/core";
import { ppuCompletions } from "./completions";
import { luaErrorToDiagnostics } from "./diagnostics";
import { ppuTheme } from "./theme";

export interface CodeEditorProps {
  initialDoc: string;
  /** PpuCore.setSource-shaped callback; called on every document change. */
  onSource: (src: string) => { ok: boolean; error?: LuaError };
}

export function CodeEditor({ initialDoc, onSource }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  // keep latest onSource without re-creating the editor
  const onSourceRef = useRef(onSource);
  onSourceRef.current = onSource;

  useEffect(() => {
    if (!host.current) return;

    const pushSource = (view: EditorView, src: string) => {
      const { error } = onSourceRef.current(src);
      view.dispatch(setDiagnostics(view.state, luaErrorToDiagnostics(view.state, error)));
    };

    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged) pushSource(u.view, u.state.doc.toString());
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

    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({ doc: initialDoc, extensions }),
    });

    // run an initial compile so setSource is called + diagnostics seed on mount
    pushSource(view, initialDoc);

    return () => view.destroy();
    // editor is created once; live onSource is read via onSourceRef
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={host} className="cm-host" style={{ height: "100%" }} />;
}

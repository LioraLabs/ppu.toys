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
import { luaErrorsToDiagnostics } from "./diagnostics";
import { ppuTheme } from "./theme";

export interface CodeEditorProps {
  initialDoc: string;
  /** PpuCore.setSource-shaped callback; called on every document change. */
  onSource: (src: string) => { ok: boolean; error?: LuaError };
  /** Latest runtime error from the transport, shown alongside compile errors. */
  runtimeError?: LuaError;
}

export function CodeEditor({ initialDoc, onSource, runtimeError }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // keep latest onSource without re-creating the editor
  const onSourceRef = useRef(onSource);
  onSourceRef.current = onSource;
  // latest compile + runtime errors, so either source can re-dispatch the merged set
  const compileErrorRef = useRef<LuaError | undefined>(undefined);
  const runtimeErrorRef = useRef<LuaError | undefined>(runtimeError);
  runtimeErrorRef.current = runtimeError;

  useEffect(() => {
    if (!host.current) return;

    const applyDiagnostics = (view: EditorView) => {
      view.dispatch(
        setDiagnostics(
          view.state,
          luaErrorsToDiagnostics(view.state, [compileErrorRef.current, runtimeErrorRef.current]),
        ),
      );
    };

    const pushSource = (view: EditorView, src: string) => {
      compileErrorRef.current = onSourceRef.current(src).error;
      applyDiagnostics(view);
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
    viewRef.current = view;

    // run an initial compile so setSource is called + diagnostics seed on mount
    pushSource(view, initialDoc);

    return () => {
      viewRef.current = null;
      view.destroy();
    };
    // editor is created once; live onSource is read via onSourceRef
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // re-dispatch the merged diagnostics whenever the runtime error changes
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch(
      setDiagnostics(
        view.state,
        luaErrorsToDiagnostics(view.state, [compileErrorRef.current, runtimeErrorRef.current]),
      ),
    );
  }, [runtimeError]);

  return <div ref={host} className="cm-host" style={{ height: "100%" }} />;
}

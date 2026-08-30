import { CoreStage } from "../cosmos/FixtureStage";
import type { EditorPaneProps } from "./EditorPane";
import { EditorPane } from "./EditorPane";
import "../styles/tokens.css";
import "./studio.css";

function EditorComposition({ onSources }: EditorPaneProps) {
  return (
    <CoreStage>
      <div style={{ display: "flex", width: "100%", height: "100vh" }}>
        <EditorPane onSources={onSources} />
      </div>
    </CoreStage>
  );
}

// Keep these compositions wired to the production EditorPane, not fixture copies.
const Default = () => <EditorComposition onSources={() => ({ ok: true })} />;

const CompileError = () => (
  <EditorComposition
    onSources={() => ({
      ok: false,
      error: { message: "unexpected symbol near ')'", line: 3, file: "main.lua" },
    })}
  />
);

export default { Default, CompileError };

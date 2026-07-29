import { CoreStage } from "../cosmos/FixtureStage";
import "../styles/tokens.css";
import "./studio.css";
import { INSPECTOR_TABS } from "./inspector/tabs";
import { WIRED_INSPECTOR_PANELS } from "./inspector/panels";

// Branch composition: every inspector page, wired to the production core, laid
// out side by side. (The Inspector chrome is gone — in the app each page is a
// dock panel; see StudioDock.fixture for the composed shell.)
export default (
  <CoreStage>
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
        gap: 12,
        padding: 12,
        height: "100vh",
        overflow: "auto",
        background: "var(--bg)",
      }}
    >
      {INSPECTOR_TABS.map((t) => (
        <section
          key={t.id}
          style={{
            display: "flex",
            flexDirection: "column",
            minHeight: 320,
            border: "1px solid var(--line)",
            borderRadius: 8,
            overflow: "hidden",
            background: "var(--panel)",
          }}
        >
          <div className="insp-subhead" style={{ padding: "8px 12px 0" }}>
            {t.label.toUpperCase()}
          </div>
          {WIRED_INSPECTOR_PANELS[t.id]()}
        </section>
      ))}
    </div>
  </CoreStage>
);

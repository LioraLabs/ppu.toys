import {
  AssignmentMatrix,
  ComposeReadout,
  EquationChip,
  MathControls,
  ScreenPreviews,
} from "./compose/ComposeSections";
import { useCompositor } from "./compose/useCompositor";
import {
  BoundCards,
  LayerMaskRows,
  WindowControls,
  WindowPreview,
  WindowReadout,
} from "./compose/WindowSections";
import "./compose/compose.css";

/** Full-screen "Compositor" overlay (⤢ Expand from Compose/Windows). Renders
 *  the SAME section components as the docked tabs over the same pokes.lua +
 *  frame registers — state lives in the sketch files and the core, so edits
 *  made here and in the tabs mirror each other by construction. Layout per the
 *  handoff: controls left (296px), composite + window mask center, register
 *  lists right (300px). */
export function CompositorOverlay({ onCollapse }: { onCollapse: () => void }) {
  const c = useCompositor();
  return (
    <div className="insp-overlay">
      <div className="insp-overlay-bar">
        <span className="insp-overlay-title">Compositor</span>
        <span className="insp-overlay-sub">main/sub screens · color math · windows</span>
        <div className="tb-spacer" />
        <button type="button" className="btn-ghost" onClick={onCollapse}>
          ↩ Collapse
        </button>
      </div>
      <div className="insp-overlay-body cmpo-body">
        <div className="cmpo-left">
          <div className="cmpo-h">SCREEN ASSIGNMENT</div>
          <AssignmentMatrix c={c} />
          <div className="cmpo-h cmpo-h--gap">COLOR MATH</div>
          <MathControls c={c} fill />
        </div>
        <div className="cmpo-center">
          <section>
            <div className="cmpo-h">SCREEN COMPOSITE</div>
            <ScreenPreviews c={c} large />
            <EquationChip c={c} />
          </section>
          <section className="cmpo-divider">
            <div className="cmpo-h">
              WINDOW MASK <span className="cmpo-hint">click the preview to drag the nearest edge</span>
            </div>
            <div className="cmpo-winrow">
              <div className="cmpo-winleft">
                <WindowPreview c={c} />
                <BoundCards c={c} />
              </div>
              <div className="cmpo-winright">
                <WindowControls c={c} />
                <div className="cmp-ctl-label">PER-LAYER WINDOW MASK</div>
                <LayerMaskRows c={c} />
              </div>
            </div>
          </section>
        </div>
        <div className="cmpo-right">
          <div className="cmpo-h">COLOR MATH REGS</div>
          <ComposeReadout c={c} flat />
          <div className="cmpo-h cmpo-h--gap">WINDOW REGS</div>
          <WindowReadout c={c} flat />
        </div>
      </div>
    </div>
  );
}

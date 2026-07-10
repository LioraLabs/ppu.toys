import { PinBar } from "./compose/chrome";
import { useCompositor } from "./compose/useCompositor";
import {
  BoundCards,
  LayerMaskRows,
  WindowControls,
  WindowPreview,
  WindowReadout,
} from "./compose/WindowSections";
import "./compose/compose.css";

/** WINDOWS — the two hardware window masks (W1/W2) and their combine logic.
 *  Every control writes a pinned override through the shared pin store. */
export function WindowsTab() {
  const c = useCompositor();
  return (
    <div className="insp-scroll">
      <PinBar c={c} />
      <WindowPreview c={c} />
      <div className="winp-caption">
        orange = W1 edges · cyan = W2 edges · click preview to drag nearest edge
      </div>
      <WindowControls c={c} />
      <BoundCards c={c} />
      <div className="cmp-ctl-label">PER-LAYER WINDOW MASK</div>
      <LayerMaskRows c={c} />
      <WindowReadout c={c} />
    </div>
  );
}

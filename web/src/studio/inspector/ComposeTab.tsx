import { PinBar } from "./compose/chrome";
import {
  AssignmentMatrix,
  ComposeReadout,
  EquationChip,
  MathControls,
  ScreenPreviews,
} from "./compose/ComposeSections";
import { useCompositor } from "./compose/useCompositor";
import "./compose/compose.css";

/** COMPOSE — main/sub screens + color math. Previews are core buffers
 *  (compositor intermediates + live framebuffer); every control writes a
 *  pinned override through the shared pin store. */
export function ComposeTab() {
  const c = useCompositor();
  return (
    <div className="insp-scroll">
      <PinBar c={c} />
      <ScreenPreviews c={c} />
      <EquationChip c={c} />
      <div className="cmp-cols">
        <AssignmentMatrix c={c} />
        <MathControls c={c} />
      </div>
      <ComposeReadout c={c} />
    </div>
  );
}

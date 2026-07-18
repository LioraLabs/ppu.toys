import type { CompositorScreens } from "../../ppu/core";
import {
  AssignmentMatrix,
  ComposeReadout,
  EquationChip,
  MathControls,
  ScreenPreviews,
} from "./compose/ComposeSections";
import type { Compositor } from "./compose/useCompositor";
import "./compose/compose.css";

/** COMPOSE — main/sub screens + color math. Previews are core buffers
 *  (compositor intermediates + live framebuffer); every control writes a
 *  friendly field poke into the generated pokes.lua. Presentational: `c` and
 *  `screens` are supplied by the caller (wired: ComposeTabWired; stories: a
 *  fixture compositor + fixture screens). */
export function ComposeTab({ c, screens }: { c: Compositor; screens: CompositorScreens }) {
  return (
    <div className="insp-scroll">
      <ScreenPreviews c={c} screens={screens} />
      <EquationChip c={c} />
      <div className="cmp-cols">
        <AssignmentMatrix c={c} />
        <MathControls c={c} />
      </div>
      <ComposeReadout c={c} />
    </div>
  );
}

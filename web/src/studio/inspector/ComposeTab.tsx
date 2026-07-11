import { DialectToggle, PokeBar } from "./compose/chrome";
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
 *  friendly field poke into the generated pokes.lua. */
export function ComposeTab() {
  const c = useCompositor();
  return (
    <div className="insp-scroll">
      <PokeBar c={c} />
      <DialectToggle />
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

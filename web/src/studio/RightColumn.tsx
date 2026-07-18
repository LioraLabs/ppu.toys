import { OutputCanvas } from "./output/OutputCanvas";
import { Inspector } from "./inspector/Inspector";
import { ErrorBoundary } from "./ErrorBoundary";

/** Thin container composing the two genuinely-wired right-column subsystems.
 *  Intentionally left wired (no Ladle story): OutputCanvas owns the rAF/wasm
 *  render loop (out of scope here — the editor/output ticket's domain) and
 *  Inspector is the inspector-frame wired container. Neither renders without a
 *  live core, so a RightColumn story would have to boot wasm — not meaningful.
 *  The wasm-free stories live on the inspector's presentational panels
 *  (RegistersTab, SpritesTab, VramTab, …) which the Inspector wires up. */
export function RightColumn() {
  return (
    <aside className="right">
      <ErrorBoundary label="Output">
        <OutputCanvas />
      </ErrorBoundary>
      <ErrorBoundary label="Inspector">
        <Inspector />
      </ErrorBoundary>
    </aside>
  );
}

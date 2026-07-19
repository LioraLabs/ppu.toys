import { OutputCanvas } from "./output/OutputCanvas";
import { Inspector } from "./inspector/Inspector";
import { ErrorBoundary } from "./ErrorBoundary";

/** Thin container composing the two genuinely-wired right-column subsystems.
 *  Intentionally left wired (no story of its own): OutputCanvas owns the
 *  rAF/wasm render loop and the default Inspector tabs read the shared core.
 *  The composed right column IS visible in Cosmos though — the StudioLayout
 *  fixture (StudioLayout.fixture) rebuilds this aside from fixtures (BlitCanvas
 *  output + slot-injected Inspector), and its LiveCore composition mounts this real
 *  one under `CoreStage`. */
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

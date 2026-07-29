import { OutputCanvas } from "./output/OutputCanvas";
import { ErrorBoundary } from "./ErrorBoundary";

/** The right column is the live output, nothing else — the Inspector moved to
 *  the bottom dock under the editor (StudioLayout's `dock` slot). Intentionally
 *  left wired (no story of its own): OutputCanvas owns the rAF/wasm render
 *  loop. The composed column IS visible in Cosmos though — the StudioLayout
 *  fixture rebuilds it from fixtures (BlitCanvas output), and its LiveCore
 *  composition mounts this real one under `CoreStage`. */
export function RightColumn() {
  return (
    <aside className="right">
      <ErrorBoundary label="Output">
        <OutputCanvas />
      </ErrorBoundary>
    </aside>
  );
}

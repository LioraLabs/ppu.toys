import type { Story, StoryDefault } from "@ladle/react";
import { ModeBadge, PlaneSeg, TraceCaption } from "./tracemem/TraceChain";
import { frameResult } from "../../fixtures";
import "./inspector.css";
import "./tracemem/tracemem.css";

// TraceTab renders <TraceChain .../>, which queries the live rasterizer
// (ppuCore.layerView / traceBgPixel / traceObj / spriteAt) to build its
// resolution-chain canvases — so the full TraceTab can't be rendered
// wasm-free. What CAN be storied is its chrome: the tm-controls row
// (PlaneSeg + ModeBadge) and the TraceCaption directly above the chain,
// reproduced here inside the same insp-scroll container TraceTab uses. See
// TraceChain.stories.tsx for the rasterizer-bound-chain documentation story.
export default {
  title: "Studio/Inspector/TraceTab",
} satisfies StoryDefault;

export const Chrome: Story = () => (
  <div className="insp-scroll">
    <div className="tm-controls">
      <PlaneSeg />
      <ModeBadge frame={frameResult} />
    </div>
    <TraceCaption frame={frameResult} />
  </div>
);

/** Inline, presentational-only explainer — no ppuCore import. */
function FullTabNote() {
  return (
    <div className="insp-scroll">
      <div className="tm-note">
        The full TraceTab embeds TraceChain, which resolves the selected
        pixel/sprite through the live rasterizer (ppuCore.layerView /
        traceBgPixel / traceObj) to render its Stage 1–5 resolution-chain
        canvases. That's a spec non-goal to fake with a PpuCore mock, so it's
        verified in-app rather than storied — see TraceChain.stories.tsx's
        ResolutionChainRequiresCore story. The Chrome story above is
        TraceTab's wasm-free surface: the plane/mode controls and caption
        rendered exactly as TraceTab arranges them.
      </div>
    </div>
  );
}

export const FullTabRequiresCore: Story = () => <FullTabNote />;

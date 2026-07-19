import { ModeBadge, PlaneSeg, TraceCaption } from "./TraceChain";
import { frameResult } from "../../../fixtures";
import "./tracemem.css";

// TraceChain.tsx exports four components. PlaneSeg, ModeBadge, and
// TraceCaption are the wasm-free chrome — they read a plane-selection store
// and a FrameResult, no ppuCore involved — and are storied below. The big
// TraceChain({ frame, copy, variant }) component is the rasterizer-bound
// resolution chain: its Stage 1-5 canvases (SOURCE minimap, CHAR tile,
// SUB-PALETTE, CGRAM COLOR, OUTPUT) are produced live via
// ppuCore.layerView / traceBgPixel / traceObj / spriteAt for the selected
// pixel or sprite. Mocking the whole PpuCore to fake those canvases is a
// spec non-goal, so that component is not storied here — see the
// ResolutionChainRequiresCore documentation story below.
const PlaneSegStory = () => <PlaneSeg />;
PlaneSegStory.storyName = "PlaneSeg";

const ModeBadgeStory = () => <ModeBadge frame={frameResult} />;
ModeBadgeStory.storyName = "ModeBadge";

const TraceCaptionStory = () => <TraceCaption frame={frameResult} />;
TraceCaptionStory.storyName = "TraceCaption";

/** Inline, presentational-only explainer — no ppuCore import. */
function ResolutionChainNote() {
  return (
    <div className="tm-chain">
      <div className="tm-note">
        The Stage 1–5 resolution-chain canvases (SOURCE minimap, CHAR tile,
        SUB-PALETTE, CGRAM COLOR, OUTPUT) are produced by the live
        rasterizer — ppuCore.layerView / traceBgPixel / traceObj — for the
        selected pixel/sprite. They can't be rendered from a static fixture
        without mocking the whole PpuCore (a spec non-goal), so the chain is
        exercised in-app rather than storied. The controls above
        (plane/mode/caption) are the wasm-free chrome and are storied here.
      </div>
    </div>
  );
}

const ResolutionChainRequiresCore = () => <ResolutionChainNote />;

export default {
  PlaneSegStory,
  ModeBadgeStory,
  TraceCaptionStory,
  ResolutionChainRequiresCore,
};

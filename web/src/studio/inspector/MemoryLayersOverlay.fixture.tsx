import { MemoryLayersOverlay } from "./MemoryLayersOverlay";
import { frameImportReports, frameResult, frameVram } from "../../fixtures";
import "./tracemem/tracemem.css";
import "./inspector.css";

// MemoryLayersOverlay is a pure props component: given a FrameResult, VRAM
// image, and ImportReport list it renders the priority stack, import health,
// VRAM/CGRAM panels, and registers with no wasm core on the render path. The
// resolution-chain center panel is injected via the `chain` slot rather than
// faked here — the real chain (TraceChain) reads ppuCore.layerView /
// traceBgPixel / traceObj directly off the live rasterizer, and reproducing
// that from a static fixture would mean mocking the whole PpuCore, which is a
// spec non-goal. The note below documents the omission in place of the chain.
const Default = () => (
  <MemoryLayersOverlay
    onCollapse={() => {}}
    frame={frameResult}
    vram={frameVram}
    reports={frameImportReports}
    chain={() => (
      <div className="tm-note">
        Resolution chain omitted — the Stage 1–5 canvases are produced by the live
        rasterizer (ppuCore.layerView / traceBgPixel / traceObj) and can't be rendered
        from a static fixture without mocking the whole PpuCore (a spec non-goal). The
        VRAM / CGRAM / import-health / register panels around it render wasm-free.
      </div>
    )}
  />
);

export default {
  Default,
};

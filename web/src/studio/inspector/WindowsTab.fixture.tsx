import { WindowsTab } from "./WindowsTab";
import { makeFrameResult, makeSceneFramebuffer, makeWinScanlines } from "../../fixtures";
import { InspectorFrameProvider } from "./useInspectorFrame";
import "./compose/compose.css";
import "./inspector.css";
import "../pokes/pokes.css";

// WindowsTab is presentational for the one thing it can't compute — the
// per-scanline window feed (wired: WindowsTabWired reads ppuCore.winScanlines()).
// Everything else routes through useCompositor(), which reads the
// useInspectorFrame seam + the poke store, both wasm-free; its WindowPreview
// blits c.frame.framebuffer through pure mask fns and never touches ppuCore. So
// InspectorFrameProvider + a fixture feed drives the whole tab with no core.
//
// The frame carries makeSceneFramebuffer() rather than the default all-black
// one: the preview's job is to dim what the mask excludes, and that only reads
// against a scene with actual pixels in it.
const frame = makeFrameResult({ framebuffer: makeSceneFramebuffer() });

// The spotlight demo's shape: W1 traces a circle's chord per scanline, so the
// mask cuts a real iris and the WH0/WH1 edges draw as curves.
const Swept = () => (
  <InspectorFrameProvider frame={frame}>
    <WindowsTab rows={makeWinScanlines()} />
  </InspectorFrameProvider>
);

// The static case: every scanline carries the same bounds, so the edges draw as
// straight lines, the badge reads "static" and the scrubber changes nothing.
const Static = () => (
  <InspectorFrameProvider frame={frame}>
    <WindowsTab rows={makeWinScanlines({ sweep: false })} />
  </InspectorFrameProvider>
);

// Before the first frame the core returns an empty buffer — the panel must fall
// back to the frame's own (scanline-0) registers rather than blanking.
const NoFeed = () => (
  <InspectorFrameProvider frame={frame}>
    <WindowsTab rows={new Uint8Array(0)} />
  </InspectorFrameProvider>
);

export default {
  Swept,
  Static,
  NoFeed,
};

import * as Sections from "./WindowSections";
import { frameResult, makeWinScanlines } from "../../../fixtures";
import { makeFixtureCompositor } from "./storyCompositor";
import { makeSweep } from "./winScanlines";
import "./compose.css";
import "../inspector.css";
import "../../pokes/pokes.css";

// WindowSections are the leaf panels WindowsTab composes: given a Compositor
// (here, makeFixtureCompositor — reads registers via the pure liveReg, writes
// are inert no-ops) plus a WinSweep (the per-scanline feed) they render with no
// wasm core on the render path. WindowPreview blits c.frame.framebuffer, which
// the fixture leaves zero-filled — it shows black with the colored W1/W2 edge
// traces, which is expected here (no rasterizer involved).
const c = makeFixtureCompositor(frameResult);
const swept = makeSweep(makeWinScanlines(), 112);
const flat = makeSweep(makeWinScanlines({ sweep: false }), 112);

const noop = () => {};

const WindowPreview = () => <Sections.WindowPreview c={c} w={swept} onScanline={noop} />;

const ScanlinePicker = () => <Sections.ScanlinePicker w={swept} onScanline={noop} />;

const ScanlinePickerStatic = () => <Sections.ScanlinePicker w={flat} onScanline={noop} />;

const WindowControls = () => <Sections.WindowControls c={c} />;

const BoundCards = () => <Sections.BoundCards c={c} w={swept} />;

const LayerMaskRows = () => <Sections.LayerMaskRows c={c} />;

const WindowReadout = () => <Sections.WindowReadout c={c} w={swept} />;

const WindowShapeTool = () => <Sections.WindowShapeTool />;

// The scanline poke dialect's edit surface: two swept edges keyframed over the
// whole frame, plus a THIRD poke scoped to a band. The chip on the selected
// line is highlighted; the value column shows what the curve holds there (the
// same maths the generated pki() will run), struck through on a poke whose
// hook does not cover the selected line.
const kfTracks: Sections.ScanlineTrack[] = [
  {
    lvalue: "win.w1.lo",
    kf: [
      { y: 0, v: 128 },
      { y: 112, v: 58 },
      { y: 223, v: 128 },
    ],
    range: [0, 223],
  },
  {
    lvalue: "win.w1.hi",
    kf: [
      { y: 0, v: 128 },
      { y: 112, v: 198 },
    ],
    range: [0, 223],
  },
  // scoped to the top band: at line 112 its hook does not run
  {
    lvalue: "win.w2.lo",
    kf: [
      { y: 0, v: 10 },
      { y: 95, v: 60 },
    ],
    range: [0, 95],
  },
];

const KeyframeTrack = () => (
  <Sections.KeyframeTrack
    tracks={kfTracks}
    dialect="scanline"
    y={112}
    onScanline={noop}
    onRemove={noop}
    onRange={noop}
  />
);

// No scanline pokes yet, but the dialect is selected — the panel explains how
// to make one instead of showing an empty box.
const KeyframeTrackEmpty = () => (
  <Sections.KeyframeTrack
    tracks={[]}
    dialect="scanline"
    y={64}
    onScanline={noop}
    onRemove={noop}
    onRange={noop}
  />
);

export default {
  WindowPreview,
  ScanlinePicker,
  ScanlinePickerStatic,
  KeyframeTrack,
  KeyframeTrackEmpty,
  WindowControls,
  BoundCards,
  LayerMaskRows,
  WindowReadout,
  WindowShapeTool,
};

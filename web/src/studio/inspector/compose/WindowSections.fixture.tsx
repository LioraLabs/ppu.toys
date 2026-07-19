import * as Sections from "./WindowSections";
import { frameResult } from "../../../fixtures";
import { makeFixtureCompositor } from "./storyCompositor";
import "./compose.css";
import "../inspector.css";
import "../../pokes/pokes.css";

// WindowSections are the leaf panels WindowsTab composes: given a Compositor
// (here, makeFixtureCompositor — reads registers via the pure liveReg, writes
// are inert no-ops) they render with no wasm core on the render path.
// WindowPreview blits c.frame.framebuffer, which the fixture leaves
// zero-filled — it shows black with the colored W1/W2 edge lines, which is
// expected here (no rasterizer involved).
const c = makeFixtureCompositor(frameResult);

const WindowPreview = () => <Sections.WindowPreview c={c} />;

const WindowControls = () => <Sections.WindowControls c={c} />;

const BoundCards = () => <Sections.BoundCards c={c} />;

const LayerMaskRows = () => <Sections.LayerMaskRows c={c} />;

const WindowReadout = () => <Sections.WindowReadout c={c} />;

export default {
  WindowPreview,
  WindowControls,
  BoundCards,
  LayerMaskRows,
  WindowReadout,
};

import * as Sections from "./ComposeSections";
import { frameResult, frameScreens } from "../../../fixtures";
import { makeFixtureCompositor } from "./storyCompositor";
import "./compose.css";
import "../inspector.css";
import "../../pokes/pokes.css";

// ComposeSections are the leaf panels ComposeTab composes: given a Compositor
// (here, makeFixtureCompositor — reads registers via the pure liveReg, writes
// are inert no-ops) and the fixture CompositorScreens, they render with no
// wasm core on the render path.
const c = makeFixtureCompositor(frameResult);

const ScreenPreviews = () => <Sections.ScreenPreviews c={c} screens={frameScreens} />;

const ScreenPreviewsLarge = () => (
  <Sections.ScreenPreviews c={c} screens={frameScreens} large />
);

const EquationChip = () => <Sections.EquationChip c={c} />;

const AssignmentMatrix = () => <Sections.AssignmentMatrix c={c} />;

const MathControls = () => <Sections.MathControls c={c} />;

const ComposeReadout = () => <Sections.ComposeReadout c={c} />;

export default {
  ScreenPreviews,
  ScreenPreviewsLarge,
  EquationChip,
  AssignmentMatrix,
  MathControls,
  ComposeReadout,
};

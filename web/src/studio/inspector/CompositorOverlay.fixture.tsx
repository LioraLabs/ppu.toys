import { CompositorOverlay } from "./CompositorOverlay";
import { makeFixtureCompositor } from "./compose/storyCompositor";
import { frameResult, frameScreens } from "../../fixtures";
import "./compose/compose.css";
import "./inspector.css";
import "../pokes/pokes.css";

// CompositorOverlay is a pure props component: given a Compositor (`c`) and
// CompositorScreens (`screens`) it renders the screen-assignment matrix, color
// math, window mask, and register readouts with no wasm core on the render
// path. makeFixtureCompositor derives a Compositor from a fixture FrameResult
// via the pure liveReg reader; writes are inert no-ops. Renders wasm-free.
const Default = () => (
  <CompositorOverlay onCollapse={() => {}} c={makeFixtureCompositor(frameResult)} screens={frameScreens} />
);

export default {
  Default,
};

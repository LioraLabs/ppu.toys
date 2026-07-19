import { RegistersTab } from "./RegistersTab";
import { frameResult } from "../../fixtures";
import { InspectorFrameProvider, useInspectorFrame } from "./useInspectorFrame";
import "./inspector.css";
import "../pokes/pokes.css";

// RegistersTab is a pure props component: given a FrameResult it renders the
// MODE/MAIN/SUB/MATH/MOSAIC/WIN summary, register rows, and CGRAM swatches with
// no wasm core on the render path. HexPoke only calls the poke store on user
// click, so it's safe to render here.
const Default = () => <RegistersTab frame={frameResult} />;

const Waiting = () => <RegistersTab frame={null} />;

// Proves the seam: a wired consumer reading useInspectorFrame() through
// InspectorFrameProvider renders identically to the direct-prop Default story,
// with no transport subscription and no wasm core involved.
function WiredRegisters() {
  const frame = useInspectorFrame();
  return <RegistersTab frame={frame} />;
}

const ViaInspectorFrameSeam = () => (
  <InspectorFrameProvider frame={frameResult}>
    <WiredRegisters />
  </InspectorFrameProvider>
);

export default {
  Default,
  Waiting,
  ViaInspectorFrameSeam,
};

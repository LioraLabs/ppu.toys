import { MemoryTab } from "./MemoryTab";
import { frameResult, frameVram } from "../../fixtures";
import "./inspector.css";
import "./tracemem/tracemem.css";

// MemoryTab is now a pure props component: given a FrameResult and a VRAM
// image, the VRAM regions + CGRAM ownership derive purely from
// frame.registers / vram / frame.oam, with no wasm core on the render path.
const Default = () => <MemoryTab frame={frameResult} vram={frameVram} />;

export default {
  Default,
};

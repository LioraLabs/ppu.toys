import { frameResult, frameVram } from "../../../fixtures";
import { MemoryTab } from "../MemoryTab";
import "./tracemem.css";
import "../inspector.css";

// MemoryTab is the production assembly of the trace-memory panels.
export default <MemoryTab frame={frameResult} vram={frameVram} />;

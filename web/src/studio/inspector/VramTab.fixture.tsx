import { VramTab } from "./VramTab";
import { frameResult, frameVram, frameImportReports } from "../../fixtures";
import "./inspector.css";

// VramTab is a pure props component: given a FrameResult plus VRAM and import
// reports it renders the tile/tilemap/CGRAM viewer with no wasm core on the
// render path.
const Default = () => (
  <VramTab frame={frameResult} vram={frameVram} reports={frameImportReports} />
);

const EmptyReports = () => <VramTab frame={frameResult} vram={frameVram} reports={[]} />;

const Waiting = () => <VramTab frame={null} vram={frameVram} reports={[]} />;

export default {
  Default,
  EmptyReports,
  Waiting,
};

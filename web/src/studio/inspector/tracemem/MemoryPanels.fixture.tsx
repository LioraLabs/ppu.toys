import { CgramGrid, VramBar, VramLegend } from "./MemoryPanels";
import { frameResult, frameVram } from "../../../fixtures";
import { cgramOwners, vramRegions } from "./regions";
import "./tracemem.css";

// MemoryPanels are the leaf panels MemoryTab composes: VRAM regions +
// CGRAM ownership derive purely from frame.registers / vram / frame.oam
// (tracemem/regions.ts), with no wasm core on the render path.
const regions = vramRegions(frameResult.registers, frameVram);
const owners = cgramOwners(frameResult.registers, frameVram, frameResult.oam);
const noop = () => {};

const VramBarStory = () => <VramBar regions={regions} onCopy={noop} />;
VramBarStory.storyName = "VramBar";

const VramLegendStory = () => <VramLegend regions={regions} onCopy={noop} />;
VramLegendStory.storyName = "VramLegend";

const CgramGridStory = () => <CgramGrid cgram={frameResult.cgram} owners={owners} />;
CgramGridStory.storyName = "CgramGrid";

export default {
  VramBarStory,
  VramLegendStory,
  CgramGridStory,
};

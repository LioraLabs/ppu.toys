import type { Story, StoryDefault } from "@ladle/react";
import { CgramGrid, VramBar, VramLegend } from "./MemoryPanels";
import { frameResult, frameVram } from "../../../fixtures";
import { cgramOwners, vramRegions } from "./regions";
import "./tracemem.css";

// MemoryPanels are the leaf panels MemoryTab composes: VRAM regions +
// CGRAM ownership derive purely from frame.registers / vram / frame.oam
// (tracemem/regions.ts), with no wasm core on the render path.
export default {
  title: "Studio/Inspector/Tracemem/MemoryPanels",
} satisfies StoryDefault;

const regions = vramRegions(frameResult.registers, frameVram);
const owners = cgramOwners(frameResult.registers, frameVram, frameResult.oam);
const noop = () => {};

export const VramBarStory: Story = () => <VramBar regions={regions} onCopy={noop} />;
VramBarStory.storyName = "VramBar";

export const VramLegendStory: Story = () => <VramLegend regions={regions} onCopy={noop} />;
VramLegendStory.storyName = "VramLegend";

export const CgramGridStory: Story = () => <CgramGrid cgram={frameResult.cgram} owners={owners} />;
CgramGridStory.storyName = "CgramGrid";

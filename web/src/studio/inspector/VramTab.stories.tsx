import type { Story, StoryDefault } from "@ladle/react";
import { VramTab } from "./VramTab";
import { frameResult, frameVram, frameImportReports } from "../../fixtures";
import "./inspector.css";

// VramTab is a pure props component: given a FrameResult plus VRAM and import
// reports it renders the tile/tilemap/CGRAM viewer with no wasm core on the
// render path.
export default {
  title: "Studio/Inspector/VramTab",
} satisfies StoryDefault;

export const Default: Story = () => (
  <VramTab frame={frameResult} vram={frameVram} reports={frameImportReports} />
);

export const EmptyReports: Story = () => <VramTab frame={frameResult} vram={frameVram} reports={[]} />;

export const Waiting: Story = () => <VramTab frame={null} vram={frameVram} reports={[]} />;

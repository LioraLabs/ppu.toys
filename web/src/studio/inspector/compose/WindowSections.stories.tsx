import type { Story, StoryDefault } from "@ladle/react";
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
export default {
  title: "Studio/Inspector/Compose/WindowSections",
} satisfies StoryDefault;

const c = makeFixtureCompositor(frameResult);

export const WindowPreview: Story = () => <Sections.WindowPreview c={c} />;

export const WindowControls: Story = () => <Sections.WindowControls c={c} />;

export const BoundCards: Story = () => <Sections.BoundCards c={c} />;

export const LayerMaskRows: Story = () => <Sections.LayerMaskRows c={c} />;

export const WindowReadout: Story = () => <Sections.WindowReadout c={c} />;

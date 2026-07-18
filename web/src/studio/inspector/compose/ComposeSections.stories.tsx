import type { Story, StoryDefault } from "@ladle/react";
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
export default {
  title: "Studio/Inspector/Compose/ComposeSections",
} satisfies StoryDefault;

const c = makeFixtureCompositor(frameResult);

export const ScreenPreviews: Story = () => <Sections.ScreenPreviews c={c} screens={frameScreens} />;

export const ScreenPreviewsLarge: Story = () => (
  <Sections.ScreenPreviews c={c} screens={frameScreens} large />
);

export const EquationChip: Story = () => <Sections.EquationChip c={c} />;

export const AssignmentMatrix: Story = () => <Sections.AssignmentMatrix c={c} />;

export const MathControls: Story = () => <Sections.MathControls c={c} />;

export const ComposeReadout: Story = () => <Sections.ComposeReadout c={c} />;

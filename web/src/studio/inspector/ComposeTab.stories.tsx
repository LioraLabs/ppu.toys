import type { Story, StoryDefault } from "@ladle/react";
import { ComposeTab } from "./ComposeTab";
import { frameResult, frameScreens } from "../../fixtures";
import { makeFixtureCompositor } from "./compose/storyCompositor";
import "./compose/compose.css";
import "./inspector.css";
import "../pokes/pokes.css";

// ComposeTab is now presentational: the compositor is built from a fixture
// frame (makeFixtureCompositor — reads registers via the pure liveReg, writes
// are inert no-ops) and screens are the fixture CompositorScreens
// (frameScreens), so no wasm core touches the render path.
export default {
  title: "Studio/Inspector/ComposeTab",
} satisfies StoryDefault;

const c = makeFixtureCompositor(frameResult);

export const Default: Story = () => <ComposeTab c={c} screens={frameScreens} />;

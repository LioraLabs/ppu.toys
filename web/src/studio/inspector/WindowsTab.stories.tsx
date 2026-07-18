import type { Story, StoryDefault } from "@ladle/react";
import { WindowsTab } from "./WindowsTab";
import { frameResult } from "../../fixtures";
import { InspectorFrameProvider } from "./useInspectorFrame";
import "./compose/compose.css";
import "./inspector.css";
import "../pokes/pokes.css";

// WindowsTab needs no presentational/wired split: it calls useCompositor()
// internally, which reads the useInspectorFrame seam + the poke store — both
// wasm-free. Its WindowPreview blits c.frame.framebuffer and uses pure mask
// fns, never touching ppuCore. Wrapping in InspectorFrameProvider drives the
// whole tab off a fixture frame with no wasm core involved.
export default {
  title: "Studio/Inspector/WindowsTab",
} satisfies StoryDefault;

export const Default: Story = () => (
  <InspectorFrameProvider frame={frameResult}>
    <WindowsTab />
  </InspectorFrameProvider>
);

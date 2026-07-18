import type { Story, StoryDefault } from "@ladle/react";
import { AddSourceButton } from "./AddSourceButton";
import "./sources.css";

// AddSourceButton is a self-contained toggle: it renders the "+ Source" ghost
// button and mounts AddSourceDialog on click. Rendered closed here (the button
// alone); the dialog has its own story. No wasm on the render path.
export default {
  title: "Studio/Sources/AddSourceButton",
} satisfies StoryDefault;

export const Default: Story = () => (
  <div style={{ padding: 16 }}>
    <AddSourceButton />
  </div>
);

import { useState } from "react";
import type { Story, StoryDefault } from "@ladle/react";
import { CgramPoke } from "./CgramPoke";
import { frameCgram } from "../../fixtures";
import "./pokes.css";

// CgramPoke is a prop-driven popover: given a CGRAM index + current BGR555 value
// it renders a native color picker and live readout. The `onChange` seam is
// injected here so no poke store (and no wasm) is on the render path — the story
// just tracks the picked value locally.
export default {
  title: "Studio/Pokes/CgramPoke",
} satisfies StoryDefault;

// The popover positions against a `position: relative` container the caller
// provides; this frame stands in for the swatch/trigger cell.
function Anchor({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ position: "relative", width: 240, height: 200, padding: 24 }}>{children}</div>
  );
}

export const Default: Story = () => {
  const [value, setValue] = useState(frameCgram[0x81] ?? 0);
  return (
    <Anchor>
      <CgramPoke index={0x81} current={value} onChange={setValue} onClose={() => undefined} />
    </Anchor>
  );
};

export const BackdropColor: Story = () => {
  const [value, setValue] = useState(frameCgram[0x01] ?? 0);
  return (
    <Anchor>
      <CgramPoke index={0x01} current={value} onChange={setValue} onClose={() => undefined} />
    </Anchor>
  );
};

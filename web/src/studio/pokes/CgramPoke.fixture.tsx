import { useState } from "react";
import { CgramPoke } from "./CgramPoke";
import { frameCgram } from "../../fixtures";
import "./pokes.css";

// CgramPoke is a prop-driven popover: given a CGRAM index + current BGR555 value
// it renders a native color picker and live readout. The `onChange` seam is
// injected here so no poke store (and no wasm) is on the render path — the story
// just tracks the picked value locally.
// The popover docks at top:100% of a `position: relative` trigger. Keep the
// trigger small (a stand-in swatch cell) and give the outer box enough height
// that the fixture root's bounding box includes the popover for screenshots.
function Anchor({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ minHeight: 240, padding: 24 }}>
      <div style={{ position: "relative", display: "inline-block" }}>
        <span
          aria-hidden
          style={{ display: "inline-block", width: 24, height: 24, background: "#c86432", borderRadius: 4 }}
        />
        {children}
      </div>
    </div>
  );
}

const Default = () => {
  const [value, setValue] = useState(frameCgram[0x81] ?? 0);
  return (
    <Anchor>
      <CgramPoke index={0x81} current={value} onChange={setValue} onClose={() => undefined} />
    </Anchor>
  );
};

const BackdropColor = () => {
  const [value, setValue] = useState(frameCgram[0x01] ?? 0);
  return (
    <Anchor>
      <CgramPoke index={0x01} current={value} onChange={setValue} onClose={() => undefined} />
    </Anchor>
  );
};

export default {
  Default,
  BackdropColor,
};

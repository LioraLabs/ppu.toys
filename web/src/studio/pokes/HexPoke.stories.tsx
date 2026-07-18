import { useState } from "react";
import type { Story, StoryDefault } from "@ladle/react";
import { HexPoke } from "./HexPoke";
import "./pokes.css";

// HexPoke is a prop-driven click-to-edit hex cell. Editable registers (those
// with a REG_LVALUES lvalue, e.g. TM $212C) swap to an inline input on click and
// commit through the injectable `onChange` seam; status registers with no lvalue
// (e.g. STAT77 $213E) render the plain value with no affordance. Both render with
// no poke store / wasm on the path.
export default {
  title: "Studio/Pokes/HexPoke",
} satisfies StoryDefault;

const TM = 0x212c; // editable (REG_LVALUES)
const STAT77 = 0x213e; // status register, not editable

export const Editable: Story = () => {
  const [value, setValue] = useState(0x17);
  return (
    <span className="pk-hex-row" style={{ padding: 16, display: "inline-block" }}>
      <HexPoke addr={TM} value={value} onChange={setValue} />
    </span>
  );
};

export const ReadOnly: Story = () => (
  <span style={{ padding: 16, display: "inline-block" }}>
    <HexPoke addr={STAT77} value={0x02} />
  </span>
);

export const WithChildLabel: Story = () => {
  const [value, setValue] = useState(0x21);
  return (
    <span style={{ padding: 16, display: "inline-block" }}>
      <HexPoke addr={0x2131} value={value} onChange={setValue}>
        ${value.toString(16).padStart(2, "0")} · add
      </HexPoke>
    </span>
  );
};

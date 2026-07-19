import { useState } from "react";
import { frameCgram } from "../../fixtures";
import { CgramPoke } from "./CgramPoke";
import { HexPoke } from "./HexPoke";
import "./pokes.css";

function PokeComposition() {
  const [color, setColor] = useState(frameCgram[0x81] ?? 0);
  const [register, setRegister] = useState(0x17);
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 220, minHeight: 280, padding: 32 }}>
      <div style={{ position: "relative", width: 24, height: 24 }}>
        <span
          aria-hidden
          style={{ display: "block", width: 24, height: 24, background: "#c86432", borderRadius: 4 }}
        />
        <CgramPoke index={0x81} current={color} onChange={setColor} onClose={() => undefined} />
      </div>
      <span className="pk-hex-row">
        <HexPoke addr={0x212c} value={register} onChange={setRegister} />
      </span>
    </div>
  );
}

export default <PokeComposition />;

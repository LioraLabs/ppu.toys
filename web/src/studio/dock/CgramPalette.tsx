import { useTransport } from "../transport/transport";
import { bgr555ToHex } from "./decode";

/** CGRAM dock section: 16x16 grid of the 256 palette entries, decoded live from
 *  the SHARED transport frame (same clock as Output + inspector). */
export function CgramPalette() {
  const { frame } = useTransport();
  return (
    <div className="cgram-section">
      <div className="section-header">CGRAM</div>
      <div className="palette-grid">
        {Array.from(frame.cgram, (v, idx) => (
          <div className="swatch" key={idx} style={{ background: bgr555ToHex(v) }} />
        ))}
      </div>
      <div className="palette-footer">
        <span>
          pal <span className="pal-num">0</span> · 16×16
        </span>
        <span className="bits">15-bit</span>
      </div>
    </div>
  );
}

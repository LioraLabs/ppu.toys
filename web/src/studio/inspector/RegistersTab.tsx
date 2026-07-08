import type { FrameResult } from "../../ppu/core";
import { formatAddr, formatValue, cgram15ToCss, bgMode } from "./format";

export function RegistersTab({ frame }: { frame: FrameResult | null }) {
  if (!frame) return <div className="insp-empty">waiting for frame…</div>;
  return (
    <div className="reg-list">
      <div className="reg-mode" title="active BG mode (BGMODE low 3 bits)">
        MODE {bgMode(frame.registers)}
      </div>
      {frame.registers.map((r) => (
        // key by addr only: the register set is stable frame-to-frame, so the
        // row's DOM node persists and updates in place on a value change. No
        // change-highlight by design — a steady, readable list (the previous
        // flash/remount strobed on every-frame registers like the scroll regs).
        <div className="reg-row" key={r.addr}>
          <span className="reg-addr">{formatAddr(r.addr)}</span>
          <span className="reg-name">{r.name}</span>
          <span className="reg-value">{formatValue(r.value)}</span>
        </div>
      ))}
      <div className="cgram-section">
        <div className="insp-subhead">CGRAM</div>
        <div className="palette-grid">
          {Array.from(frame.cgram, (c, i) => (
            <div
              className="swatch"
              key={i}
              style={{ background: cgram15ToCss(c) }}
              title={`idx 0x${i.toString(16).padStart(2, "0")}`}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

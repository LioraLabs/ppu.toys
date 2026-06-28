import type { FrameResult } from "../../ppu/core";
import { formatAddr, formatValue, cgram15ToCss } from "./format";

export function RegistersTab({ frame }: { frame: FrameResult | null }) {
  if (!frame) return <div className="insp-empty">waiting for frame…</div>;
  return (
    <div className="reg-list">
      {frame.registers.map((r) => (
        <div
          className={"reg-row" + (r.changed ? " reg-row--flash" : "")}
          // key includes the value so a changed register remounts the row,
          // restarting the flash keyframe even when it changes every frame
          // (React won't touch the DOM node if only the className is stable).
          key={`${r.addr}:${r.value}`}
        >
          <span className="reg-addr">{formatAddr(r.addr)}</span>
          <span className="reg-name">{r.name}</span>
          <span className={"reg-value" + (r.changed ? " reg-value--changed" : "")}>
            {formatValue(r.value)}
          </span>
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

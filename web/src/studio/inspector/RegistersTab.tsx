import type { FrameResult } from "../../ppu/core";
import {
  formatAddr,
  formatValue,
  cgram15ToCss,
  bgMode,
  screenLayers,
  colorMath,
  windowRanges,
  displayFlags,
} from "./format";

export function RegistersTab({ frame }: { frame: FrameResult | null }) {
  if (!frame) return <div className="insp-empty">waiting for frame…</div>;
  return (
    <div className="reg-list">
      <div className="reg-mode" title="active BG mode (BGMODE low 3 bits)">
        MODE {bgMode(frame.registers)}
      </div>
      {(() => {
        const cm = colorMath(frame.registers);
        const flags = displayFlags(frame.registers);
        const win = windowRanges(frame.registers);
        const main = screenLayers(frame.registers, "TM");
        const sub = screenLayers(frame.registers, "TS");
        const band = ([l, r]: [number, number]) =>
          l <= r ? { left: `${(l / 256) * 100}%`, width: `${((r - l + 1) / 256) * 100}%` } : null;
        const w1 = band(win.w1);
        const w2 = band(win.w2);
        return (
          <div className="reg-m6">
            <div className="reg-m6-row" title="TM $212C — main-screen layers">
              <span className="reg-m6-key">MAIN</span>
              <span className="reg-m6-val">{main.length ? main.join(" ") : "—"}</span>
            </div>
            <div className="reg-m6-row" title="TS $212D — sub-screen layers">
              <span className="reg-m6-key">SUB</span>
              <span className="reg-m6-val">{sub.length ? sub.join(" ") : "—"}</span>
            </div>
            <div className="reg-m6-row" title="CGADSUB/CGWSEL — colour math">
              <span className="reg-m6-key">MATH</span>
              <span className="reg-m6-val">
                {cm.layers.length
                  ? `${cm.op === "sub" ? "−" : "+"}${cm.half ? "½" : ""} ${cm.source} · ${cm.layers.join(",")}`
                  : "off"}
              </span>
            </div>
            <div className="reg-m6-row" title="CGWSEL.0 direct colour · INIDISP.7 force blank">
              <span className="reg-m6-key">FLAGS</span>
              <span className="reg-m6-val">
                {[flags.directColor && "DIRECT", flags.forceBlank && "BLANK"]
                  .filter(Boolean)
                  .join(" ") || "—"}
              </span>
            </div>
            <div className="reg-m6-row" title="WH0-3 — window 1 / 2 spans">
              <span className="reg-m6-key">WIN</span>
              <span className="reg-m6-val">
                {`1:${win.w1[0]}–${win.w1[1]}  2:${win.w2[0]}–${win.w2[1]}`}
              </span>
            </div>
            <div className="reg-winbar" title="active window bands across the 256px line">
              {w1 && <i className="reg-winband reg-winband--1" style={w1} />}
              {w2 && <i className="reg-winband reg-winband--2" style={w2} />}
            </div>
          </div>
        );
      })()}
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

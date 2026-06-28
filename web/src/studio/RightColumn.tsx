import { WIDTH, HEIGHT } from "../ppu/core";

const REGS: { addr: string; name: string; value: string; changed?: boolean }[] = [
  { addr: "$2100", name: "INIDISP", value: "0F" },
  { addr: "$2105", name: "BGMODE", value: "01" },
  { addr: "$2107", name: "BG1SC", value: "10" },
  { addr: "$2108", name: "BG2SC", value: "18" },
  { addr: "$210B", name: "BG12NBA", value: "22" },
  { addr: "$210D", name: "BG1HOFS", value: "00C8", changed: true },
  { addr: "$210F", name: "BG2HOFS", value: "0258", changed: true },
  { addr: "$2115", name: "VMAIN", value: "80" },
  { addr: "$2121", name: "CGADD", value: "40", changed: true },
  { addr: "$2122", name: "CGDATA", value: "3DEF", changed: true },
  { addr: "$212C", name: "TM", value: "17" },
  { addr: "$2132", name: "COLDATA", value: "E6" },
  { addr: "$2133", name: "SETINI", value: "00" },
  { addr: "$4200", name: "NMITIMEN", value: "81" },
];

export function RightColumn() {
  return (
    <aside className="right">
      <div className="output">
        <div className="output-header">
          <span className="section-header" style={{ padding: 0 }}>OUTPUT</span>
          <div className="tb-spacer" />
          <span className="pill">MODE 1</span>
          <span className="pill">256×224</span>
        </div>
        <div className="display">
          {/* Canvas slot — WASM PPU output wired by a later ticket. */}
          <canvas className="display-canvas" width={WIDTH} height={HEIGHT} />
          <span className="display-badge">webgl · wasm-ppu</span>
        </div>
        <div className="transport">
          <button className="play-btn" aria-label="Play">▶</button>
          <div className="scrubber">
            <div className="scrubber-fill" />
            <div className="scrubber-handle" />
          </div>
          <span className="time">t=6.4s</span>
          <span className="fullscreen">⛶</span>
        </div>
      </div>
      <div className="inspector">
        <div className="insp-tabs">
          <div className="insp-tab insp-tab--active">REGISTERS</div>
          <div className="insp-tab">SPRITES</div>
          <div className="insp-tab">VRAM</div>
        </div>
        {/* Only REGISTERS panel for now; SPRITES/VRAM filled by a later ticket. */}
        <div className="reg-list">
          {REGS.map((r) => (
            <div className="reg-row" key={r.addr}>
              <span className="reg-addr">{r.addr}</span>
              <span className="reg-name">{r.name}</span>
              <span className={"reg-value" + (r.changed ? " reg-value--changed" : "")}>{r.value}</span>
            </div>
          ))}
        </div>
      </div>
    </aside>
  );
}

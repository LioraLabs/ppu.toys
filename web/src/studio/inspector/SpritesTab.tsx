import type { FrameResult } from "../../ppu/core";
import { cgram15ToCss } from "./format";

// Authentic OBSEL size-pair table (mirrors ppu-core sprite.rs OBJ_SIZE_PAIRS):
// size_sel -> [small [w,h], large [w,h]].
const OBJ_SIZE_PAIRS: [[number, number], [number, number]][] = [
  [[8, 8], [16, 16]],
  [[8, 8], [32, 32]],
  [[8, 8], [64, 64]],
  [[16, 16], [32, 32]],
  [[16, 16], [64, 64]],
  [[32, 32], [64, 64]],
  [[16, 32], [32, 64]],
  [[16, 32], [32, 32]],
];

/** OBSEL size_sel (bits 5-7) off the $2101 register the frame carries; 0 if absent. */
function objSizeSel(frame: FrameResult): number {
  const obsel = frame.registers.find((r) => r.addr === 0x2101);
  return obsel ? (obsel.value >> 5) & 0x07 : 0;
}

/** SPRITES tab: live OAM from the shared frame. Lists the active sprites with
 *  their per-sprite fields; the colour chip uses the sprite's OBJ palette. */
export function SpritesTab({ frame }: { frame: FrameResult | null }) {
  if (!frame) return <div className="insp-empty">waiting for frame…</div>;
  const sizeSel = objSizeSel(frame);
  const active = frame.oam.filter((s) => s.on).length;
  return (
    <div className="insp-scroll">
      <div className="insp-subhead">OAM · {active} active / {frame.oam.length}</div>
      <div className="oam-list">
        {frame.oam.map((s, i) =>
          s.on ? (
            <div className="oam-row" key={i} title={`OBJ ${i}`}>
              <span
                className="oam-chip"
                style={{ background: cgram15ToCss(frame.cgram[0x80 + s.pal * 16 + 1] ?? 0) }}
              />
              <span className="oam-idx">{i.toString().padStart(3, "0")}</span>
              <span className="oam-field">x{s.x}</span>
              <span className="oam-field">y{s.y}</span>
              <span className="oam-field">t{s.tile}</span>
              <span className="oam-field">p{s.pal}</span>
              <span className="oam-field">pr{s.prio}</span>
              {(() => {
                const [w, h] = OBJ_SIZE_PAIRS[sizeSel][s.large ? 1 : 0];
                return <span className="oam-field">{w}×{h}</span>;
              })()}
              <span className="oam-field">
                {s.flipX ? "↔" : ""}{s.flipY ? "↕" : ""}
              </span>
            </div>
          ) : null,
        )}
      </div>
      {active === 0 && <div className="insp-empty">no active sprites</div>}
    </div>
  );
}

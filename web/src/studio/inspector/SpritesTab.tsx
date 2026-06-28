import type { FrameResult } from "../../ppu/core";
import { cgram15ToCss } from "./format";

/** SPRITES tab: live OAM from the shared frame. Lists the active sprites with
 *  their per-sprite fields; the colour chip uses the sprite's OBJ palette. */
export function SpritesTab({ frame }: { frame: FrameResult | null }) {
  if (!frame) return <div className="insp-empty">waiting for frame…</div>;
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
              <span className="oam-field">{s.size ? "16" : "8"}px</span>
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

import type { FrameResult } from "../../ppu/core";
import { cgram15ToCss } from "./format";

const SLOTS = 128; // SNES OAM holds 128 sprite entries

export function SpritesTab({ frame }: { frame: FrameResult | null }) {
  if (!frame) return <div className="insp-empty">waiting for frame…</div>;
  // OBJ palettes live in the upper half of CGRAM (0x80..0xff): 8 palettes x 16.
  const objBase = 0x80;
  return (
    <div className="insp-scroll">
      <div className="insp-note">
        OAM not surfaced by the PpuCore seam yet — slots show the live OBJ CGRAM
        palette. Per-sprite fields (x / y / tile / pal / prio / on) wire up at U8.
      </div>
      <div className="oam-grid">
        {Array.from({ length: SLOTS }, (_, i) => {
          const color = cgram15ToCss(frame.cgram[objBase + (i % 128)] ?? 0);
          return (
            <div className="oam-cell" key={i} title={`OBJ ${i}`}>
              <span className="oam-chip" style={{ background: color }} />
              <span className="oam-idx">{i.toString().padStart(3, "0")}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

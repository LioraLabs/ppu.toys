import { useMemo, useState } from "react";
import type { SourceKind, SourceMeta } from "../../ppu/core";
import { BlitCanvas } from "../inspector/BlitCanvas";
import { buildPreviewModel } from "./preview";
import { rgbaFrom555 } from "./payload";
import "./sources.css";

/** Stable per-sub-palette tint hues for the assignment overlay. */
const palTint = (p: number) => `hsla(${(p % 8) * 45}, 85%, 60%, 0.32)`;
const css555 = (c: number) => {
  const [r, g, b] = rgbaFrom555(c);
  return `rgb(${r}, ${g}, ${b})`;
};

export function SourcePreview({ kind, meta, payload, cellSize, sourceImage }: {
  kind: SourceKind;
  meta: SourceMeta;
  payload: Uint8Array;
  cellSize?: number;
  sourceImage?: ImageData; // fallback base when payload is undecodable (mock)
}) {
  const model = useMemo(() => buildPreviewModel(kind, meta, payload, cellSize), [kind, meta, payload, cellSize]);
  const [view, setView] = useState<"snes" | "source">("snes");
  const [tint, setTint] = useState(false);
  const src = sourceImage ? { pixels: new Uint8ClampedArray(sourceImage.data), width: sourceImage.width, height: sourceImage.height } : null;
  const img = view === "source" && src ? src : (model.image ?? src);
  const w = model.width || 1, h = model.height || 1;
  const canTint = model.palettes.length > 1 && model.cells.some((c) => c.pal !== undefined);

  return (
    <div className="srcpv">
      <div className="srcpv-tools" role="group" aria-label="Preview view">
        <button type="button" className="srcpv-tool" aria-pressed={view === "snes"} onClick={() => setView("snes")}>snes</button>
        <button type="button" className="srcpv-tool" aria-pressed={view === "source"} disabled={!src} onClick={() => setView("source")}>source</button>
        {canTint && (
          <button type="button" className="srcpv-tool srcpv-tool-right" aria-pressed={tint} onClick={() => setTint((t) => !t)}>palettes</button>
        )}
      </div>
      <div className="srcpv-stage" style={{ aspectRatio: `${w} / ${h}` }}>
        {img && <BlitCanvas pixels={img.pixels} width={img.width} height={img.height} className="srcpv-canvas" />}
        <div className="srcpv-grid" style={{ gridTemplateColumns: `repeat(${model.cols}, 1fr)`, gridTemplateRows: `repeat(${model.rows}, 1fr)` }} aria-hidden={false}>
          {model.cells.map((c, i) => (
            <div className="srcpv-cell" key={i} style={tint && c.pal !== undefined ? { background: palTint(c.pal) } : undefined}>
              <span className="srcpv-cell-top">{c.top}</span>
              {c.bot && <span className="srcpv-cell-bot">{c.bot}</span>}
            </div>
          ))}
        </div>
      </div>
      {model.palettes.length > 0 && (
        <div className="srcpv-swatches" aria-label="Resulting palettes">
          {model.palettes.map((p, pi) => (
            <div className="srcpv-swatchrow" key={pi}>
              <span className="srcpv-swatchrow-label" style={tint ? { background: palTint(pi) } : undefined}>p{pi}</span>
              {p.map((c, ci) => <span className="srcpv-swatch" key={ci} style={{ background: css555(c) }} title={`$${c.toString(16).padStart(4, "0")}`} />)}
            </div>
          ))}
        </div>
      )}
      <div className="srcpv-budget">
        {model.budget.map((b) => <span key={b} className="srcpv-chip">{b}</span>)}
      </div>
      {model.warns.length > 0 && (
        <ul className="srcpv-warns">
          {model.warns.map((w) => <li key={w}>{w}</li>)}
        </ul>
      )}
      {kind === "bg" && <div className="srcpv-note">Depth/mode mismatches are reported at bind, not here.</div>}
    </div>
  );
}

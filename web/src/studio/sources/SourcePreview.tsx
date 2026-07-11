import { useMemo } from "react";
import type { SourceKind, SourceMeta } from "../../ppu/core";
import { BlitCanvas } from "../inspector/BlitCanvas";
import { buildPreviewModel } from "./preview";

export function SourcePreview({ kind, meta, payload, cellSize, sourceImage }: {
  kind: SourceKind;
  meta: SourceMeta;
  payload: Uint8Array;
  cellSize?: number;
  sourceImage?: ImageData; // fallback base when payload is undecodable (mock)
}) {
  const model = useMemo(() => buildPreviewModel(kind, meta, payload, cellSize), [kind, meta, payload, cellSize]);
  const img = model.image ?? (sourceImage ? { pixels: new Uint8ClampedArray(sourceImage.data), width: sourceImage.width, height: sourceImage.height } : null);
  const w = model.width || 1, h = model.height || 1;

  return (
    <div className="srcpv">
      <div className="srcpv-stage" style={{ aspectRatio: `${w} / ${h}` }}>
        {img && <BlitCanvas pixels={img.pixels} width={img.width} height={img.height} className="srcpv-canvas" />}
        <div className="srcpv-grid" style={{ gridTemplateColumns: `repeat(${model.cols}, 1fr)`, gridTemplateRows: `repeat(${model.rows}, 1fr)` }} aria-hidden={false}>
          {model.cells.map((c, i) => (
            <div className="srcpv-cell" key={i}>
              <span className="srcpv-cell-top">{c.top}</span>
              {c.bot && <span className="srcpv-cell-bot">{c.bot}</span>}
            </div>
          ))}
        </div>
      </div>
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

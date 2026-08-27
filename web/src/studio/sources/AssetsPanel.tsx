import { useState } from "react";
import type { SketchSource } from "../sketches/sketchStore";
import { openSketchStore, useOpenSketch, openContextLabel } from "../sketches/openSketch";
import { demoById } from "../demos/demos";
import { transport } from "../transport/transport";
import { AddSourceDialog } from "./AddSourceDialog";
import { SourcePreview } from "./SourcePreview";
import "../sketches/sketches.css"; // library-head/title/btn flyout chrome
import "./sources.css";

/** A Record, not a ternary chain: adding a source kind must fail to compile
 *  here rather than silently badge the new kind as the fallthrough one. */
export const KIND_LABEL: Record<SketchSource["kind"], string> = {
  bg: "BG",
  m7: "M7",
  obj: "OBJ",
  sheet: "SHEET",
};

/** Assets flyout, mounted off the rail's top item: the graphics sources
 *  attached to the OPEN toy — what a processed PNG becomes. Add opens the
 *  existing convert dialog; every row previews and removes. Wired by design
 *  (open-sketch store + transport are its whole purpose); the fixture drives
 *  it through the store like production does. */
/** `onClose` is optional: as a dock panel (the default home since the rail
 *  retired) the dockview tab owns closing; flyout callers pass it. */
export function AssetsPanel({ onClose }: { onClose?: () => void } = {}) {
  const state = useOpenSketch();
  const [adding, setAdding] = useState(false);
  const [openRow, setOpenRow] = useState<string | null>(null);

  const ctx = state.context;
  const sources: SketchSource[] = ctx.kind === "sketch" ? ctx.sketch.sources : [];
  // A demo/starter context's assets are procedural (regenerated on restore) —
  // shown as fixed rows; editing them means forking via a real add/remove.
  const templateAssets = ctx.kind === "demo" ? (demoById(ctx.demoId)?.assets ?? []) : [];

  const remove = (name: string) => {
    if (!window.confirm(`Remove "${name}" from this toy?`)) return;
    openSketchStore.removeSource(name);
    transport.removeSource(name);
    if (openRow === name) setOpenRow(null);
  };

  return (
    <aside
      className={"library assets-panel" + (onClose ? "" : " library--docked")}
      aria-label="Toy assets"
    >
      <header className="library-head">
        <span className="library-title">ASSETS · {openContextLabel(state)}</span>
        <button type="button" className="library-btn" onClick={() => setAdding(true)}>
          + Add
        </button>
        {onClose && (
          <button type="button" className="library-btn" aria-label="Close assets" onClick={onClose}>
            ×
          </button>
        )}
      </header>
      <ul className="library-list">
        {sources.length === 0 && templateAssets.length === 0 && (
          <li className="library-empty">
            No assets yet. Add a PNG and it becomes real SNES graphics — tiles, palettes and a
            tilemap — then reference it from Lua:{" "}
            <code>dma(&quot;name&quot;, &#123; char = 0x1000 &#125;)</code>.
          </li>
        )}
        {templateAssets.map((a) => (
          <li key={a.id} className="library-row">
            <div className="library-open asset-row">
              <span className="library-name">{a.id}</span>
              <span className="asset-kind">{KIND_LABEL[a.kind]}</span>
              <span className="library-updated">
                {a.width}×{a.height} · template
              </span>
            </div>
          </li>
        ))}
        {sources.map((s) => (
          <li
            key={s.name}
            className={"library-row" + (openRow === s.name ? " library-row--open" : "")}
          >
            <button
              type="button"
              className="library-open asset-row"
              title="Toggle preview"
              onClick={() => setOpenRow((v) => (v === s.name ? null : s.name))}
            >
              <span className="library-name">{s.name}</span>
              <span className="asset-kind">{KIND_LABEL[s.kind]}</span>
            </button>
            <span className="library-actions">
              <button type="button" className="library-btn" onClick={() => remove(s.name)}>
                Remove
              </button>
            </span>
          </li>
        ))}
      </ul>
      {openRow &&
        (() => {
          const s = sources.find((x) => x.name === openRow);
          return s ? (
            <div className="asset-preview">
              <SourcePreview
                kind={s.kind}
                meta={s.meta}
                payload={s.payload}
                cellSize={s.kind === "obj" ? s.options.cell_size : undefined}
              />
            </div>
          ) : null;
        })()}
      {adding && <AddSourceDialog onClose={() => setAdding(false)} />}
    </aside>
  );
}

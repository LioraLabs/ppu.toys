import { useCallback, useRef, useState } from "react";
import type { SketchSource } from "../sketches/sketchStore";
import { openSketchStore, useOpenSketch, openContextLabel } from "../sketches/openSketch";
import { transport } from "../transport/transport";
import { AddSourceDialog } from "./AddSourceDialog";
import { SourcePreview } from "./SourcePreview";
import { decodeSourcePayload, quantizedRgba, priorityMaskRgba } from "./payload";
import { saveSourceFile, parseSourceFile, downloadBlob, sourceFileStem } from "../cloud/localFile";
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

/** Encode an RGBA buffer as a PNG download via a 2D canvas. */
function downloadPng(
  img: { pixels: Uint8ClampedArray; width: number; height: number },
  filename: string,
): void {
  const canvas = document.createElement("canvas");
  canvas.width = img.width;
  canvas.height = img.height;
  canvas
    .getContext("2d")!
    .putImageData(new ImageData(new Uint8ClampedArray(img.pixels), img.width, img.height), 0, 0);
  canvas.toBlob((blob) => blob && downloadBlob(blob, filename), "image/png");
}

/** Export a source as the quantized PNG the converter would take back in:
 *  `<name>.png`, plus `<name>-priority.png` for EXTBG (the mask is bit 7,
 *  which the color image drops). Throws when the payload is undecodable. */
function exportSourcePng(s: SketchSource): void {
  const d = decodeSourcePayload(s.payload);
  if (!d) throw new Error(`"${s.name}" has no decodable payload to export.`);
  const { width, height } = s.meta;
  const stem = sourceFileStem(s.name);
  downloadPng(quantizedRgba(d, width, height, s.meta.cells), `${stem}.png`);
  if (d.kind === "m7" && d.extbg)
    downloadPng(priorityMaskRgba(d, width, height), `${stem}-priority.png`);
}

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
  // stable so the memoized dialog is inert to this panel's re-renders
  const closeAdding = useCallback(() => setAdding(false), []);
  const [openRow, setOpenRow] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const importRef = useRef<HTMLInputElement>(null);

  const ctx = state.context;
  const sources: SketchSource[] = ctx.sketch.sources;
  // A demo/starter context's assets are procedural (regenerated on restore) —
  // shown as fixed rows; editing them means forking via a real add/remove.

  const remove = (name: string) => {
    if (!window.confirm(`Remove "${name}" from this toy?`)) return;
    openSketchStore.removeSource(name);
    transport.removeSource(name);
    if (openRow === name) setOpenRow(null);
  };

  /** Import a `.ppusrc.json` as a new source: validated before any store
   *  call; a taken name gets a numeric suffix rather than replacing. */
  const importFile = async (file: File) => {
    try {
      const s = parseSourceFile(await file.text());
      const taken = new Set(sources.map((x) => x.name));
      let name = s.name;
      for (let k = 2; taken.has(name); k++) name = `${s.name}_${k}`;
      const res = transport.addSource(name, s.payload);
      if (!res.ok) throw new Error(res.error ?? "addSource failed");
      openSketchStore.addSource({ ...s, name });
      setError(null);
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
    }
  };

  const exportPng = (s: SketchSource) => {
    try {
      exportSourcePng(s);
      setError(null);
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
    }
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
        <button
          type="button"
          className="library-btn"
          title="Import a .ppusrc.json exported from another toy"
          onClick={() => importRef.current?.click()}
        >
          Import
        </button>
        <input
          ref={importRef}
          type="file"
          accept=".json"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void importFile(f);
            e.target.value = "";
          }}
        />
        {onClose && (
          <button type="button" className="library-btn" aria-label="Close assets" onClick={onClose}>
            ×
          </button>
        )}
      </header>
      {error && (
        <p className="library-empty" role="alert">
          {error}
        </p>
      )}
      <ul className="library-list">
        {sources.length === 0 && (
          <li className="library-empty">
            No assets yet. Add a PNG and it becomes real SNES graphics — tiles, palettes and a
            tilemap — then reference it from Lua:{" "}
            <code>dma(&quot;name&quot;, &#123; char = 0x1000 &#125;)</code>.
          </li>
        )}
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
              <button
                type="button"
                className="library-btn"
                title="Save as .ppusrc.json: exact payload, import into another toy"
                onClick={() => saveSourceFile(s)}
              >
                JSON
              </button>
              <button
                type="button"
                className="library-btn"
                title="Save the quantized image as PNG: edit it, then re-add"
                onClick={() => exportPng(s)}
              >
                PNG
              </button>
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
      {adding && <AddSourceDialog onClose={closeAdding} />}
    </aside>
  );
}

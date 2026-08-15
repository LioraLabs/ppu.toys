import { useCallback, useMemo, useRef, useState } from "react";
import type { ChangeEvent } from "react";
import type { ConvertSourceOptions, SourceKind } from "../../ppu/core";
import { ppuCore } from "../../ppu/instance";
import { transport } from "../transport/transport";
import { openSketchStore } from "../sketches/openSketch";
import { decodeImageFile, pngFiles } from "../assets/decode";
import { SourcePreview } from "./SourcePreview";
import "./sources.css";

const KINDS: { id: SourceKind; label: string }[] = [
  { id: "bg", label: "BG (tilemap)" },
  { id: "m7", label: "Mode 7" },
  { id: "obj", label: "OBJ (sprites)" },
  { id: "sheet", label: "Tilesheet (chars only)" },
];

type Dither = "none" | "bayer" | "diffusion";
const DITHERS: { id: Dither; label: string }[] = [
  { id: "none", label: "none · flat nearest color" },
  { id: "bayer", label: "ordered · 8×8 Bayer" },
  { id: "diffusion", label: "diffusion · Floyd-Steinberg" },
];

export function AddSourceDialog({ onClose }: { onClose: () => void }) {
  const [image, setImage] = useState<ImageData | null>(null);
  const [fileName, setFileName] = useState("");
  const [kind, setKind] = useState<SourceKind>("bg");
  const [bitDepth, setBitDepth] = useState<2 | 4 | 8>(4);
  const [cellSize, setCellSize] = useState<8 | 16 | 32 | 64>(16);
  const [dither, setDither] = useState<Dither>("none");
  const [strength, setStrength] = useState(50);
  const [alphaT, setAlphaT] = useState(128);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const options: ConvertSourceOptions = useMemo(() => {
    if (kind === "m7") return {}; // fixed pipeline, no remap options
    const remap = { dither, dither_strength: strength, alpha_threshold: alphaT };
    // bg and sheet are both BG char data: bit depth, 8px tiles. Only obj sizes cells.
    return kind === "obj"
      ? { cell_size: cellSize, ...remap }
      : { bit_depth: bitDepth, tile_size: 8, ...remap };
  }, [kind, bitDepth, cellSize, dither, strength, alphaT]);

  const converted = useMemo(() => {
    if (!image) return null;
    try {
      return ppuCore.convertSource(kind, options, image);
    } catch (e) {
      return { error: String((e as Error)?.message ?? e) } as const;
    }
  }, [image, kind, options]);

  const take = useCallback(
    async (files: FileList) => {
      const png = pngFiles(files)[0];
      if (!png) {
        setError("drop a PNG");
        return;
      }
      try {
        const d = await decodeImageFile(png);
        setImage(d.imageData);
        setFileName(d.name);
        setError(null);
        if (!name) setName(d.name.replace(/\.png$/i, ""));
      } catch {
        setError("could not decode image");
      }
    },
    [name],
  );

  const add = () => {
    if (!converted || "error" in converted) return;
    const trimmed = name.trim();
    const res = transport.addSource(trimmed, converted.payload);
    if (!res.ok) {
      setError(res.error ?? "addSource failed");
      return;
    }
    // record into the open toy too — engine registration alone does not
    // survive a reload
    openSketchStore.addSource({
      name: trimmed,
      kind,
      options,
      payload: converted.payload,
      meta: converted.meta,
    });
    onClose();
  };

  const ok = !!image && !!converted && !("error" in converted) && name.trim().length > 0;

  return (
    <div className="srcdlg-scrim" onClick={onClose}>
      <div
        className="srcdlg"
        role="dialog"
        aria-label="Add source"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="srcdlg-head">
          <span className="srcdlg-title">Add Source</span>
          <button type="button" className="btn-ghost" onClick={onClose} aria-label="Close">
            ×
          </button>
        </header>

        <div className="srcdlg-body">
          <div className="srcdlg-left">
            <div
              className="srcdlg-drop"
              role="button"
              tabIndex={0}
              onClick={() => inputRef.current?.click()}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  inputRef.current?.click();
                }
              }}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault();
                if (e.dataTransfer.files.length) void take(e.dataTransfer.files);
              }}
            >
              {image ? `${fileName} · ${image.width}×${image.height}` : "drop PNG or click to pick"}
              <input
                ref={inputRef}
                type="file"
                accept="image/png"
                hidden
                onChange={(e: ChangeEvent<HTMLInputElement>) => {
                  if (e.target.files) void take(e.target.files);
                  e.target.value = "";
                }}
              />
            </div>

            <label className="srcdlg-field">
              kind
              <select value={kind} onChange={(e) => setKind(e.target.value as SourceKind)}>
                {KINDS.map((k) => (
                  <option key={k.id} value={k.id}>
                    {k.label}
                  </option>
                ))}
              </select>
            </label>

            {(kind === "bg" || kind === "sheet") && (
              <label className="srcdlg-field">
                bit depth
                <select
                  value={bitDepth}
                  onChange={(e) => setBitDepth(Number(e.target.value) as 2 | 4 | 8)}
                >
                  <option value={2}>2bpp · 4 colors</option>
                  <option value={4}>4bpp · 16 colors</option>
                  <option value={8}>8bpp · 256 colors</option>
                </select>
              </label>
            )}
            {kind === "bg" && <div className="srcpv-note">tile size fixed at 8px.</div>}
            {kind === "sheet" && (
              <div className="srcpv-note">
                Cells fixed at 8×8, read row-major: char N is the Nth cell, so bg[n].map[x][y] =
                &#123;tile = N&#125;. No dedup and no tilemap — map geometry is yours.
              </div>
            )}

            {kind === "obj" && (
              <label className="srcdlg-field">
                cell / sprite size
                <select
                  value={cellSize}
                  onChange={(e) => setCellSize(Number(e.target.value) as 8 | 16 | 32 | 64)}
                >
                  {[8, 16, 32, 64].map((s) => (
                    <option key={s} value={s}>
                      {s}px = obj[i].size
                    </option>
                  ))}
                </select>
              </label>
            )}
            {kind === "obj" && (
              <div className="srcpv-note">
                Uniform grid, no margins — one cell = OBJ size. Pre-crop irregular/marginned
                downloaded sheets; we quantize what's given (no slicer/reflow).
              </div>
            )}
            {kind === "m7" && (
              <div className="srcpv-note">
                Fixed: 8bpp chunky, ≤256 tiles, flat palette. No options.
              </div>
            )}

            {kind !== "m7" && (
              <>
                <label className="srcdlg-field">
                  dither
                  <select value={dither} onChange={(e) => setDither(e.target.value as Dither)}>
                    {DITHERS.map((d) => (
                      <option key={d.id} value={d.id}>
                        {d.label}
                      </option>
                    ))}
                  </select>
                </label>
                {dither === "diffusion" && (
                  <div className="srcpv-note">
                    Smoothest gradients, but the noise defeats tile dedup — expect a much higher
                    unique-tile count.
                  </div>
                )}
                {dither !== "none" && (
                  <label className="srcdlg-field">
                    dither strength · {strength}%
                    <input
                      type="range"
                      min={0}
                      max={100}
                      value={strength}
                      onChange={(e) => setStrength(Number(e.target.value))}
                    />
                  </label>
                )}
                <label className="srcdlg-field">
                  alpha threshold · {alphaT}
                  <input
                    type="range"
                    min={0}
                    max={255}
                    value={alphaT}
                    onChange={(e) => setAlphaT(Number(e.target.value))}
                  />
                </label>
              </>
            )}

            <label className="srcdlg-field">
              name
              <input
                type="text"
                value={name}
                placeholder="track"
                onChange={(e) => setName(e.target.value)}
              />
            </label>

            {error && <div className="srcdlg-error">{error}</div>}
            <div className="srcdlg-actions">
              <button type="button" className="btn-ghost" onClick={onClose}>
                Cancel
              </button>
              <button type="button" className="btn-solid" disabled={!ok} onClick={add}>
                Add source
              </button>
            </div>
          </div>

          <div className="srcdlg-right">
            {!image && (
              <div className="srcdlg-hint">Preview appears here after you drop an image.</div>
            )}
            {image && converted && "error" in converted && (
              <div className="srcdlg-error">{converted.error}</div>
            )}
            {image && converted && !("error" in converted) && (
              <SourcePreview
                kind={kind}
                meta={converted.meta}
                payload={converted.payload}
                cellSize={kind === "obj" ? cellSize : undefined}
                sourceImage={image}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

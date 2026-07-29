import { useEffect, useMemo, useRef, useState } from "react";
import type { Poke } from "../pokes/pokes";
import "./inspector.css";

/** Data feed for the M7 editor: the full 1024x1024 plane RGBA and the
 *  per-scanline sampling segments ([u0,v0,u1,v1] per row, NaN = not mode 7). */
export interface Mode7ViewData {
  map: Uint8ClampedArray | null;
  segments: Float32Array;
}

const MAP = 1024;
const VIEW = 512; // canvas device size (map at 0.5x)
export const M7_LVALUES = ["m7.a", "m7.b", "m7.c", "m7.d", "m7.cx", "m7.cy"] as const;

interface Matrix {
  a: number;
  b: number;
  c: number;
  d: number;
  cx: number;
  cy: number;
}
const IDENTITY: Matrix = { a: 1, b: 0, c: 0, d: 1, cx: 0, cy: 0 };

const fmt = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/0+$/, ""));

/** Scanline color: far rows (top) burn orange, near rows (bottom) go green —
 *  the classic FF-overworld frustum-fan look. */
function fanColor(y: number, rows: number): string {
  const hue = 20 + (y / Math.max(1, rows - 1)) * 100;
  return `hsla(${hue}, 90%, 55%, 0.5)`;
}

/** MODE 7 — the affine editor: the full tilemap with the per-scanline sampling
 *  fan drawn over it (what each screen row actually reads), matrix controls
 *  that write friendly `m7.*` pokes, and a perspective tool that generates the
 *  classic hdma() floor snippet. Presentational: data + poke sinks injected
 *  (wired: Mode7PanelWired; stories: fixture data). */
export function Mode7Panel({
  data,
  onPoke,
  onClearPokes,
}: {
  data: Mode7ViewData;
  onPoke: (pokes: Poke[]) => void;
  onClearPokes: () => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const baseRef = useRef<HTMLCanvasElement | null>(null);
  const [m, setM] = useState<Matrix>(IDENTITY);
  const [horizon, setHorizon] = useState(96);
  const [scale, setScale] = useState(64);
  const [angle, setAngle] = useState(0);
  const [copied, setCopied] = useState(false);

  // cache the map image on its own canvas; refresh when the feed hands a new buffer
  useEffect(() => {
    if (!data.map) {
      baseRef.current = null;
      return;
    }
    const off = document.createElement("canvas");
    off.width = MAP;
    off.height = MAP;
    off
      .getContext("2d")
      ?.putImageData(new ImageData(new Uint8ClampedArray(data.map), MAP, MAP), 0, 0);
    baseRef.current = off;
  }, [data.map]);

  // repaint: map at 0.5x, then the scanline fan
  useEffect(() => {
    const ctx = canvasRef.current?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, VIEW, VIEW);
    ctx.fillStyle = "#05060a";
    ctx.fillRect(0, 0, VIEW, VIEW);
    if (baseRef.current) ctx.drawImage(baseRef.current, 0, 0, VIEW, VIEW);
    const rows = data.segments.length / 4;
    const k = VIEW / MAP;
    ctx.lineWidth = 1;
    for (let y = 0; y < rows; y += 2) {
      const u0 = data.segments[y * 4];
      if (Number.isNaN(u0)) continue;
      const v0 = data.segments[y * 4 + 1];
      const u1 = data.segments[y * 4 + 2];
      const v1 = data.segments[y * 4 + 3];
      ctx.strokeStyle = fanColor(y, rows);
      ctx.beginPath();
      ctx.moveTo(u0 * k, v0 * k);
      ctx.lineTo(u1 * k, v1 * k);
      ctx.stroke();
    }
    ctx.strokeStyle = "rgba(255,255,255,0.25)";
    ctx.strokeRect(0.5, 0.5, VIEW - 1, VIEW - 1);
  }, [data.map, data.segments]);

  const setField = (k: keyof Matrix, v: number) => {
    const next = { ...m, [k]: v };
    setM(next);
    onPoke([{ lvalue: `m7.${k}`, expr: fmt(v), note: "M7 editor" }]);
  };

  const snippet = useMemo(() => {
    const rad = (angle * Math.PI) / 180;
    const cos = fmt(Math.cos(rad));
    const sin = fmt(Math.sin(rad));
    const rot =
      angle === 0
        ? `  m7.a, m7.d = s, s\n  m7.b, m7.c = 0, 0`
        : `  m7.a, m7.d = s * ${cos}, s * ${cos}\n  m7.b, m7.c = s * ${sin}, -(s * ${sin})`;
    return [
      `-- M7 editor: perspective floor (call inside frame(), after apply_pokes())`,
      `hdma(${horizon + 1}, 223, function(y)`,
      `  local s = ${scale} / (y - ${horizon})`,
      rot,
      `  m7.cx, m7.cy = ${m.cx}, ${m.cy}`,
      `end)`,
    ].join("\n");
  }, [horizon, scale, angle, m.cx, m.cy]);

  const num = (k: keyof Matrix, step: number, min?: number, max?: number) => (
    <label className="m7-field" key={k}>
      {k}
      <input
        type="number"
        step={step}
        min={min}
        max={max}
        value={m[k]}
        onChange={(e) => setField(k, Number(e.target.value))}
      />
    </label>
  );

  return (
    <div className="insp-scroll">
      <div className="m7-cols">
        <div className="m7-map">
          <canvas
            ref={canvasRef}
            width={VIEW}
            height={VIEW}
            className="m7-canvas"
            title="Mode 7 plane — colored lines show what each scanline samples (orange = far rows, green = near)"
          />
          <div className="srcpv-note">
            1024×1024 plane · each line = one scanline's sampling path (orange far → green near).
            Not in mode 7? Set <code>mode = 7</code> and bind an m7 source to bg[1].
          </div>
        </div>
        <div className="m7-side">
          <div className="insp-subhead">MATRIX · $211B-$2120</div>
          <div className="m7-grid">
            {num("a", 0.05)}
            {num("b", 0.05)}
            {num("c", 0.05)}
            {num("d", 0.05)}
            {num("cx", 8, 0, 1023)}
            {num("cy", 8, 0, 1023)}
          </div>
          <div className="m7-actions">
            <button
              type="button"
              className="btn-ghost"
              onClick={() => {
                setM(IDENTITY);
                onPoke(
                  M7_LVALUES.map((lv) => ({
                    lvalue: lv,
                    expr: fmt(IDENTITY[lv.slice(3) as keyof Matrix]),
                    note: "M7 editor",
                  })),
                );
              }}
            >
              identity
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={() => {
                setM(IDENTITY);
                onClearPokes();
              }}
            >
              clear pokes
            </button>
          </div>
          <div className="srcpv-note">
            Edits write friendly <code>m7.*</code> pokes — they apply where your code calls{" "}
            <code>apply_pokes()</code>, and code that sets <code>m7.*</code> afterward wins.
          </div>

          <div className="insp-subhead">PERSPECTIVE</div>
          <label className="m7-field m7-field--wide">
            horizon · {horizon}
            <input
              type="range"
              min={0}
              max={222}
              value={horizon}
              onChange={(e) => setHorizon(Number(e.target.value))}
            />
          </label>
          <label className="m7-field m7-field--wide">
            scale · {scale}
            <input
              type="range"
              min={8}
              max={256}
              value={scale}
              onChange={(e) => setScale(Number(e.target.value))}
            />
          </label>
          <label className="m7-field m7-field--wide">
            angle · {angle}°
            <input
              type="range"
              min={0}
              max={359}
              value={angle}
              onChange={(e) => setAngle(Number(e.target.value))}
            />
          </label>
          <pre className="m7-snippet">{snippet}</pre>
          <div className="m7-actions">
            <button
              type="button"
              className="btn-ghost"
              onClick={() => {
                void navigator.clipboard?.writeText(snippet).then(() => {
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1200);
                });
              }}
            >
              {copied ? "copied ✓" : "copy Lua"}
            </button>
          </div>
          <div className="srcpv-note">
            Per-scanline registers need <code>hdma()</code> in your code — pokes are frame-wide.
            Paste the snippet and drag the sliders to regenerate it.
          </div>
        </div>
      </div>
    </div>
  );
}

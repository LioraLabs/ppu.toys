import { useMemo, useRef, useState } from "react";
import { HEIGHT, WIDTH } from "../../../ppu/core";
import { formatAddr } from "../format";
import {
  AREA_FIELDS,
  COMBINE_FIELDS,
  LOGIC_LABELS,
  REG,
  REG_LVALUES,
  WINDOW_LAYERS,
  areaValue,
  combineValue,
  type PokeDialect,
  nearestEdgeAddr,
  setArea,
  setCombine,
  setWindowEdge,
  toggleWindowEnable,
  toggleWindowInvert,
  windowRow,
  winRowFields,
  type WinLogic,
} from "./model";
import { boundsAt, dimOutsideSweptMask, edgeTrace, sweptMask, type WinSweep } from "./winScanlines";
import {
  WIN_SHAPES,
  WIN_SHAPE_DEFAULTS,
  winSnippet,
  type WinShape,
  type WinShapeParams,
} from "./winSnippet";
import { PokeDot, RegRow } from "./chrome";
import { BlitCanvas } from "../BlitCanvas";
import type { Compositor } from "./useCompositor";
import { LAST_LINE, fmtNum, keyframeValueAt, type Keyframe } from "../../pokes/scanlinePokes";

/** One per-scanline poke as the keyframe track renders it. */
export interface ScanlineTrack {
  lvalue: string;
  kf: Keyframe[];
  /** Inclusive line range of this poke's generated hook. */
  range: readonly [number, number];
}

/** Edge-line colors (canvas fillStyle can't resolve CSS vars; dark accents). */
const W1_COLOR = "#ff9540";
const W2_COLOR = "#5fc9e8";
const CURSOR_COLOR = "rgba(255,255,255,0.55)";

/** How far an edge may move between scanlines and still be drawn as a joined
 *  curve. Below it the gap is the trace outrunning the pixel grid (the top and
 *  bottom of an iris move many columns per row) and bridging reads as one
 *  smooth edge; above it the jump is real — a hook switching the window off, or
 *  a band boundary — and a bridge would draw a horizontal bar that isn't there. */
const EDGE_JOIN_MAX = 16;

/** The ⚡ marker for a register an `hdma()` hook sweeps: the frame-wide controls
 *  still write it, but the hook wins on every line it covers, so the value you
 *  poke only survives outside the hook's range. */
export function HdmaChip({ w, addr }: { w: WinSweep; addr: number }) {
  if (!w.varying.has(addr)) return null;
  return (
    <span
      className="winp-hdma"
      title={`${REG_LVALUES[addr]} changes across the frame — an hdma() hook drives it, so a frame-wide poke here is overwritten on every line the hook covers`}
    >
      ⚡
    </span>
  );
}

/** The live scene with the combined W1/W2 mask dimmed away (handoff: x0.3 R/G,
 *  x0.42 B) and the four window edges traced on top. The mask and the edges are
 *  resolved PER SCANLINE, so an hdma-swept window shows its real shape — a
 *  spotlight iris draws as a circle, not as the two straight lines row 0 would
 *  imply. A static window looks exactly as it always did.
 *
 *  Click sets the inspected scanline AND grabs the nearest edge on that row;
 *  dragging keeps re-poking that WH register (frame-wide — see HdmaChip). */
export function WindowPreview({
  c,
  w,
  onScanline,
}: {
  c: Compositor;
  w: WinSweep;
  onScanline: (y: number) => void;
}) {
  const logic = combineValue(c.read) ?? 0;
  const outside = areaValue(c.read) === "outside";
  const mask = sweptMask(w.rows, c.read, logic, outside);
  const drag = useRef<number | null>(null);
  return (
    <BlitCanvas
      className="winp-canvas"
      pixels={dimOutsideSweptMask(c.frame.framebuffer, mask)}
      width={WIDTH}
      height={HEIGHT}
      title="click / drag to move the nearest window edge; the click also picks the inspected scanline"
      overlay={(ctx) => {
        const trace = (addr: number, color: string) => {
          const xs = edgeTrace(w.rows, addr, c.read);
          ctx.fillStyle = color;
          // One row per scanline, bridged to the previous row's x so a fast
          // sweep stays a continuous curve instead of a dotted arc. fillRect
          // (not stroke) keeps it crisp on the 256x224 pixel grid; a static
          // edge still draws as the same straight line as before.
          let prev = xs[0];
          for (let y = 0; y < HEIGHT; y++) {
            const x = xs[Math.min(y, xs.length - 1)];
            const join = Math.abs(x - prev) <= EDGE_JOIN_MAX;
            const lo = join ? Math.min(x, prev) : x;
            const hi = join ? Math.max(x, prev) : x;
            ctx.fillRect(lo, y, hi - lo + 1, 1);
            prev = x;
          }
        };
        trace(REG.WH0, W1_COLOR);
        trace(REG.WH1, W1_COLOR);
        trace(REG.WH2, W2_COLOR);
        trace(REG.WH3, W2_COLOR);
        ctx.fillStyle = CURSOR_COLOR;
        ctx.fillRect(0, w.y, WIDTH, 1);
      }}
      onDown={(x, y) => {
        onScanline(y);
        drag.current = nearestEdgeAddr(x, boundsAt(w.rows, y, c.read));
        c.write(setWindowEdge(drag.current, x));
      }}
      onDrag={(x) => {
        if (drag.current !== null) c.write(setWindowEdge(drag.current, x));
      }}
      onUp={() => {
        drag.current = null;
      }}
    />
  );
}

/** SCANLINE — which row every control and readout below reports. The ⚡HDMA
 *  badge names the registers this frame sweeps; "static" means row 0 IS the
 *  whole frame and the scrubber changes nothing. */
export function ScanlinePicker({
  w,
  onScanline,
}: {
  w: WinSweep;
  onScanline: (y: number) => void;
}) {
  const swept = [...w.varying].map((a) => REG_LVALUES[a]);
  return (
    <div className="winp-scan">
      <label className="winp-scan-field">
        <span className="winp-scan-lab">SCANLINE</span>
        <input
          type="range"
          min={0}
          max={HEIGHT - 1}
          value={w.y}
          onChange={(e) => onScanline(Number(e.target.value))}
        />
        <span className="winp-scan-val">{w.y}</span>
      </label>
      {swept.length > 0 ? (
        <span
          className="winp-badge winp-badge--on"
          title={`swept by hdma(): ${swept.join(", ")} — every control below reads scanline ${w.y}`}
        >
          ⚡ HDMA
        </span>
      ) : (
        <span
          className="winp-badge"
          title="no window register changes across this frame — every scanline reads the same"
        >
          static
        </span>
      )}
    </div>
  );
}

/** W1·W2 COMBINE ($212A/$212B — every slot) + MASK AREA (bulk invert bits).
 *  A segment lights only when the underlying slots/bits agree. */
export function WindowControls({ c }: { c: Compositor }) {
  const logic = combineValue(c.read);
  const area = areaValue(c.read);
  return (
    <div className="winp-row">
      <div className="winp-combine">
        <div className="cmp-ctl-label">
          W1 · W2 COMBINE · $212A
          <PokeDot c={c} addr={REG.WBGLOG} fields={COMBINE_FIELDS} />
        </div>
        <div className="cmp-seg">
          {LOGIC_LABELS.map((label, i) => (
            <button
              key={label}
              type="button"
              className={logic === i ? "cmp-seg--on" : ""}
              title="write this combine op into every WBGLOG / WOBJLOG slot"
              onClick={() => c.writeMany(setCombine(i as WinLogic))}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <div className="winp-area">
        <div className="cmp-ctl-label">
          MASK AREA
          <PokeDot c={c} addr={REG.W12SEL} fields={AREA_FIELDS} />
        </div>
        <div className="cmp-seg">
          {(["inside", "outside"] as const).map((a) => (
            <button
              key={a}
              type="button"
              className={area === a ? "cmp-seg--on" : ""}
              title="set / clear the invert bits of every layer's window select"
              onClick={() => c.writeMany(setArea(a, c.read))}
            >
              {a}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/** WH0-3 bound cards for the inspected scanline (decimal values; W1 orange,
 *  W2 cyan). A swept edge carries the ⚡ chip and its value tracks the
 *  scrubber. */
export function BoundCards({ c, w }: { c: Compositor; w: WinSweep }) {
  const b = boundsAt(w.rows, w.y, c.read);
  const cards = [
    { name: "WH0", addr: REG.WH0, val: b.wh0, w: 1 },
    { name: "WH1", addr: REG.WH1, val: b.wh1, w: 1 },
    { name: "WH2", addr: REG.WH2, val: b.wh2, w: 2 },
    { name: "WH3", addr: REG.WH3, val: b.wh3, w: 2 },
  ];
  return (
    <div className="winp-bounds">
      {cards.map((card) => (
        <div key={card.name} className={`winp-bound winp-bound--w${card.w}`}>
          <div className="winp-bound-name">
            {card.name} · {formatAddr(card.addr)}
            <PokeDot c={c} addr={card.addr} />
            <HdmaChip w={w} addr={card.addr} />
          </div>
          <div className="winp-bound-val">{card.val}</div>
        </div>
      ))}
    </div>
  );
}

/** BG1/BG2/BG3/OBJ/Color-math rows: invert + enable chips writing the real
 *  window-select nibbles (enable also mirrors TMW / the CGWSEL prevent-math
 *  region — see model.toggleWindowEnable). */
export function LayerMaskRows({ c }: { c: Compositor }) {
  return (
    <div className="winp-layers">
      {WINDOW_LAYERS.map((l) => {
        const row = windowRow(l, c.read);
        return (
          <div key={l.id} className="winp-layer">
            <span className="cmp-ldot" style={{ background: l.color }} />
            <span className="winp-lname">{l.label}</span>
            <PokeDot c={c} addr={l.selAddr} fields={winRowFields(l)} />
            <button
              type="button"
              className={"winp-chip" + (row.inverted ? " winp-chip--inv-on" : "")}
              title="invert this layer's window (both W1 and W2 invert bits)"
              onClick={() => c.writeMany(toggleWindowInvert(l, c.read))}
            >
              {row.inverted ? "outside" : "inside"}
            </button>
            <button
              type="button"
              className={"winp-chip winp-chip--en" + (row.enabled ? " winp-chip--en-on" : "")}
              title={
                l.id === "color"
                  ? "enable the color window (WOBJSEL high nibble + CGWSEL prevent-math outside it)"
                  : "enable this layer's window (select nibble + TMW clip bit)"
              }
              onClick={() => c.writeMany(toggleWindowEnable(l, c.read))}
            >
              {row.enabled ? "on" : "off"}
            </button>
          </div>
        );
      })}
    </div>
  );
}

/** Copyable window register readout — the full family the core reports, at the
 *  inspected scanline. A swept register's note carries ⚡hdma, so the readout
 *  says outright which values the scrubber moves. */
export function WindowReadout({ c, w, flat }: { c: Compositor; w: WinSweep; flat?: boolean }) {
  const rows: [number, string, string][] = [
    [REG.W12SEL, "W12SEL", "BG1/BG2 select"],
    [REG.W34SEL, "W34SEL", "BG3/BG4 select"],
    [REG.WOBJSEL, "WOBJSEL", "OBJ/color select"],
    [REG.WH0, "WH0", "W1 left"],
    [REG.WH1, "WH1", "W1 right"],
    [REG.WH2, "WH2", "W2 left"],
    [REG.WH3, "WH3", "W2 right"],
    [REG.WBGLOG, "WBGLOG", "BG combine"],
    [REG.WOBJLOG, "WOBJLOG", "OBJ/color combine"],
    [REG.TMW, "TMW", "main-screen clip"],
  ];
  return (
    <div className={"cmp-regs" + (flat ? " cmp-regs--flat" : "")}>
      {rows.map(([addr, name, note]) => (
        <RegRow
          key={addr}
          c={c}
          addr={addr}
          name={name}
          note={w.varying.has(addr) ? `${note} · ⚡hdma` : note}
        />
      ))}
    </div>
  );
}

/** KEYFRAMES — the scanline dialect's edit surface. Every per-scanline poke in
 *  `pokes.lua` gets a row: its keyframes as chips (click to jump the scrubber
 *  there, × to delete), and the value the curve currently holds at the selected
 *  line. Renders nothing when no scanline pokes exist, so the panel is
 *  unchanged until you actually make one.
 *
 *  In the scanline dialect a control edit — dragging a window edge, say —
 *  writes a keyframe on the selected line instead of a frame-wide value; the
 *  generated hook interpolates between them. */
export function KeyframeTrack({
  tracks,
  dialect,
  y,
  onScanline,
  onRemove,
  onRange,
}: {
  tracks: readonly ScanlineTrack[];
  dialect: PokeDialect;
  y: number;
  onScanline: (y: number) => void;
  onRemove: (lvalue: string, y: number) => void;
  onRange: (lvalue: string, y0: number, y1: number) => void;
}) {
  if (tracks.length === 0) {
    return dialect === "scanline" ? (
      <div className="srcpv-note winp-kf-hint">
        POKE AS <b>scanline</b>: drag a window edge (or use any control) to drop a keyframe on line{" "}
        {y}. Move the scrubber and drag again to add another — the generated <code>hdma()</code>{" "}
        hook interpolates between them.
      </div>
    ) : null;
  }
  return (
    <div className="winp-kf">
      <div className="cmp-ctl-label">KEYFRAMES · per-scanline pokes</div>
      {tracks.map(({ lvalue, kf, range }) => (
        <div key={lvalue} className="winp-kf-row">
          <span className="winp-kf-name">{lvalue}</span>
          <span
            className={
              "winp-kf-now" + (y < range[0] || y > range[1] ? " winp-kf-now--outside" : "")
            }
            title={
              y < range[0] || y > range[1]
                ? `scanline ${y} is outside this poke's range — its hook does not run here, so your script's value stands`
                : `value the curve holds at scanline ${y}`
            }
          >
            {fmtNum(keyframeValueAt(lvalue, kf, y))}
          </span>
          <span className="winp-kf-range" title="lines this poke's hdma() hook covers">
            <input
              type="number"
              min={0}
              max={LAST_LINE}
              value={range[0]}
              aria-label={`${lvalue} first line`}
              onChange={(e) => onRange(lvalue, Number(e.target.value), range[1])}
            />
            <span>–</span>
            <input
              type="number"
              min={0}
              max={LAST_LINE}
              value={range[1]}
              aria-label={`${lvalue} last line`}
              onChange={(e) => onRange(lvalue, range[0], Number(e.target.value))}
            />
          </span>
          <span className="winp-kf-chips">
            {kf.map((k) => (
              <span key={k.y} className={"winp-kf-chip" + (k.y === y ? " winp-kf-chip--at" : "")}>
                <button
                  type="button"
                  title={`jump the scrubber to scanline ${k.y}`}
                  onClick={() => onScanline(k.y)}
                >
                  {k.y}:{fmtNum(k.v)}
                </button>
                <button
                  type="button"
                  className="winp-kf-del"
                  title={
                    kf.length === 1
                      ? "remove the last keyframe — unpokes this field"
                      : `remove the keyframe on scanline ${k.y}`
                  }
                  onClick={() => onRemove(lvalue, k.y)}
                >
                  ×
                </button>
              </span>
            ))}
          </span>
        </div>
      ))}
      <div className="srcpv-note">
        Each poke generates an <code>hdma()</code> hook over the lines you give it (pokes sharing a
        range share one hook). Inside that band the hook beats the frame defaults, so a plain
        assignment in your script does <b>not</b> override it — only your own <code>hdma()</code>{" "}
        does. Outside the band the hook never runs, so narrow the range to hand those lines back to
        your script.
      </div>
    </div>
  );
}

/** SHAPE — the per-scanline AUTHORING affordance, mirroring the M7 panel's
 *  perspective tool: pick a swept shape, drag its params, copy the generated
 *  `hdma()` hook into your own code. It has to be a snippet rather than a
 *  control, because a poke is one line in `apply_pokes()` and therefore
 *  frame-wide — there is no per-scanline poke. */
export function WindowShapeTool() {
  const [shape, setShape] = useState<WinShape>("iris");
  const [p, setP] = useState<WinShapeParams>(WIN_SHAPE_DEFAULTS);
  const [copied, setCopied] = useState(false);
  const snippet = useMemo(() => winSnippet(shape, p), [shape, p]);
  const set = (k: keyof WinShapeParams, v: number) => setP((prev) => ({ ...prev, [k]: v }));

  const slider = (k: keyof WinShapeParams, label: string, min: number, max: number, step = 1) => (
    <label className="winp-shape-field" key={k}>
      {label} · {p[k]}
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={p[k]}
        onChange={(e) => set(k, Number(e.target.value))}
      />
    </label>
  );

  return (
    <div className="winp-shape">
      <div className="cmp-ctl-label">SHAPE · generates an hdma() hook</div>
      <div className="cmp-seg">
        {WIN_SHAPES.map((s) => (
          <button
            key={s.id}
            type="button"
            className={shape === s.id ? "cmp-seg--on" : ""}
            title={s.hint}
            onClick={() => setShape(s.id)}
          >
            {s.label}
          </button>
        ))}
      </div>
      <div className="winp-shape-fields">
        {slider("y0", "from line", 0, HEIGHT - 1)}
        {slider("y1", "to line", 0, HEIGHT - 1)}
        {shape === "iris" && slider("cx", "centre x", 0, 255)}
        {shape === "iris" && slider("cy", "centre y", 0, HEIGHT - 1)}
        {shape === "iris" && slider("r", "radius", 1, 160)}
        {shape === "wipe" && slider("x0", "edge x", 0, 255)}
        {shape === "wipe" && slider("slope", "slope", -2, 2, 0.05)}
        {shape === "bars" && slider("period", "band height", 1, 32)}
      </div>
      <pre className="winp-snippet">{snippet}</pre>
      <div className="winp-shape-actions">
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
        Per-scanline windows need <code>hdma()</code> in your code — pokes are frame-wide. Paste the
        snippet, then scrub the scanline above to read back what it did.
      </div>
    </div>
  );
}

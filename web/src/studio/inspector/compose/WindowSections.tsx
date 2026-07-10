import { useRef } from "react";
import { HEIGHT, WIDTH } from "../../../ppu/core";
import { formatAddr } from "../format";
import {
  LOGIC_LABELS,
  REG,
  WINDOW_LAYERS,
  areaValue,
  columnMask,
  combineValue,
  dimOutsideMask,
  nearestEdgeAddr,
  setArea,
  setCombine,
  toggleWindowEnable,
  toggleWindowInvert,
  windowBounds,
  windowRow,
  type RegWrite,
  type WinLogic,
} from "./model";
import { PinDot, RegRow } from "./chrome";
import { BlitCanvas } from "../BlitCanvas";
import type { Compositor } from "./useCompositor";

/** ppu-61: the pin write path is gone; controls call these no-ops until
 *  Task 7 rewires them onto the generated pokes.lua writer. */
/* ppu-61: replaced in Task 7 */
function writePin(_addr: number, _value: number): void {}
/* ppu-61: replaced in Task 7 */
function writePins(_writes: RegWrite[]): void {}

/** Edge-line colors (canvas fillStyle can't resolve CSS vars; dark accents). */
const W1_COLOR = "#ff9540";
const W2_COLOR = "#5fc9e8";

/** The live scene with columns outside the combined W1/W2 mask dimmed
 *  (handoff: x0.3 R/G, x0.42 B) and the four edge lines on top. Click grabs
 *  the nearest edge; dragging keeps writing that WH pin. */
export function WindowPreview({ c }: { c: Compositor }) {
  const b = windowBounds(c.read);
  const mask = columnMask(b, combineValue(c.read) ?? 0, areaValue(c.read) === "outside");
  const drag = useRef<number | null>(null);
  return (
    <BlitCanvas
      className="winp-canvas"
      pixels={dimOutsideMask(c.frame.framebuffer, mask)}
      width={WIDTH}
      height={HEIGHT}
      title="click / drag to move the nearest window edge"
      overlay={(ctx) => {
        const edge = (x: number, color: string) => {
          ctx.fillStyle = color;
          ctx.fillRect(x, 0, 1, HEIGHT);
        };
        edge(b.wh0, W1_COLOR);
        edge(b.wh1, W1_COLOR);
        edge(b.wh2, W2_COLOR);
        edge(b.wh3, W2_COLOR);
      }}
      onDown={(x) => {
        drag.current = nearestEdgeAddr(x, b);
        writePin(drag.current, x);
      }}
      onDrag={(x) => {
        if (drag.current !== null) writePin(drag.current, x);
      }}
      onUp={() => {
        drag.current = null;
      }}
    />
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
          <PinDot c={c} addr={REG.WBGLOG} />
        </div>
        <div className="cmp-seg">
          {LOGIC_LABELS.map((label, i) => (
            <button
              key={label}
              type="button"
              className={logic === i ? "cmp-seg--on" : ""}
              title="write this combine op into every WBGLOG / WOBJLOG slot"
              onClick={() => writePins(setCombine(i as WinLogic))}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <div className="winp-area">
        <div className="cmp-ctl-label">
          MASK AREA
          <PinDot c={c} addr={REG.W12SEL} />
        </div>
        <div className="cmp-seg">
          {(["inside", "outside"] as const).map((a) => (
            <button
              key={a}
              type="button"
              className={area === a ? "cmp-seg--on" : ""}
              title="set / clear the invert bits of every layer's window select"
              onClick={() => writePins(setArea(a, c.read))}
            >
              {a}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/** WH0-3 bound cards (decimal values; W1 orange, W2 cyan). */
export function BoundCards({ c }: { c: Compositor }) {
  const b = windowBounds(c.read);
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
            <PinDot c={c} addr={card.addr} />
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
            <PinDot c={c} addr={l.selAddr} />
            <button
              type="button"
              className={"winp-chip" + (row.inverted ? " winp-chip--inv-on" : "")}
              title="invert this layer's window (both W1 and W2 invert bits)"
              onClick={() => writePins(toggleWindowInvert(l, c.read))}
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
              onClick={() => writePins(toggleWindowEnable(l, c.read))}
            >
              {row.enabled ? "on" : "off"}
            </button>
          </div>
        );
      })}
    </div>
  );
}

/** Copyable window register readout — the full family the core reports. */
export function WindowReadout({ c, flat }: { c: Compositor; flat?: boolean }) {
  return (
    <div className={"cmp-regs" + (flat ? " cmp-regs--flat" : "")}>
      <RegRow c={c} addr={REG.W12SEL} name="W12SEL" note="BG1/BG2 select" />
      <RegRow c={c} addr={REG.W34SEL} name="W34SEL" note="BG3/BG4 select" />
      <RegRow c={c} addr={REG.WOBJSEL} name="WOBJSEL" note="OBJ/color select" />
      <RegRow c={c} addr={REG.WH0} name="WH0" note="W1 left" />
      <RegRow c={c} addr={REG.WH1} name="WH1" note="W1 right" />
      <RegRow c={c} addr={REG.WH2} name="WH2" note="W2 left" />
      <RegRow c={c} addr={REG.WH3} name="WH3" note="W2 right" />
      <RegRow c={c} addr={REG.WBGLOG} name="WBGLOG" note="BG combine" />
      <RegRow c={c} addr={REG.WOBJLOG} name="WOBJLOG" note="OBJ/color combine" />
      <RegRow c={c} addr={REG.TMW} name="TMW" note="main-screen clip" />
    </div>
  );
}

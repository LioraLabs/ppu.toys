import { useState } from "react";
import { HEIGHT, WIDTH } from "../../../ppu/core";
import { cgram15ToCss } from "../format";
import {
  BACKDROP_MATH_BIT,
  COMPOSE_LAYERS,
  FIXED_COLOR_SWATCHES,
  REG,
  equation,
  hexToBgr555,
  mathHalf,
  mathOp,
  tintMathRegion,
  toggleMaskBit,
  withMathHalf,
  withMathOp,
} from "./model";
import { screensFor, writePin } from "./pinStore";
import { PinDot, RegRow } from "./chrome";
import { BlitCanvas } from "../BlitCanvas";
import type { Compositor } from "./useCompositor";

/** MAIN / SUB / RESULT previews straight from the core: the two compositor
 *  intermediates (pre-math, pre-brightness) and the live framebuffer. The ▦
 *  toggle tints RESULT where the core's math-region mask says color math ran.
 *  `large` = overlay sizing. */
export function ScreenPreviews({ c, large }: { c: Compositor; large?: boolean }) {
  const [showMath, setShowMath] = useState(false);
  const screens = screensFor(c.frame);
  const op = mathOp(c.read(REG.CGADSUB));
  const result = showMath
    ? tintMathRegion(c.frame.framebuffer, screens.mathMask)
    : c.frame.framebuffer;
  return (
    <div className={"cmp-previews" + (large ? " cmp-previews--lg" : "")}>
      <div className="cmp-preview">
        <div className="cmp-preview-label">MAIN · $212C</div>
        <BlitCanvas pixels={screens.main} width={WIDTH} height={HEIGHT} title="main-screen composite (pre-math, pre-brightness)" />
      </div>
      <div className="cmp-opglyph">{op === "sub" ? "−" : "+"}</div>
      <div className="cmp-preview">
        <div className="cmp-preview-label">SUB · $212D</div>
        <BlitCanvas pixels={screens.sub} width={WIDTH} height={HEIGHT} title="sub-screen composite (pre-math, pre-brightness)" />
      </div>
      <div className="cmp-opglyph cmp-opglyph--eq">=</div>
      <div className="cmp-preview cmp-preview--result">
        <div className="cmp-preview-label">
          RESULT
          <button
            type="button"
            className={"cmp-mathtoggle" + (showMath ? " cmp-mathtoggle--on" : "")}
            title="tint pixels where the core applied color math (math-region mask)"
            onClick={() => setShowMath((v) => !v)}
          >
            ▦
          </button>
        </div>
        <BlitCanvas pixels={result} width={WIDTH} height={HEIGHT} title="final framebuffer" />
      </div>
    </div>
  );
}

/** The color-math equation chip, derived from the effective CGADSUB. */
export function EquationChip({ c }: { c: Compositor }) {
  const adsub = c.read(REG.CGADSUB);
  return <div className="cmp-eq">{equation(mathOp(adsub), mathHalf(adsub))}</div>;
}

function MatrixCell({
  layer,
  kind,
  on,
  onToggle,
}: {
  layer: string;
  kind: "main" | "sub" | "math";
  on: boolean;
  onToggle: () => void;
}) {
  const name = `${layer} ${kind} ${on ? "on" : "off"}`;
  return (
    <span className="cmp-cellwrap">
      <button
        type="button"
        className={`cmp-cell cmp-cell--${kind}` + (on ? " cmp-cell--on" : "")}
        title={name}
        aria-label={name}
        aria-pressed={on}
        onClick={onToggle}
      >
        {on ? "●" : ""}
      </button>
    </span>
  );
}

/** Per-layer MAIN/SUB/MATH toggle cells + the Backdrop row. Every click
 *  writes a pinned override of the whole register (TM / TS / CGADSUB) —
 *  the column headers wear the pin marker. */
export function AssignmentMatrix({ c }: { c: Compositor }) {
  const tm = c.read(REG.TM);
  const ts = c.read(REG.TS);
  const adsub = c.read(REG.CGADSUB);
  const toggle = (addr: number, current: number, bit: number) => {
    const w = toggleMaskBit(addr, current, bit);
    writePin(w.addr, w.value);
  };
  return (
    <div className="cmp-matrix">
      <div className="cmp-matrix-head">
        <span className="cmp-lay">LAYER</span>
        <span className="cmp-col">
          MAIN
          <PinDot c={c} addr={REG.TM} />
        </span>
        <span className="cmp-col">
          SUB
          <PinDot c={c} addr={REG.TS} />
        </span>
        <span className="cmp-col">
          MATH
          <PinDot c={c} addr={REG.CGADSUB} />
        </span>
      </div>
      {COMPOSE_LAYERS.map((l) => (
        <div className="cmp-matrix-row" key={l.id}>
          <span className="cmp-lname">
            <span className="cmp-ldot" style={{ background: l.color }} />
            {l.label}
          </span>
          <MatrixCell layer={l.label} kind="main" on={(tm & (1 << l.bit)) !== 0} onToggle={() => toggle(REG.TM, tm, l.bit)} />
          <MatrixCell layer={l.label} kind="sub" on={(ts & (1 << l.bit)) !== 0} onToggle={() => toggle(REG.TS, ts, l.bit)} />
          <MatrixCell
            layer={l.label}
            kind="math"
            on={(adsub & (1 << l.bit)) !== 0}
            onToggle={() => toggle(REG.CGADSUB, adsub, l.bit)}
          />
        </div>
      ))}
      <div className="cmp-matrix-row">
        <span className="cmp-lname">
          <span className="cmp-ldot cmp-ldot--backdrop" style={{ background: cgram15ToCss(c.frame.cgram[0]) }} />
          Backdrop
        </span>
        <span className="cmp-cellwrap cmp-fixed">—</span>
        <span className="cmp-cellwrap cmp-fixed cmp-fixed--fix">fix</span>
        <MatrixCell
          layer="Backdrop"
          kind="math"
          on={(adsub & (1 << BACKDROP_MATH_BIT)) !== 0}
          onToggle={() => toggle(REG.CGADSUB, adsub, BACKDROP_MATH_BIT)}
        />
      </div>
    </div>
  );
}

/** Operation ($2131 add/sub), ÷2 half toggle, fixed sub color ($2132). */
export function MathControls({ c, fill }: { c: Compositor; fill?: boolean }) {
  const adsub = c.read(REG.CGADSUB);
  const coldata = c.read(REG.COLDATA);
  const half = mathHalf(adsub);
  const op = mathOp(adsub);
  return (
    <div className={"cmp-controls" + (fill ? " cmp-controls--fill" : "")}>
      <div>
        <div className="cmp-ctl-label">
          OPERATION · $2131
          <PinDot c={c} addr={REG.CGADSUB} />
        </div>
        <div className="cmp-seg">
          <button
            type="button"
            className={op === "add" ? "cmp-seg--on" : ""}
            onClick={() => writePin(REG.CGADSUB, withMathOp(adsub, "add"))}
          >
            + add
          </button>
          <button
            type="button"
            className={op === "sub" ? "cmp-seg--on" : ""}
            onClick={() => writePin(REG.CGADSUB, withMathOp(adsub, "sub"))}
          >
            − sub
          </button>
        </div>
      </div>
      <button
        type="button"
        className={"cmp-half" + (half ? " cmp-half--on" : "")}
        onClick={() => writePin(REG.CGADSUB, withMathHalf(adsub, !half))}
      >
        <span className="cmp-half-track">
          <span className="cmp-half-knob" />
        </span>
        ÷ 2 (half)
      </button>
      <div>
        <div className="cmp-ctl-label">
          FIXED SUB COLOR · $2132
          <PinDot c={c} addr={REG.COLDATA} />
        </div>
        <div className="cmp-swatches">
          {FIXED_COLOR_SWATCHES.map((hex) => (
            <button
              key={hex}
              type="button"
              className={"cmp-swatch" + (coldata === hexToBgr555(hex) ? " cmp-swatch--sel" : "")}
              style={{ background: hex }}
              title={`fixed sub color ${hex}`}
              aria-label={`fixed sub color ${hex}`}
              aria-pressed={coldata === hexToBgr555(hex)}
              onClick={() => writePin(REG.COLDATA, hexToBgr555(hex))}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

/** Copyable color-math register readout — values are what the register-
 *  complete core reports (no handoff simplifications). */
export function ComposeReadout({ c, flat }: { c: Compositor; flat?: boolean }) {
  const adsub = c.read(REG.CGADSUB);
  return (
    <div className={"cmp-regs" + (flat ? " cmp-regs--flat" : "")}>
      <RegRow c={c} addr={REG.TM} name="TM" note="main screen" />
      <RegRow c={c} addr={REG.TS} name="TS" note="sub screen" />
      <RegRow
        c={c}
        addr={REG.CGADSUB}
        name="CGADSUB"
        note={(mathOp(adsub) === "sub" ? "sub" : "add") + (mathHalf(adsub) ? " · ½" : "")}
      />
      <RegRow c={c} addr={REG.CGWSEL} name="CGWSEL" note="math region" />
      <RegRow c={c} addr={REG.COLDATA} name="COLDATA" note="fixed color" swatch={cgram15ToCss(c.read(REG.COLDATA))} />
    </div>
  );
}

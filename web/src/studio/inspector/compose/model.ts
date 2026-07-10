import { HEIGHT, WIDTH, type RegisterView } from "../../../ppu/core";

/** Pure decode/encode logic for the Compose/Windows tabs + Compositor overlay.
 *  The UI reads EFFECTIVE register values (pinned override wins over the live
 *  script-driven value; power-on default when the core omits the register) and
 *  turns every click into whole-register writes for the pin API — controls
 *  never poke one-shot state. Encodings mirror the core's derive_registers /
 *  pins::apply_one round-trip. */

export const REG = {
  W12SEL: 0x2123,
  W34SEL: 0x2124,
  WOBJSEL: 0x2125,
  WH0: 0x2126,
  WH1: 0x2127,
  WH2: 0x2128,
  WH3: 0x2129,
  WBGLOG: 0x212a,
  WOBJLOG: 0x212b,
  TM: 0x212c,
  TS: 0x212d,
  TMW: 0x212e,
  CGWSEL: 0x2130,
  CGADSUB: 0x2131,
  COLDATA: 0x2132,
} as const;

/** Power-on defaults (core LineTableRow::default): TM = all five layers, rest 0. */
const POWER_ON = new Map<number, number>([[REG.TM, 0x1f]]);

export interface EffectiveReg {
  value: number;
  pinned: boolean;
}

/** What a control displays for `addr`: pinned override if present, else the
 *  live value the core reports, else the power-on default (the mock core
 *  omits most registers; the wasm core reports all of them). */
export function effectiveReg(
  registers: RegisterView[],
  pins: RegWrite[],
  addr: number,
): EffectiveReg {
  const pin = pins.find((p) => p.addr === addr);
  if (pin) return { value: pin.value, pinned: true };
  const live = registers.find((r) => r.addr === addr);
  return { value: live?.value ?? POWER_ON.get(addr) ?? 0, pinned: false };
}

/** Effective-value accessor the encode helpers read through. */
export type ReadReg = (addr: number) => number;

/** One register write for the pin API. */
export interface RegWrite {
  addr: number;
  value: number;
}

// ── Compose: screen assignment + color math ─────────────────────────────────

export interface ComposeLayer {
  id: "bg1" | "bg2" | "bg3" | "obj";
  label: string;
  color: string;
  /** TM/TS/CGADSUB bit index (BG1..BG4 = 0..3, OBJ = 4). */
  bit: number;
}

/** Matrix rows per the handoff (BG4 exists in the registers but gets no row). */
export const COMPOSE_LAYERS: ComposeLayer[] = [
  { id: "bg1", label: "BG1", color: "var(--orange)", bit: 0 },
  { id: "bg2", label: "BG2", color: "var(--purple)", bit: 1 },
  { id: "bg3", label: "BG3", color: "var(--magenta)", bit: 2 },
  { id: "obj", label: "OBJ", color: "var(--green)", bit: 4 },
];

/** CGADSUB bit 5 — backdrop math enable (the matrix Backdrop row). */
export const BACKDROP_MATH_BIT = 5;

/** Flip one designation bit, preserving the rest of the register. */
export function toggleMaskBit(addr: number, current: number, bit: number): RegWrite {
  return { addr, value: current ^ (1 << bit) };
}

export type MathOp = "add" | "sub";

export function mathOp(cgadsub: number): MathOp {
  return cgadsub & 0x80 ? "sub" : "add";
}

export function mathHalf(cgadsub: number): boolean {
  return (cgadsub & 0x40) !== 0;
}

export function withMathOp(cgadsub: number, op: MathOp): number {
  return op === "sub" ? cgadsub | 0x80 : cgadsub & ~0x80;
}

export function withMathHalf(cgadsub: number, half: boolean): number {
  return half ? cgadsub | 0x40 : cgadsub & ~0x40;
}

/** The equation chip, exactly as the handoff renders it. */
export function equation(op: MathOp, half: boolean): string {
  return `out = ( main ${op === "sub" ? "−" : "+"} sub )${half ? " ÷ 2" : ""}`;
}

/** The handoff's six COLDATA swatches. */
export const FIXED_COLOR_SWATCHES = [
  "#000000",
  "#3a2358",
  "#0d2a3a",
  "#3a1020",
  "#12321e",
  "#ffffff",
] as const;

/** '#rrggbb' -> 15-bit BGR (the COLDATA display/pin encoding). */
export function hexToBgr555(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  return ((b >> 3) << 10) | ((g >> 3) << 5) | (r >> 3);
}

// ── Windows: per-layer select nibbles ────────────────────────────────────────

export type WinLayerId = "bg1" | "bg2" | "bg3" | "obj" | "color";

export interface WindowLayer {
  id: WinLayerId;
  label: string;
  color: string;
  /** W12SEL / W34SEL / WOBJSEL. */
  selAddr: number;
  /** Low or high nibble of selAddr. */
  shift: 0 | 4;
  /** Bit in TMW (absent for the color window). */
  tmwBit?: number;
}

export const WINDOW_LAYERS: WindowLayer[] = [
  { id: "bg1", label: "BG1", color: "var(--orange)", selAddr: REG.W12SEL, shift: 0, tmwBit: 0 },
  { id: "bg2", label: "BG2", color: "var(--purple)", selAddr: REG.W12SEL, shift: 4, tmwBit: 1 },
  { id: "bg3", label: "BG3", color: "var(--magenta)", selAddr: REG.W34SEL, shift: 0, tmwBit: 2 },
  { id: "obj", label: "OBJ", color: "var(--green)", selAddr: REG.WOBJSEL, shift: 0, tmwBit: 4 },
  { id: "color", label: "Color math", color: "var(--cyan)", selAddr: REG.WOBJSEL, shift: 4 },
];

/** Select-nibble layout, LSB first: W1 invert, W1 enable, W2 invert, W2 enable. */
const ENABLE_BITS = 0b1010;
const INVERT_BITS = 0b0101;

export interface WindowRowView {
  /** Either window enabled for the layer. */
  enabled: boolean;
  /** Either window inverted. */
  inverted: boolean;
}

export function windowRow(layer: WindowLayer, read: ReadReg): WindowRowView {
  const nibble = (read(layer.selAddr) >> layer.shift) & 0xf;
  return { enabled: (nibble & ENABLE_BITS) !== 0, inverted: (nibble & INVERT_BITS) !== 0 };
}

function withNibbleBits(reg: number, shift: number, bits: number, on: boolean): number {
  const mask = bits << shift;
  return on ? reg | mask : reg & ~mask;
}

/** Toggle a row's enable. Writes both window-enable bits of the select nibble;
 *  BG/OBJ rows mirror the state into their TMW bit (so the clip is visible on
 *  the main screen — the handoff's single-register shortcut does nothing
 *  against the real core); the color row instead points CGWSEL's prevent-math
 *  region (bits 4-5) at "outside the window" (1) when enabling — color math
 *  only inside — and back to "never" (0) when disabling. */
export function toggleWindowEnable(layer: WindowLayer, read: ReadReg): RegWrite[] {
  const on = !windowRow(layer, read).enabled;
  const writes: RegWrite[] = [
    { addr: layer.selAddr, value: withNibbleBits(read(layer.selAddr), layer.shift, ENABLE_BITS, on) },
  ];
  if (layer.tmwBit !== undefined) {
    const tmw = read(REG.TMW);
    writes.push({
      addr: REG.TMW,
      value: on ? tmw | (1 << layer.tmwBit) : tmw & ~(1 << layer.tmwBit),
    });
  }
  if (layer.id === "color") {
    writes.push({ addr: REG.CGWSEL, value: (read(REG.CGWSEL) & ~0x30) | (on ? 0x10 : 0) });
  }
  return writes;
}

/** Toggle a row's invert (both windows' invert bits at once). */
export function toggleWindowInvert(layer: WindowLayer, read: ReadReg): RegWrite[] {
  const on = !windowRow(layer, read).inverted;
  return [
    { addr: layer.selAddr, value: withNibbleBits(read(layer.selAddr), layer.shift, INVERT_BITS, on) },
  ];
}

// ── Combine + area segmenteds ────────────────────────────────────────────────

/** 0=OR, 1=AND, 2=XOR, 3=XNOR (WBGLOG/WOBJLOG 2-bit fields). */
export type WinLogic = 0 | 1 | 2 | 3;
export const LOGIC_LABELS = ["OR", "AND", "XOR", "XNOR"] as const;

/** The six 2-bit slots the UI rows cover: WBGLOG BG1..BG4 + WOBJLOG OBJ,COLOR. */
function logicSlots(read: ReadReg): number[] {
  const bg = read(REG.WBGLOG);
  const obj = read(REG.WOBJLOG);
  return [bg & 3, (bg >> 2) & 3, (bg >> 4) & 3, (bg >> 6) & 3, obj & 3, (obj >> 2) & 3];
}

/** The segmented's value: the op every slot agrees on, or null (mixed). */
export function combineValue(read: ReadReg): WinLogic | null {
  const s = logicSlots(read);
  return s.every((v) => v === s[0]) ? (s[0] as WinLogic) : null;
}

/** Write one combine op into every slot of both logic registers. */
export function setCombine(op: WinLogic): RegWrite[] {
  return [
    { addr: REG.WBGLOG, value: op * 0b01010101 },
    { addr: REG.WOBJLOG, value: op * 0b0101 },
  ];
}

export type WinArea = "inside" | "outside";

/** Aggregate of the five rows' inverts: none -> inside, all -> outside, else null. */
export function areaValue(read: ReadReg): WinArea | null {
  const rows = WINDOW_LAYERS.map((l) => windowRow(l, read).inverted);
  if (rows.every((v) => !v)) return "inside";
  if (rows.every((v) => v)) return "outside";
  return null;
}

/** Bulk-set the invert bits of every row nibble (W34SEL's BG4 nibble untouched). */
export function setArea(area: WinArea, read: ReadReg): RegWrite[] {
  const on = area === "outside";
  const byAddr = new Map<number, number>();
  for (const l of WINDOW_LAYERS) {
    const cur = byAddr.get(l.selAddr) ?? read(l.selAddr);
    byAddr.set(l.selAddr, withNibbleBits(cur, l.shift, INVERT_BITS, on));
  }
  return [...byAddr].map(([addr, value]) => ({ addr, value }));
}

// ── Window preview geometry + buffers ────────────────────────────────────────

export interface WindowBounds {
  wh0: number;
  wh1: number;
  wh2: number;
  wh3: number;
}

export function windowBounds(read: ReadReg): WindowBounds {
  return { wh0: read(REG.WH0), wh1: read(REG.WH1), wh2: read(REG.WH2), wh3: read(REG.WH3) };
}

/** Combined W1/W2 raw-range membership per screen column (per-window inverts
 *  surface through the area control instead). 1 = inside the mask. */
export function columnMask(b: WindowBounds, logic: WinLogic, outside: boolean): Uint8Array {
  const mask = new Uint8Array(WIDTH);
  for (let x = 0; x < WIDTH; x++) {
    const a = b.wh0 <= x && x <= b.wh1;
    const c = b.wh2 <= x && x <= b.wh3;
    const combined = logic === 0 ? a || c : logic === 1 ? a && c : logic === 2 ? a !== c : a === c;
    mask[x] = (outside ? !combined : combined) ? 1 : 0;
  }
  return mask;
}

/** Which WH edge a preview click grabs: the register addr of the nearest of
 *  WH0..WH3 (ties -> the lower address). */
export function nearestEdgeAddr(x: number, b: WindowBounds): number {
  const edges = [b.wh0, b.wh1, b.wh2, b.wh3];
  let best = 0;
  for (let i = 1; i < 4; i++) {
    if (Math.abs(edges[i] - x) < Math.abs(edges[best] - x)) best = i;
  }
  return REG.WH0 + best;
}

/** Handoff dimming for pixels outside the window mask: x0.3 R/G, x0.42 B. */
export function dimOutsideMask(fb: Uint8ClampedArray, mask: Uint8Array): Uint8ClampedArray {
  const out = new Uint8ClampedArray(fb.length);
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = (y * WIDTH + x) * 4;
      if (mask[x]) {
        out[i] = fb[i];
        out[i + 1] = fb[i + 1];
        out[i + 2] = fb[i + 2];
      } else {
        out[i] = fb[i] * 0.3;
        out[i + 1] = fb[i + 1] * 0.3;
        out[i + 2] = fb[i + 2] * 0.42;
      }
      out[i + 3] = 255;
    }
  }
  return out;
}

/** Tint the RESULT preview green where the core applied color math (the
 *  math-region mask's bit 0). Returns a copy; the input stays untouched. */
export function tintMathRegion(fb: Uint8ClampedArray, mask: Uint8Array): Uint8ClampedArray {
  const out = new Uint8ClampedArray(fb);
  for (let p = 0; p < mask.length; p++) {
    if (mask[p] & 1) {
      const i = p * 4;
      out[i] = out[i] * 0.4;
      out[i + 1] = Math.min(255, out[i + 1] * 0.6 + 110);
      out[i + 2] = out[i + 2] * 0.4;
    }
  }
  return out;
}

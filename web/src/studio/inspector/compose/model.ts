import { HEIGHT, WIDTH, type RegisterView } from "../../../ppu/core";
import type { Poke } from "../../pokes/pokes";

/** Pure decode/encode logic for the Compose/Windows tabs + Compositor overlay.
 *  The UI reads LIVE register values (power-on default when the core omits
 *  the register) and turns every click into a whole-register write emitted as
 *  a poke — a generated DSL assignment in pokes.lua. The script wins:
 *  apply_pokes() runs at the top of frame(), so a later script write shows
 *  its own value with the poke marker hollow. Encodings mirror the core's
 *  derive_registers round-trip. */

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
  TSW: 0x212f,
  CGWSEL: 0x2130,
  CGADSUB: 0x2131,
  COLDATA: 0x2132,
} as const;

/** Power-on defaults (core LineTableRow::default): TM = all five layers, rest 0. */
const POWER_ON = new Map<number, number>([[REG.TM, 0x1f]]);

/** What a control displays for `addr`: the live value the core reports, else
 *  the power-on default (the mock core omits most registers; the wasm core
 *  reports all of them). */
export function liveReg(registers: RegisterView[], addr: number): number {
  return registers.find((r) => r.addr === addr)?.value ?? POWER_ON.get(addr) ?? 0;
}

/** Live-value accessor the encode helpers read through. */
export type ReadReg = (addr: number) => number;

/** Every compose/windows register the UI writes, by $21xx address, to its DSL
 *  flat-global mnemonic. This IS the poke inverse map — pokes emit `TM = 0x13`. */
export const REG_LVALUES: Readonly<Record<number, string>> = {
  0x2123: "W12SEL",
  0x2124: "W34SEL",
  0x2125: "WOBJSEL",
  0x2126: "WH0",
  0x2127: "WH1",
  0x2128: "WH2",
  0x2129: "WH3",
  0x212a: "WBGLOG",
  0x212b: "WOBJLOG",
  0x212c: "TM",
  0x212d: "TS",
  0x212e: "TMW",
  0x212f: "TSW",
  0x2130: "CGWSEL",
  0x2131: "CGADSUB",
  0x2132: "COLDATA",
};

/** A register write as a poke: `TM = 0x13 -- $212C`. */
export function regPoke(addr: number, value: number): Poke {
  const lvalue = REG_LVALUES[addr];
  if (!lvalue) throw new Error(`no DSL lvalue for $${addr.toString(16)}`);
  return {
    lvalue,
    expr: `0x${value.toString(16).padStart(2, "0")}`,
    note: `$${addr.toString(16).toUpperCase()}`,
  };
}

const ADDR_BY_LVALUE = new Map(Object.entries(REG_LVALUES).map(([a, l]) => [l, Number(a)]));

/** One friendly-field write a control produces. Carries BOTH poke identities:
 *  the friendly field (field = lvalue, expr = canonical RHS) and the raw
 *  register (addr + whole-register value after the control action). Dialect
 *  selection (the upcoming raw/friendly toggle) is a pure projection — see
 *  writesToPokes. */
export interface FieldWrite {
  field: string;
  expr: string;
  addr: number;
  value: number;
}

/** A field write as a friendly-dialect poke: `color.op = "sub" -- $2131`. */
export function fieldPoke(w: FieldWrite): Poke {
  return { lvalue: w.field, expr: w.expr, note: `$${w.addr.toString(16).toUpperCase()}` };
}

export type PokeDialect = "friendly" | "raw";

/** Project one control action's field writes into pokes. friendly = one poke
 *  per field (neighbor bits preserved by the core's fold); raw = one
 *  whole-register poke per touched register, last write wins. */
export function writesToPokes(writes: readonly FieldWrite[], dialect: PokeDialect): Poke[] {
  if (dialect === "friendly") return writes.map(fieldPoke);
  const last = new Map<number, number>();
  for (const w of writes) last.set(w.addr, w.value);
  return [...last].map(([addr, value]) => regPoke(addr, value));
}

// Canonical-expr helpers for the friendly dialect (module-private; wired up
// by the emitters in a later task).
const bool = (b: boolean) => (b ? "true" : "false");
const str = (s: string) => `"${s}"`;

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

/** Flip one TM/TS/CGADSUB designation bit as a friendly bool field write. */
export function toggleDesignation(field: string, addr: number, current: number, bit: number): FieldWrite {
  const on = (current & (1 << bit)) === 0;
  return { field, expr: bool(on), addr, value: current ^ (1 << bit) };
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

export type MathAddend = "sub" | "fixed";

/** CGWSEL bit1 — is the math addend the sub screen (true) or the COLDATA
 *  fixed color (false)? */
export function mathAddend(cgwsel: number): MathAddend {
  return cgwsel & 0x02 ? "sub" : "fixed";
}

export function withMathAddend(cgwsel: number, addend: MathAddend): number {
  return addend === "sub" ? cgwsel | 0x02 : cgwsel & ~0x02;
}

export function setMathOp(op: MathOp, cgadsub: number): FieldWrite {
  return { field: "color.op", expr: str(op), addr: REG.CGADSUB, value: withMathOp(cgadsub, op) };
}

export function setMathHalf(half: boolean, cgadsub: number): FieldWrite {
  return { field: "color.half", expr: bool(half), addr: REG.CGADSUB, value: withMathHalf(cgadsub, half) };
}

export function setMathAddend(addend: MathAddend, cgwsel: number): FieldWrite {
  return { field: "color.addend", expr: str(addend), addr: REG.CGWSEL, value: withMathAddend(cgwsel, addend) };
}

export function setFixedColor(bgr555: number): FieldWrite {
  return {
    field: "color.fixed",
    expr: `0x${bgr555.toString(16).padStart(4, "0")}`,
    addr: REG.COLDATA,
    value: bgr555,
  };
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

/** '#rrggbb' -> 15-bit BGR (the COLDATA display/poke encoding). */
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

/** WH edge registers -> their friendly scalar fields (whole-value semantics). */
export const WH_FIELDS: Readonly<Record<number, string>> = {
  [REG.WH0]: "win.w1.lo",
  [REG.WH1]: "win.w1.hi",
  [REG.WH2]: "win.w2.lo",
  [REG.WH3]: "win.w2.hi",
};

/** Friendly window-layer geometry — mirrors the Rust core's WIN_LAYERS
 *  (includes bg4, which has registers but no UI row). */
const WIN_FIELD_LAYERS: {
  id: string; selAddr: number; shift: 0 | 4; logAddr: number; logShift: number; tmwBit?: number;
}[] = [
  { id: "bg1", selAddr: REG.W12SEL, shift: 0, logAddr: REG.WBGLOG, logShift: 0, tmwBit: 0 },
  { id: "bg2", selAddr: REG.W12SEL, shift: 4, logAddr: REG.WBGLOG, logShift: 2, tmwBit: 1 },
  { id: "bg3", selAddr: REG.W34SEL, shift: 0, logAddr: REG.WBGLOG, logShift: 4, tmwBit: 2 },
  { id: "bg4", selAddr: REG.W34SEL, shift: 4, logAddr: REG.WBGLOG, logShift: 6, tmwBit: 3 },
  { id: "obj", selAddr: REG.WOBJSEL, shift: 0, logAddr: REG.WOBJLOG, logShift: 0, tmwBit: 4 },
  { id: "color", selAddr: REG.WOBJSEL, shift: 4, logAddr: REG.WOBJLOG, logShift: 2 },
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
export function toggleWindowEnable(layer: WindowLayer, read: ReadReg): FieldWrite[] {
  const on = !windowRow(layer, read).enabled;
  const sel = withNibbleBits(read(layer.selAddr), layer.shift, ENABLE_BITS, on);
  const writes: FieldWrite[] = [
    { field: `win.${layer.id}.w1`, expr: bool(on), addr: layer.selAddr, value: sel },
    { field: `win.${layer.id}.w2`, expr: bool(on), addr: layer.selAddr, value: sel },
  ];
  if (layer.tmwBit !== undefined) {
    const tmw = read(REG.TMW);
    writes.push({
      field: `win.${layer.id}.main`,
      expr: bool(on),
      addr: REG.TMW,
      value: on ? tmw | (1 << layer.tmwBit) : tmw & ~(1 << layer.tmwBit),
    });
  }
  if (layer.id === "color") {
    // WHERE math is prevented is color's turf (CGWSEL is co-owned): route
    // through color.region — `win` never writes CGWSEL.
    writes.push({
      field: "color.region",
      expr: str(on ? "inside" : "everywhere"),
      addr: REG.CGWSEL,
      value: (read(REG.CGWSEL) & ~0x30) | (on ? 0x10 : 0),
    });
  }
  return writes;
}

/** Toggle a row's invert (both windows' invert bits at once). */
export function toggleWindowInvert(layer: WindowLayer, read: ReadReg): FieldWrite[] {
  const on = !windowRow(layer, read).inverted;
  return [{
    // lossy on purpose: the friendly field drives BOTH invert bits, exactly
    // like this control always has; a lone invert bit stays raw-only
    field: `win.${layer.id}.invert`,
    expr: bool(on),
    addr: layer.selAddr,
    value: withNibbleBits(read(layer.selAddr), layer.shift, INVERT_BITS, on),
  }];
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
export function setCombine(op: WinLogic): FieldWrite[] {
  const bgByte = op * 0b01010101;
  const objByte = op * 0b0101;
  return WIN_FIELD_LAYERS.map((l) => ({
    field: `win.${l.id}.combine`,
    expr: str(LOGIC_LABELS[op]),
    addr: l.logAddr,
    value: l.logAddr === REG.WBGLOG ? bgByte : objByte,
  }));
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
export function setArea(area: WinArea, read: ReadReg): FieldWrite[] {
  const on = area === "outside";
  const byAddr = new Map<number, number>();
  for (const l of WINDOW_LAYERS) {
    const cur = byAddr.get(l.selAddr) ?? read(l.selAddr);
    byAddr.set(l.selAddr, withNibbleBits(cur, l.shift, INVERT_BITS, on));
  }
  return WINDOW_LAYERS.map((l) => ({
    field: `win.${l.id}.invert`,
    expr: bool(on),
    addr: l.selAddr,
    value: byAddr.get(l.selAddr) ?? 0,
  }));
}

export function setWindowEdge(addr: number, x: number): FieldWrite {
  return { field: WH_FIELDS[addr], expr: String(x), addr, value: x };
}

/** Field groups the section-header PokeDots key on. */
export const SCREEN_MAIN_FIELDS = COMPOSE_LAYERS.map((l) => `screen.main.${l.id}`);
export const SCREEN_SUB_FIELDS = COMPOSE_LAYERS.map((l) => `screen.sub.${l.id}`);
export const MATH_ENABLE_FIELDS = [...COMPOSE_LAYERS.map((l) => `color.on.${l.id}`), "color.on.backdrop"];
export const OPERATION_FIELDS = ["color.op", "color.half"];
export const ADDEND_FIELDS = ["color.addend"];
export const FIXED_FIELDS = ["color.fixed"];
export const COMBINE_FIELDS = WIN_FIELD_LAYERS.map((l) => `win.${l.id}.combine`);
export const AREA_FIELDS = WINDOW_LAYERS.map((l) => `win.${l.id}.invert`);

/** The three fields a Windows layer row's marker covers. */
export function winRowFields(l: WindowLayer): string[] {
  return [`win.${l.id}.w1`, `win.${l.id}.w2`, `win.${l.id}.invert`];
}

// ── Field decode table ───────────────────────────────────────────────────────

/** SEL-nibble enable bits, matching the core: W1 = 0x2, W2 = 0x8. */
const W1_ENABLE = 0x2;
const W2_ENABLE = 0x8;

/** CGWSEL bits 4-5 decode, matching the core's color.region. */
const REGION_NAMES = ["everywhere", "inside", "outside", "never"] as const;

type FieldValue = string | number | boolean;

/** THE dual-dialect round-trip table: friendly field lvalue -> the register
 *  it lives in + a live-decode returning the field's current value. Emission
 *  and pokeMatchesLive both key on it; ownership mirrors the Rust folds
 *  (win never owns CGWSEL; the color window has no TMW/TSW bit). */
export const FIELD_SPECS: ReadonlyMap<string, { addr: number; live: (read: ReadReg) => FieldValue }> =
  buildFieldSpecs();

function buildFieldSpecs() {
  const m = new Map<string, { addr: number; live: (read: ReadReg) => FieldValue }>();
  const layerBits: [string, number][] = [["bg1", 0], ["bg2", 1], ["bg3", 2], ["bg4", 3], ["obj", 4]];
  for (const [id, bit] of layerBits) {
    m.set(`screen.main.${id}`, { addr: REG.TM, live: (r) => (r(REG.TM) & (1 << bit)) !== 0 });
    m.set(`screen.sub.${id}`, { addr: REG.TS, live: (r) => (r(REG.TS) & (1 << bit)) !== 0 });
    m.set(`color.on.${id}`, { addr: REG.CGADSUB, live: (r) => (r(REG.CGADSUB) & (1 << bit)) !== 0 });
  }
  m.set("color.on.backdrop", { addr: REG.CGADSUB, live: (r) => (r(REG.CGADSUB) & 0x20) !== 0 });
  m.set("color.op", { addr: REG.CGADSUB, live: (r) => mathOp(r(REG.CGADSUB)) });
  m.set("color.half", { addr: REG.CGADSUB, live: (r) => mathHalf(r(REG.CGADSUB)) });
  m.set("color.addend", { addr: REG.CGWSEL, live: (r) => mathAddend(r(REG.CGWSEL)) });
  m.set("color.region", { addr: REG.CGWSEL, live: (r) => REGION_NAMES[(r(REG.CGWSEL) >> 4) & 3] });
  m.set("color.fixed", { addr: REG.COLDATA, live: (r) => r(REG.COLDATA) & 0x7fff });
  for (const [addr, field] of Object.entries(WH_FIELDS)) {
    const a = Number(addr);
    m.set(field, { addr: a, live: (r) => r(a) });
  }
  for (const l of WIN_FIELD_LAYERS) {
    const nib = (r: ReadReg) => (r(l.selAddr) >> l.shift) & 0xf;
    m.set(`win.${l.id}.w1`, { addr: l.selAddr, live: (r) => (nib(r) & W1_ENABLE) !== 0 });
    m.set(`win.${l.id}.w2`, { addr: l.selAddr, live: (r) => (nib(r) & W2_ENABLE) !== 0 });
    // lossy shared decode, mirrors the core: true if EITHER invert bit is set
    m.set(`win.${l.id}.invert`, { addr: l.selAddr, live: (r) => (nib(r) & INVERT_BITS) !== 0 });
    m.set(`win.${l.id}.combine`, { addr: l.logAddr, live: (r) => LOGIC_LABELS[(r(l.logAddr) >> l.logShift) & 3] });
    const bit = l.tmwBit;
    if (bit !== undefined) {
      m.set(`win.${l.id}.main`, { addr: REG.TMW, live: (r) => (r(REG.TMW) & (1 << bit)) !== 0 });
      m.set(`win.${l.id}.sub`, { addr: REG.TSW, live: (r) => (r(REG.TSW) & (1 << bit)) !== 0 });
    }
  }
  return m;
}

/** Parse a friendly poke's RHS: true/false, a "quoted string", or a Lua
 *  number (decimal or 0x hex). null = non-comparable. */
function parseFieldExpr(expr: string): FieldValue | null {
  if (expr === "true") return true;
  if (expr === "false") return false;
  const s = /^"([^"]*)"$/.exec(expr);
  if (s) return s[1];
  const n = Number(expr);
  return Number.isNaN(n) ? null : n;
}

/** Solid/hollow decision for the poke marker, both dialects: friendly field
 *  pokes decode the live register through FIELD_SPECS; raw register pokes
 *  compare the whole byte. null = non-comparable. */
export function pokeMatchesLive(p: Poke, registers: RegisterView[]): boolean | null {
  const read: ReadReg = (addr) => liveReg(registers, addr);
  const spec = FIELD_SPECS.get(p.lvalue);
  if (spec) {
    const want = parseFieldExpr(p.expr);
    return want === null ? null : spec.live(read) === want;
  }
  const want = Number(p.expr);
  const addr = ADDR_BY_LVALUE.get(p.lvalue);
  if (Number.isNaN(want) || addr === undefined) return null;
  return read(addr) === want;
}

/** Pokes targeting a control: its raw whole-register poke plus, with a
 *  `fields` list, exactly those friendly pokes (control labels), or, without
 *  one, ANY friendly field living in the register (register-centric rows). */
export function pokesAt(pokes: readonly Poke[], addr: number, fields?: readonly string[]): Poke[] {
  return pokes.filter(
    (p) =>
      p.lvalue === REG_LVALUES[addr] ||
      (fields ? fields.includes(p.lvalue) : FIELD_SPECS.get(p.lvalue)?.addr === addr),
  );
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

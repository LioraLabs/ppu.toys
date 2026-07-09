import type { RegisterView } from "../../ppu/core";

/** Pure formatting helpers for the inspector. No React, no DOM. */

/** SNES register address -> "$XXXX" uppercase, 4 hex digits. */
export function formatAddr(addr: number): string {
  return "$" + addr.toString(16).toUpperCase().padStart(4, "0");
}

/** Register value -> uppercase hex, at least 2 digits (matches mock/design). */
export function formatValue(value: number): string {
  return value.toString(16).toUpperCase().padStart(2, "0");
}

/** Packed 15-bit SNES colour (0bBBBBB_GGGGG_RRRRR, bit15 unused) -> css rgb(). */
export function cgram15ToCss(c: number): string {
  const r5 = c & 0x1f;
  const g5 = (c >> 5) & 0x1f;
  const b5 = (c >> 10) & 0x1f;
  const x = (v: number) => (v << 3) | (v >> 2); // 5-bit -> 8-bit
  return `rgb(${x(r5)}, ${x(g5)}, ${x(b5)})`;
}

/** Active BG mode (0/1/2/3/4/7) = BGMODE register low 3 bits. Defaults to 1. */
export function bgMode(registers: RegisterView[]): number {
  return (registers.find((r) => r.name === "BGMODE")?.value ?? 1) & 0x07;
}

/** The five screen/math layers, LSB-first (TM/TS/CGADSUB bit order). */
const LAYER_NAMES = ["BG1", "BG2", "BG3", "BG4", "OBJ"] as const;

function regValue(registers: RegisterView[], name: string, dflt: number): number {
  return registers.find((r) => r.name === name)?.value ?? dflt;
}

/** Decode a TM/TS-style 5-bit layer mask into layer labels. Absent -> power-on
 *  default (TM = all five layers on, TS = none). */
export function screenLayers(registers: RegisterView[], name: "TM" | "TS"): string[] {
  const mask = regValue(registers, name, name === "TM" ? 0x1f : 0x00);
  return LAYER_NAMES.filter((_, i) => mask & (1 << i));
}

export interface ColorMathView {
  op: "add" | "sub";
  half: boolean;
  source: "sub" | "fixed";
  layers: string[]; // CGADSUB bits 0-5 -> BG1..BG4, OBJ, BACK
}

/** Decode CGADSUB (sign/half/enable) + CGWSEL (addend source) into a summary. */
export function colorMath(registers: RegisterView[]): ColorMathView {
  const adsub = regValue(registers, "CGADSUB", 0);
  const wsel = regValue(registers, "CGWSEL", 0);
  const names = [...LAYER_NAMES, "BACK"];
  return {
    op: adsub & 0x80 ? "sub" : "add",
    half: (adsub & 0x40) !== 0,
    source: wsel & 0x02 ? "sub" : "fixed",
    layers: names.filter((_, i) => adsub & (1 << i)),
  };
}

/** SETINI $2133 bit 6 — Mode 7 EXTBG (per-pixel Mode-7 priority). */
export function extbg(registers: RegisterView[]): boolean {
  return (regValue(registers, "SETINI", 0) & 0x40) !== 0;
}

export interface WindowRangesView {
  w1: [number, number];
  w2: [number, number];
}

/** Decode WH0-3 into the two window [left,right] spans. */
export function windowRanges(registers: RegisterView[]): WindowRangesView {
  return {
    w1: [regValue(registers, "WH0", 0), regValue(registers, "WH1", 0)],
    w2: [regValue(registers, "WH2", 0), regValue(registers, "WH3", 0)],
  };
}

export interface DisplayFlagsView {
  directColor: boolean; // CGWSEL bit 0 — 8bpp direct colour
  forceBlank: boolean; // INIDISP bit 7 — force blank
}

/** Decode the two output-path flags: CGWSEL.0 direct colour + INIDISP.7 blank. */
export function displayFlags(registers: RegisterView[]): DisplayFlagsView {
  return {
    directColor: (regValue(registers, "CGWSEL", 0) & 0x01) !== 0,
    forceBlank: (regValue(registers, "INIDISP", 0) & 0x80) !== 0,
  };
}

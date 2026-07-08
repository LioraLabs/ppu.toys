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

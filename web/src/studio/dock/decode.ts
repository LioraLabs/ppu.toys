/** Decode a SNES BGR555 (15-bit) CGRAM color to a CSS "#rrggbb" string.
 *  Bits: r = v & 0x1f, g = (v >> 5) & 0x1f, b = (v >> 10) & 0x1f; bit 15 unused.
 *  Each 5-bit channel is scaled to 8-bit via round(c / 31 * 255). */
export function bgr555ToHex(v: number): string {
  const c5to8 = (c: number) => Math.round((c / 31) * 255);
  const r = c5to8(v & 0x1f);
  const g = c5to8((v >> 5) & 0x1f);
  const b = c5to8((v >> 10) & 0x1f);
  const h = (n: number) => n.toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

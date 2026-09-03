/** Controller input: keyboard + gamepad -> the PAD bitmask the core mirrors
 *  into Lua's `pad` table. Bit order matches PAD_NAMES in
 *  crates/ppu-core/src/lua.rs. */
export const PAD = {
  up: 1 << 0,
  down: 1 << 1,
  left: 1 << 2,
  right: 1 << 3,
  a: 1 << 4,
  b: 1 << 5,
  x: 1 << 6,
  y: 1 << 7,
  l: 1 << 8,
  r: 1 << 9,
  start: 1 << 10,
  select: 1 << 11,
} as const;

/** KeyboardEvent.code -> button. The usual emulator layout: Z/X are the
 *  bottom/right face buttons (B/A), A/S the left/top (Y/X). */
export const KEY_TO_PAD: Record<string, number> = {
  ArrowUp: PAD.up,
  ArrowDown: PAD.down,
  ArrowLeft: PAD.left,
  ArrowRight: PAD.right,
  KeyZ: PAD.b,
  KeyX: PAD.a,
  KeyA: PAD.y,
  KeyS: PAD.x,
  KeyQ: PAD.l,
  KeyW: PAD.r,
  Enter: PAD.start,
  ShiftLeft: PAD.select,
  ShiftRight: PAD.select,
};

/** One-line hint for a surface that takes pad input. */
export const PAD_HINT = "arrows · Z/X · A/S · Q/W · Enter";

/** W3C standard-mapping gamepad -> mask. Face buttons follow position, so a
 *  SNES-shaped pad lands on the matching SNES names. */
export function gamepadMask(gp: {
  buttons: readonly { pressed: boolean }[];
  axes: readonly number[];
}): number {
  const b = (i: number) => (gp.buttons[i]?.pressed ? 1 : 0);
  let m = 0;
  if (b(0)) m |= PAD.b;
  if (b(1)) m |= PAD.a;
  if (b(2)) m |= PAD.y;
  if (b(3)) m |= PAD.x;
  if (b(4)) m |= PAD.l;
  if (b(5)) m |= PAD.r;
  if (b(8)) m |= PAD.select;
  if (b(9)) m |= PAD.start;
  if (b(12) || (gp.axes[1] ?? 0) < -0.5) m |= PAD.up;
  if (b(13) || (gp.axes[1] ?? 0) > 0.5) m |= PAD.down;
  if (b(14) || (gp.axes[0] ?? 0) < -0.5) m |= PAD.left;
  if (b(15) || (gp.axes[0] ?? 0) > 0.5) m |= PAD.right;
  return m;
}

/** Poll every connected gamepad into one mask; 0 where the API is absent. */
export function pollGamepads(): number {
  if (typeof navigator === "undefined" || !navigator.getGamepads) return 0;
  let m = 0;
  for (const gp of navigator.getGamepads()) if (gp) m |= gamepadMask(gp);
  return m;
}

type KeyLike = { code: string; preventDefault(): void };

/** Handlers to spread on a focusable surface (a div with tabIndex). Mapped
 *  keys are swallowed so arrows don't scroll the page; unmapped keys pass
 *  through. Blur releases everything. Keep ONE instance per surface (useState
 *  initializer) so the held mask survives re-renders. */
export function padKeyHandlers(set: (mask: number) => void) {
  let held = 0;
  return {
    tabIndex: 0,
    onKeyDown(e: KeyLike) {
      const bit = KEY_TO_PAD[e.code];
      if (!bit) return;
      e.preventDefault();
      held |= bit;
      set(held);
    },
    onKeyUp(e: KeyLike) {
      const bit = KEY_TO_PAD[e.code];
      if (!bit) return;
      e.preventDefault();
      held &= ~bit;
      set(held);
    },
    onBlur() {
      held = 0;
      set(0);
    },
  };
}

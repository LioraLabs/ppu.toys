import { describe, it, expect } from "vitest";
import { PAD, gamepadMask, padKeyHandlers } from "./pad";

const key = (code: string) => ({ code, preventDefault: () => {} });

describe("padKeyHandlers", () => {
  it("accumulates held keys, releases on keyup, clears on blur, ignores unmapped", () => {
    const seen: number[] = [];
    const h = padKeyHandlers((m) => seen.push(m));
    h.onKeyDown(key("ArrowLeft"));
    h.onKeyDown(key("KeyZ"));
    h.onKeyDown(key("KeyP")); // unmapped: no emit
    h.onKeyUp(key("ArrowLeft"));
    h.onBlur();
    expect(seen).toEqual([PAD.left, PAD.left | PAD.b, PAD.b, 0]);
  });
});

describe("gamepadMask", () => {
  it("maps standard buttons and the left stick past the deadzone", () => {
    const buttons = Array.from({ length: 16 }, () => ({ pressed: false }));
    buttons[0].pressed = true; // bottom face = B
    buttons[9].pressed = true; // start
    expect(gamepadMask({ buttons, axes: [0.9, 0] })).toBe(PAD.b | PAD.start | PAD.right);
    expect(gamepadMask({ buttons: [], axes: [0, -0.2] })).toBe(0);
  });
});

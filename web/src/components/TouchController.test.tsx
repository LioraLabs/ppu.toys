// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { TouchController } from "./TouchController";
import { PAD } from "../studio/transport/pad";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("holds simultaneous inputs, slides diagonally, and releases cancelled or abandoned touches", () => {
  vi.stubGlobal(
    "PointerEvent",
    class extends MouseEvent {
      pointerId: number;
      constructor(type: string, init: PointerEventInit) {
        super(type, init);
        this.pointerId = init.pointerId ?? 0;
      }
    },
  );
  const onChange = vi.fn();
  const { getByRole, unmount } = render(<TouchController onChange={onChange} />);
  const up = getByRole("button", { name: "Up" });
  const a = getByRole("button", { name: "A" });
  up.setPointerCapture = a.setPointerCapture = vi.fn();
  fireEvent.pointerDown(up, { pointerId: 1, button: 0 });
  fireEvent.pointerDown(a, { pointerId: 2, button: 0 });
  expect(onChange).toHaveBeenLastCalledWith(PAD.up | PAD.a);
  document.elementFromPoint = vi.fn(() => getByRole("button", { name: "Up right" }));
  fireEvent.pointerMove(up, { pointerId: 1 });
  expect(onChange).toHaveBeenLastCalledWith(PAD.up | PAD.right | PAD.a);
  fireEvent.pointerCancel(a, { pointerId: 2 });
  expect(onChange).toHaveBeenLastCalledWith(PAD.up | PAD.right);
  fireEvent.lostPointerCapture(up, { pointerId: 1 });
  expect(onChange).toHaveBeenLastCalledWith(0);
  fireEvent.keyDown(a, { key: " " });
  expect(onChange).toHaveBeenLastCalledWith(PAD.a);
  fireEvent.keyUp(a, { key: " " });
  expect(onChange).toHaveBeenLastCalledWith(0);
  const center = getByRole("button", { name: "Center" });
  center.setPointerCapture = vi.fn();
  fireEvent.pointerDown(center, { pointerId: 6, button: 0 });
  expect(center.setPointerCapture).toHaveBeenCalledWith(6);
  expect(onChange).toHaveBeenLastCalledWith(0);
  expect(center.getAttribute("aria-pressed")).toBe("false");
  fireEvent.pointerDown(a, { pointerId: 7, button: 0 });
  document.elementFromPoint = vi.fn(() => getByRole("button", { name: "Left" }));
  fireEvent.pointerMove(center, { pointerId: 6 });
  expect(onChange).toHaveBeenLastCalledWith(PAD.left | PAD.a);
  document.elementFromPoint = vi.fn(() => center);
  fireEvent.pointerMove(center, { pointerId: 6 });
  expect(onChange).toHaveBeenLastCalledWith(PAD.a);
  fireEvent.pointerUp(center, { pointerId: 6 });
  expect(onChange).toHaveBeenLastCalledWith(PAD.a);
  fireEvent.pointerCancel(a, { pointerId: 7 });
  expect(onChange).toHaveBeenLastCalledWith(0);
  fireEvent.pointerDown(up, { pointerId: 3, button: 0 });
  fireEvent.blur(window);
  expect(onChange).toHaveBeenLastCalledWith(0);
  fireEvent.pointerDown(a, { pointerId: 4, button: 0 });
  fireEvent(document, new Event("visibilitychange"));
  expect(onChange).toHaveBeenLastCalledWith(0);
  fireEvent.pointerDown(a, { pointerId: 5, button: 0 });
  unmount();
  expect(onChange).toHaveBeenLastCalledWith(0);
});

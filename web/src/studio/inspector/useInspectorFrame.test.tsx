// @vitest-environment jsdom
import { describe, it, expect, afterEach } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { frameResult } from "../../fixtures";
import { bgMode } from "./format";
import { InspectorFrameProvider, useInspectorFrame } from "./useInspectorFrame";

/** Tiny consumer that just reads the hook and renders decoded values, so we
 *  can assert the injected fixture frame reaches it — no wasm, no ppuCore. */
function Consumer() {
  const frame = useInspectorFrame();
  return (
    <div>
      <span data-testid="bg-mode">{bgMode(frame.registers)}</span>
      <span data-testid="active-sprites">{frame.oam.filter((s) => s.on).length}</span>
    </div>
  );
}

describe("useInspectorFrame (injected path)", () => {
  afterEach(() => cleanup());

  it("renders the fixture frame supplied via InspectorFrameProvider", () => {
    render(
      <InspectorFrameProvider frame={frameResult}>
        <Consumer />
      </InspectorFrameProvider>,
    );

    expect(screen.getByTestId("bg-mode")).toHaveTextContent(String(bgMode(frameResult.registers)));
    expect(screen.getByTestId("active-sprites")).toHaveTextContent(
      String(frameResult.oam.filter((s) => s.on).length),
    );
  });
});

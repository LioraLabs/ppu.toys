// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import "fake-indexeddb/auto";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { openSketchStore } from "./sketches/openSketch";
import { pokeMany } from "./pokes/pokeStore";
import { POKES_FILE } from "./pokes/pokes";
import { PokeFileBar } from "./EditorPane";

describe("PokeFileBar (poke menu placement above the editor)", () => {
  beforeEach(async () => {
    await openSketchStore.newSketch();
  });
  afterEach(() => cleanup());

  it("renders the dialect toggle and poke bar when pokes.lua is the active tab", () => {
    pokeMany([{ lvalue: "TM", expr: "0x13", note: "$212C" }]);
    render(<PokeFileBar active={POKES_FILE} />);
    expect(screen.getByText("POKE AS")).toBeInTheDocument();
    expect(screen.getByText(/1 poked/)).toBeInTheDocument();
  });

  it("renders nothing when a different file is active", () => {
    pokeMany([{ lvalue: "TM", expr: "0x13", note: "$212C" }]);
    const { container } = render(<PokeFileBar active="main.lua" />);
    expect(container).toBeEmptyDOMElement();
  });
});

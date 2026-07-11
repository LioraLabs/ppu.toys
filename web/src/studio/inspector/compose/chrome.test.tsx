// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import "fake-indexeddb/auto";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { openSketchStore } from "../../sketches/openSketch";
import { pokeMany } from "../../pokes/pokeStore";
import { PokeBar } from "./chrome";

describe("PokeBar (store-sourced, no Compositor prop)", () => {
  beforeEach(async () => {
    await openSketchStore.newSketch();
  });
  afterEach(() => cleanup());

  it("renders chips from the store with no Compositor prop", () => {
    pokeMany([{ lvalue: "TM", expr: "0x13", note: "$212C" }]);
    render(<PokeBar />);
    expect(screen.getByText(/1 poked/)).toBeInTheDocument();
  });
});

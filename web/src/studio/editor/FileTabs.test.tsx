// @vitest-environment jsdom
import { describe, it, expect } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { FileTabs } from "./FileTabs";

const base = {
  files: ["pokes.lua", "main.lua"],
  active: "main.lua",
  errorFiles: new Set<string>(),
  generated: new Set(["pokes.lua"]),
  onSelect() {},
  onAdd() {},
  onRename() {
    return true;
  },
  onDelete() {},
  onReorder() {},
};

describe("FileTabs generated-tab glyph", () => {
  it("shows ⚙ on a poked generated tab, ⚡ otherwise", () => {
    const { rerender } = render(<FileTabs {...base} pokedFiles={new Set()} />);
    expect(screen.getByText("⚙")).toBeInTheDocument();

    rerender(<FileTabs {...base} pokedFiles={new Set(["pokes.lua"])} />);
    expect(screen.getByText("⚡")).toBeInTheDocument();
  });
});

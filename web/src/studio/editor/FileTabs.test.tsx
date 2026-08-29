// @vitest-environment jsdom
import { afterEach, describe, it, expect, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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

afterEach(cleanup);

describe("FileTabs generated-tab glyph", () => {
  it("shows ⚙ on a poked generated tab, ⚡ otherwise", () => {
    const { rerender } = render(<FileTabs {...base} pokedFiles={new Set()} />);
    expect(screen.getByText("⚙")).toBeInTheDocument();

    rerender(<FileTabs {...base} pokedFiles={new Set(["pokes.lua"])} />);
    expect(screen.getByText("⚡")).toBeInTheDocument();
  });

  it("selects and reorders files from the keyboard", () => {
    const onSelect = vi.fn();
    const onReorder = vi.fn();
    render(
      <FileTabs
        {...base}
        files={["pokes.lua", "main.lua", "fx.lua"]}
        onSelect={onSelect}
        onReorder={onReorder}
      />,
    );

    const main = screen.getByRole("tab", { name: /main.lua/i });
    fireEvent.keyDown(main, { key: "ArrowLeft" });
    expect(onSelect).toHaveBeenCalledWith("pokes.lua");

    fireEvent.keyDown(main, { key: "ArrowRight", altKey: true });
    expect(onReorder).toHaveBeenCalledWith(1, 2);
  });
});

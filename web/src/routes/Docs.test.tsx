// @vitest-environment jsdom
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Docs } from "./Docs";

describe("Docs", () => {
  it("finds chapters by register, concept, or Lua name", () => {
    const { container } = render(<Docs />);
    const search = screen.getByRole("searchbox", { name: /search the guide/i });
    const map = () => within(container.querySelector(".docs-map")!);

    fireEvent.change(search, { target: { value: "CGADSUB" } });
    expect(map().getByRole("link", { name: /screens & color math/i })).toBeTruthy();
    expect(map().queryByRole("link", { name: /^display/i })).toBeNull();

    fireEvent.change(search, { target: { value: "not-a-ppu-thing" } });
    expect(screen.getByRole("status").textContent).toContain("0 matching chapters");
    expect(screen.getByText(/try a register such as/i)).toBeTruthy();
  });
});

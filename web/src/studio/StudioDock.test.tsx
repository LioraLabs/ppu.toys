// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DockviewApi } from "dockview-react";
import { LayoutMenu } from "./StudioDock";

afterEach(cleanup);

describe("LayoutMenu", () => {
  it("reopens a closed inspector panel", () => {
    const addPanel = vi.fn(() => ({ api: { setSize: vi.fn() } }));
    const api = {
      getPanel: vi.fn((id: string) => (id === "trace" ? {} : undefined)),
      addPanel,
      onDidLayoutChange: vi.fn(() => ({ dispose: vi.fn() })),
    } as unknown as DockviewApi;

    render(<LayoutMenu api={api} />);
    fireEvent.click(screen.getByRole("button", { name: /panel/i }));
    fireEvent.click(screen.getByRole("button", { name: "MODE 7" }));

    expect(addPanel).toHaveBeenCalledWith(
      expect.objectContaining({ id: "m7", position: { referencePanel: "trace" } }),
    );
  });

  it("closes the panel menu with Escape and restores trigger focus", () => {
    const api = {
      getPanel: vi.fn(),
      onDidLayoutChange: vi.fn(() => ({ dispose: vi.fn() })),
    } as unknown as DockviewApi;
    render(<LayoutMenu api={api} />);
    const trigger = screen.getByRole("button", { name: /panel/i });
    fireEvent.click(trigger);
    fireEvent.keyDown(screen.getByLabelText(/studio panels/i), { key: "Escape" });
    expect(screen.queryByLabelText(/studio panels/i)).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("does not take focus when Escape comes from panel content", () => {
    const api = {
      getPanel: vi.fn(),
      onDidLayoutChange: vi.fn(() => ({ dispose: vi.fn() })),
    } as unknown as DockviewApi;
    render(
      <>
        <LayoutMenu api={api} />
        <div contentEditable aria-label="Editor" />
        <div tabIndex={0} aria-label="Game" />
      </>,
    );
    fireEvent.click(screen.getByRole("button", { name: /panel/i }));
    for (const label of ["Editor", "Game"]) {
      const surface = screen.getByLabelText(label);
      surface.focus();
      expect(fireEvent.keyDown(surface, { key: "Escape" })).toBe(true);
      expect(surface).toHaveFocus();
      expect(screen.getByLabelText(/studio panels/i)).toBeInTheDocument();
    }
  });
});

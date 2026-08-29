// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DockviewApi } from "dockview-react";
import { LayoutMenu } from "./StudioDock";

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
});

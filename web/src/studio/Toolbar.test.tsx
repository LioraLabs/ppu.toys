// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, it, vi } from "vitest";
import { Toolbar } from "./Toolbar";

afterEach(cleanup);

it("renames the open toy from the toolbar", () => {
  const rename = vi.fn();
  render(<Toolbar sketchName="old name" onRename={rename} />);

  const input = screen.getByRole("textbox", { name: "Toy name" });
  fireEvent.change(input, { target: { value: "new name" } });
  fireEvent.blur(input);
  expect(rename).toHaveBeenCalledWith("new name");

  fireEvent.change(input, { target: { value: "  " } });
  fireEvent.blur(input);
  expect((input as HTMLInputElement).value).toBe("old name");
});

it("exposes unsaved and settings state", () => {
  render(<Toolbar dirty onToggleSettings={() => {}} settingsOpen />);
  expect(screen.getByRole("status")).toHaveTextContent("Unsaved changes");
  expect(screen.getByRole("button", { name: "Editor settings" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
});

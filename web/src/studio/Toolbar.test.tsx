// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { Toolbar } from "./Toolbar";

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

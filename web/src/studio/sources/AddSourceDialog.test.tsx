// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import { AddSourceDialog } from "./AddSourceDialog";
import { transport } from "../transport/transport";
import * as decode from "../assets/decode";

function fakeImageData(w = 16, h = 8): ImageData {
  return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4), colorSpace: "srgb" } as ImageData;
}

describe("AddSourceDialog", () => {
  beforeEach(() => {
    vi.spyOn(decode, "decodeImageFile").mockResolvedValue({ name: "track.png", imageData: fakeImageData(), preview: "" });
  });

  afterEach(() => cleanup());

  it("drops a PNG, converts, names it, and registers via transport.addSource", async () => {
    const add = vi.spyOn(transport, "addSource").mockReturnValue({ ok: true });
    const onClose = vi.fn();
    render(<AddSourceDialog onClose={onClose} />);

    const drop = screen.getByText(/drop png/i);
    const file = new File([new Uint8Array([1])], "track.png", { type: "image/png" });
    fireEvent.drop(drop, { dataTransfer: { files: [file] } });

    // name auto-fills from file; Add becomes enabled after conversion
    await waitFor(() => expect(screen.getByRole("button", { name: /add source/i })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: /add source/i }));

    expect(add).toHaveBeenCalledWith("track", expect.any(Uint8Array));
    expect(onClose).toHaveBeenCalled();
  });

  it("switches kind to obj and shows the pre-crop scope note", async () => {
    render(<AddSourceDialog onClose={() => {}} />);
    fireEvent.change(screen.getByLabelText(/kind/i), { target: { value: "obj" } });
    expect(screen.getByText(/pre-crop/i)).toBeInTheDocument();
  });
});

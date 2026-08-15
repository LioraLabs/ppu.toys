// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import { AddSourceDialog } from "./AddSourceDialog";
import { transport } from "../transport/transport";
import * as decode from "../assets/decode";
import { ppuCore, setPpuCore } from "../../ppu/instance";
import type { ConvertSourceOptions, PpuCore, SourceKind } from "../../ppu/core";

function fakeImageData(w = 16, h = 8): ImageData {
  return {
    width: w,
    height: h,
    data: new Uint8ClampedArray(w * h * 4),
    colorSpace: "srgb",
  } as ImageData;
}

/** Records what the dialog asks the core to convert, delegating to the stub. */
let converts: { kind: SourceKind; options: ConvertSourceOptions }[] = [];

describe("AddSourceDialog", () => {
  let realCore: PpuCore;

  beforeEach(() => {
    vi.spyOn(decode, "decodeImageFile").mockResolvedValue({
      name: "track.png",
      imageData: fakeImageData(),
      preview: "",
    });
    converts = [];
    realCore = ppuCore;
    const rec: PpuCore = Object.create(realCore);
    rec.convertSource = (kind, options, imageData) => {
      converts.push({ kind, options });
      return realCore.convertSource(kind, options, imageData);
    };
    setPpuCore(rec);
  });

  afterEach(() => {
    setPpuCore(realCore);
    cleanup();
  });

  async function dropPng() {
    const drop = screen.getByText(/drop png/i);
    const file = new File([new Uint8Array([1])], "track.png", { type: "image/png" });
    fireEvent.drop(drop, { dataTransfer: { files: [file] } });
    await waitFor(() => expect(screen.getByRole("button", { name: /add source/i })).toBeEnabled());
  }

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

  // PPU-94: the Tilesheet kind takes bit depth + the remap options, and has no
  // cell-size option at all — sheet cells are fixed 8x8.
  it("offers the Tilesheet kind: bit depth and remap options apply, no cell size", async () => {
    render(<AddSourceDialog onClose={() => {}} />);
    await dropPng();

    fireEvent.change(screen.getByLabelText(/kind/i), { target: { value: "sheet" } });
    fireEvent.change(screen.getByLabelText(/bit depth/i), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText(/alpha threshold/i), { target: { value: "200" } });

    expect(screen.queryByLabelText(/cell \/ sprite size/i)).not.toBeInTheDocument();
    expect(screen.getByText(/cells fixed at 8×8/i)).toBeInTheDocument();

    expect(converts[converts.length - 1]).toEqual({
      kind: "sheet",
      options: {
        bit_depth: 2,
        tile_size: 8,
        dither: "none",
        dither_strength: 50,
        alpha_threshold: 200,
      },
    });
  });
});

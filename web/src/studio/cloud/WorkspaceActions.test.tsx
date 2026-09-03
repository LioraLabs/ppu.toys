// @vitest-environment jsdom
import "fake-indexeddb/auto";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { WorkspaceActions } from "./WorkspaceActions";
import { openSketchStore } from "../sketches/openSketch";
import { _resetSketchStoreForTests } from "../sketches/sketchStore";
import { serializeToFile, _resetLocalFileForTests, PPU_FILE_VERSION } from "./localFile";
import type { Me } from "../../api/apiClient";

vi.mock("../../api/apiClient", () => ({
  SIGN_IN_URL: "/api/auth/discord",
}));
vi.mock("../../api/session", () => ({
  useSession: vi.fn(),
  sessionStore: { refresh: vi.fn() },
}));

import { SIGN_IN_URL } from "../../api/apiClient";
import { useSession } from "../../api/session";

const mockUseSession = useSession as unknown as ReturnType<typeof vi.fn>;

const USER: Me = { id: "u1", handle: "ada", avatar: null, isAdmin: false };

function makePicker() {
  let written = "";
  const writable = { write: vi.fn(async (t: string) => void (written = t)), close: vi.fn() };
  const handle = { createWritable: vi.fn(async () => writable) };
  const picker = vi.fn(async () => handle);
  (window as unknown as { showSaveFilePicker: typeof picker }).showSaveFilePicker = picker;
  return { picker, writable, getWritten: () => written };
}

function queryFileInput(container: HTMLElement): HTMLInputElement {
  return container.querySelector('input[type="file"]') as HTMLInputElement;
}

beforeEach(() => {
  (globalThis as { indexedDB: IDBFactory }).indexedDB = new IDBFactory();
  _resetSketchStoreForTests();
  openSketchStore._resetForTests();
  _resetLocalFileForTests();
  mockUseSession.mockReset();
  delete (window as unknown as { showSaveFilePicker?: unknown }).showSaveFilePicker;
});
afterEach(() => cleanup());

describe("WorkspaceActions", () => {
  it("signed-out: Sign in link, Save and Open… present, no Publish…; Ctrl+S writes a versioned file", async () => {
    mockUseSession.mockReturnValue({ user: null, loading: false });
    const { picker, getWritten } = makePicker();
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    const link = screen.getByRole("link", { name: /sign in to publish/i });
    expect(link).toHaveAttribute("href", SIGN_IN_URL);
    expect(screen.getByRole("button", { name: /^save$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^open…$/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^publish…$/i })).not.toBeInTheDocument();

    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    await waitFor(() => expect(picker).toHaveBeenCalledTimes(1));
    const written = getWritten();
    expect(JSON.parse(written).version).toBe(PPU_FILE_VERSION);
  });

  it("signed-in: Save twice reuses the same picker handle but writes twice", async () => {
    mockUseSession.mockReturnValue({ user: USER, loading: false });
    const { picker, writable } = makePicker();
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    const saveBtn = screen.getByRole("button", { name: /^save$/i });
    fireEvent.click(saveBtn);
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Saved"));
    fireEvent.click(saveBtn);
    await waitFor(() => expect(writable.write).toHaveBeenCalledTimes(2));

    expect(picker).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status")).toHaveTextContent("Saved");
  });

  it("signed-in: Open… with a bad file shows an error and leaves the open sketch untouched", async () => {
    mockUseSession.mockReturnValue({ user: USER, loading: false });
    const before = openSketchStore.state().context.sketch.id;
    const { container } = render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    const input = queryFileInput(container);
    expect(input).toBeTruthy();
    const bad = new File(['{"version":"ppu.toys/0"}'], "bad.ppu.json");
    fireEvent.change(input, { target: { files: [bad] } });

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/version/i));
    expect(openSketchStore.state().context.sketch.id).toBe(before);
  });

  it.each([
    ["signed-out", null],
    ["signed-in", USER],
  ])(
    "%s: Open… a good file with an origin re-links it and changes the sketch id",
    async (_label, user) => {
      mockUseSession.mockReturnValue({ user, loading: false });
      openSketchStore.editFile("main.lua", "-- hello");
      openSketchStore.setOrigin({ id: "toy9", revision: 2, authorId: "u1" });
      const text = serializeToFile(openSketchStore.state());
      const beforeId = openSketchStore.state().context.sketch.id;
      openSketchStore._resetForTests();

      const { container } = render(
        <MemoryRouter>
          <WorkspaceActions />
        </MemoryRouter>,
      );

      const input = queryFileInput(container);
      fireEvent.change(input, { target: { files: [new File([text], "good.ppu.json")] } });

      await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Opened"));
      expect(openSketchStore.state().context.sketch.origin?.id).toBe("toy9");
      expect(openSketchStore.state().context.sketch.id).not.toBe(beforeId);
    },
  );

  it("signed-out: an edit after mount is still what Ctrl+S writes (no stale closure)", async () => {
    mockUseSession.mockReturnValue({ user: null, loading: false });
    const { picker, getWritten } = makePicker();
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    openSketchStore.editFile("main.lua", "-- edited");
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });

    await waitFor(() => expect(picker).toHaveBeenCalledTimes(1));
    expect(getWritten()).toContain("-- edited");
  });

  it("signed-in: a cancelled picker (AbortError) shows no Saved status", async () => {
    mockUseSession.mockReturnValue({ user: USER, loading: false });
    const abort = Object.assign(new Error("cancelled"), { name: "AbortError" });
    const picker = vi.fn(async () => {
      throw abort;
    });
    (window as unknown as { showSaveFilePicker: typeof picker }).showSaveFilePicker = picker;
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    const saveBtn = screen.getByRole("button", { name: /^save$/i });
    fireEvent.click(saveBtn);
    await waitFor(() => expect(picker).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(saveBtn).toBeEnabled());

    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

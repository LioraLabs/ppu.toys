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
import type { Me } from "../../api/apiClient";

vi.mock("../../api/apiClient", () => ({
  SIGN_IN_URL: "/api/auth/discord",
  createToy: vi.fn(),
  updateToy: vi.fn(),
}));
vi.mock("../../api/session", () => ({
  useSession: vi.fn(),
  sessionStore: { refresh: vi.fn() },
}));
vi.mock("./serialize", () => ({
  serializeWorkspace: () => ({
    files: [{ name: "main.lua", source: "x" }],
    sources: [{ name: "sky", kind: "bg", builtinId: null, options: {}, meta: {}, payload: "AQ==" }],
  }),
}));

import { createToy, updateToy, SIGN_IN_URL } from "../../api/apiClient";
import { useSession } from "../../api/session";

const mockUseSession = useSession as unknown as ReturnType<typeof vi.fn>;
const mockCreateToy = createToy as unknown as ReturnType<typeof vi.fn>;
const mockUpdateToy = updateToy as unknown as ReturnType<typeof vi.fn>;

const USER: Me = { id: "u1", handle: "ada", avatar: null, isAdmin: false };

beforeEach(() => {
  (globalThis as { indexedDB: IDBFactory }).indexedDB = new IDBFactory();
  _resetSketchStoreForTests();
  openSketchStore._resetForTests();
  mockCreateToy.mockReset();
  mockUpdateToy.mockReset();
  mockUseSession.mockReset();
});
afterEach(() => cleanup());

describe("WorkspaceActions", () => {
  it("signed-out: shows a Sign in to publish link, no Save button", () => {
    mockUseSession.mockReturnValue({ user: null, loading: false });
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );
    const link = screen.getByRole("link", { name: /sign in to publish/i });
    expect(link).toHaveAttribute("href", SIGN_IN_URL);
    expect(screen.queryByRole("button", { name: /^save$/i })).not.toBeInTheDocument();
  });

  it("signed-in: Save creates the toy, then updates it with the revision the previous save returned", async () => {
    mockUseSession.mockReturnValue({ user: USER, loading: false });
    mockCreateToy.mockResolvedValue({ id: "toy1", revision: 1 });
    mockUpdateToy.mockResolvedValueOnce({ revision: 2 }).mockResolvedValueOnce({ revision: 3 });
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    const saveBtn = screen.getByRole("button", { name: /^save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => expect(mockCreateToy).toHaveBeenCalledTimes(1));
    const body = mockCreateToy.mock.calls[0][0];
    expect(
      body.sources.every(
        (s: { payload: unknown }) => typeof s.payload === "string" && s.payload.length > 0,
      ),
    ).toBe(true);
    expect(typeof body.description).toBe("string");
    expect(body.description).toBe("");

    fireEvent.click(saveBtn);
    await waitFor(() => expect(mockUpdateToy).toHaveBeenCalledTimes(1));
    expect(mockUpdateToy.mock.calls[0][1]).toBe(1);
    expect(mockCreateToy).toHaveBeenCalledTimes(1);

    fireEvent.click(saveBtn);
    await waitFor(() => expect(mockUpdateToy).toHaveBeenCalledTimes(2));
    expect(mockUpdateToy.mock.calls[1][1]).toBe(2);
    expect(mockCreateToy).toHaveBeenCalledTimes(1);
  });

  it("shows unlinked, then linked to t/<id> after a save, then unlinked again after Unlink; the next save creates a new toy", async () => {
    mockUseSession.mockReturnValue({ user: USER, loading: false });
    mockCreateToy.mockResolvedValueOnce({ id: "toy1", revision: 1 }).mockResolvedValueOnce({
      id: "toy2",
      revision: 1,
    });
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    expect(screen.getByText("unlinked")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(mockCreateToy).toHaveBeenCalledTimes(1));
    const unlinkBtn = await screen.findByRole("button", { name: /unlink/i });
    expect(screen.getByText(/linked to t\/toy1/)).toBeInTheDocument();

    fireEvent.click(unlinkBtn);
    await waitFor(() => expect(screen.getByText("unlinked")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(mockCreateToy).toHaveBeenCalledTimes(2));
    expect(mockUpdateToy).not.toHaveBeenCalled();
  });

  it("an origin authored by someone else always creates a new toy, never updates", async () => {
    mockUseSession.mockReturnValue({ user: USER, loading: false });
    openSketchStore.setOrigin({ id: "other", revision: 7, authorId: "someone-else" });
    mockCreateToy.mockResolvedValue({ id: "toy1", revision: 1 });
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(mockCreateToy).toHaveBeenCalledTimes(1));
    expect(mockUpdateToy).not.toHaveBeenCalled();
  });
});

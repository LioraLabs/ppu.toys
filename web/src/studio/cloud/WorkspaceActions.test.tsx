// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { WorkspaceActions } from "./WorkspaceActions";
import { cloudDraft } from "./cloudDraft";
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

const USER: Me = { id: "u1", handle: "ada", isAdmin: false };

beforeEach(() => {
  cloudDraft._resetForTests();
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

  it("signed-in: Save creates the toy, then a second Save updates it", async () => {
    mockUseSession.mockReturnValue({ user: USER, loading: false });
    mockCreateToy.mockResolvedValue({ id: "toy1" });
    mockUpdateToy.mockResolvedValue(undefined);
    render(
      <MemoryRouter>
        <WorkspaceActions />
      </MemoryRouter>,
    );

    const saveBtn = screen.getByRole("button", { name: /^save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => expect(mockCreateToy).toHaveBeenCalledTimes(1));
    const body = mockCreateToy.mock.calls[0][0];
    expect(body.sources.every((s: { payload: unknown }) => typeof s.payload === "string" && s.payload.length > 0)).toBe(true);
    expect(typeof body.description).toBe("string");
    expect(body.description).toBe("");

    fireEvent.click(saveBtn);
    await waitFor(() => expect(mockUpdateToy).toHaveBeenCalledTimes(1));
    expect(mockCreateToy).toHaveBeenCalledTimes(1);
  });
});

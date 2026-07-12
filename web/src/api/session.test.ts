import { describe, it, expect, afterEach, vi } from "vitest";
import { sessionStore } from "./session";

vi.mock("./apiClient", () => ({
  getMe: vi.fn(),
  logout: vi.fn(async () => {}),
}));
import { getMe, logout } from "./apiClient";

afterEach(() => {
  vi.clearAllMocks();
  sessionStore._resetForTests();
});

describe("sessionStore", () => {
  it("starts loading with no user", () => {
    expect(sessionStore.get()).toEqual({ user: null, loading: true });
  });

  it("refresh loads the current user and clears loading", async () => {
    (getMe as ReturnType<typeof vi.fn>).mockResolvedValue({ id: "1", handle: "ada", isAdmin: false });
    await sessionStore.refresh();
    expect(sessionStore.get()).toEqual({ user: { id: "1", handle: "ada", isAdmin: false }, loading: false });
  });

  it("refresh on a signed-out session yields a null user", async () => {
    (getMe as ReturnType<typeof vi.fn>).mockResolvedValue(null);
    await sessionStore.refresh();
    expect(sessionStore.get()).toEqual({ user: null, loading: false });
  });

  it("signOut calls the API then refreshes to null", async () => {
    (getMe as ReturnType<typeof vi.fn>).mockResolvedValue(null);
    await sessionStore.signOut();
    expect(logout).toHaveBeenCalledOnce();
    expect(sessionStore.get().user).toBeNull();
  });

  it("notifies subscribers on change", async () => {
    (getMe as ReturnType<typeof vi.fn>).mockResolvedValue(null);
    let hits = 0;
    const unsub = sessionStore.subscribe(() => hits++);
    await sessionStore.refresh();
    unsub();
    expect(hits).toBeGreaterThan(0);
  });
});

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { getMe, getWall, getToy, getProfile } from "./apiClient";

function mockFetch(status: number, body: unknown) {
  return vi.fn(async () =>
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    }),
  );
}

let fetchSpy: ReturnType<typeof mockFetch>;
afterEach(() => vi.unstubAllGlobals());

describe("read endpoints", () => {
  it("getMe returns the user on 200", async () => {
    fetchSpy = mockFetch(200, { id: "1", handle: "ada", isAdmin: false });
    vi.stubGlobal("fetch", fetchSpy);
    expect(await getMe()).toEqual({ id: "1", handle: "ada", isAdmin: false });
    const [url, init] = fetchSpy.mock.calls[0];
    expect(url).toBe("/api/me");
    expect(init.credentials).toBe("include");
  });

  it("getMe returns null on 401 (signed out)", async () => {
    vi.stubGlobal("fetch", mockFetch(401, undefined));
    expect(await getMe()).toBeNull();
  });

  it("getWall builds the sort+page query", async () => {
    fetchSpy = mockFetch(200, { toys: [], nextPage: null });
    vi.stubGlobal("fetch", fetchSpy);
    await getWall("popular", 2);
    expect(fetchSpy.mock.calls[0][0]).toBe("/api/toys?sort=popular&page=2");
  });

  it("getToy hits /api/toys/:id", async () => {
    fetchSpy = mockFetch(200, { id: "abc", files: [], sources: [] });
    vi.stubGlobal("fetch", fetchSpy);
    await getToy("abc");
    expect(fetchSpy.mock.calls[0][0]).toBe("/api/toys/abc");
  });

  it("getProfile hits /api/users/:handle", async () => {
    fetchSpy = mockFetch(200, { user: { handle: "ada", avatar: null }, toys: [] });
    vi.stubGlobal("fetch", fetchSpy);
    await getProfile("ada");
    expect(fetchSpy.mock.calls[0][0]).toBe("/api/users/ada");
  });

  it("throws on a 500", async () => {
    vi.stubGlobal("fetch", mockFetch(500, { error: "boom" }));
    await expect(getToy("x")).rejects.toThrow();
  });
});

describe("mutations send X-PPU-CSRF", () => {
  it("forkToy POSTs with the CSRF header and returns the new id", async () => {
    const spy = mockFetch(200, { id: "new1" });
    vi.stubGlobal("fetch", spy);
    const { forkToy } = await import("./apiClient");
    expect(await forkToy("abc")).toEqual({ id: "new1" });
    const [url, init] = spy.mock.calls[0];
    expect(url).toBe("/api/toys/abc/fork");
    expect(init.method).toBe("POST");
    expect((init.headers as Record<string, string>)["X-PPU-CSRF"]).toBe("1");
    expect(init.credentials).toBe("include");
  });

  it("addHeart PUTs, removeHeart DELETEs, both with CSRF", async () => {
    const spy = mockFetch(204, undefined);
    vi.stubGlobal("fetch", spy);
    const { addHeart, removeHeart } = await import("./apiClient");
    await addHeart("abc");
    await removeHeart("abc");
    expect(spy.mock.calls[0][1].method).toBe("PUT");
    expect(spy.mock.calls[1][1].method).toBe("DELETE");
    expect((spy.mock.calls[0][1].headers as Record<string, string>)["X-PPU-CSRF"]).toBe("1");
  });

  it("logout POSTs to /api/auth/logout with CSRF", async () => {
    const spy = mockFetch(204, undefined);
    vi.stubGlobal("fetch", spy);
    const { logout } = await import("./apiClient");
    await logout();
    expect(spy.mock.calls[0][0]).toBe("/api/auth/logout");
    expect(spy.mock.calls[0][1].method).toBe("POST");
  });
});

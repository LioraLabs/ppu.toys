import { describe, it, expect, afterEach, vi } from "vitest";
import { getMe, getWall, getToy, getProfile } from "./apiClient";
import type { SaveToyBody } from "./apiClient";

function mockFetch(status: number, body: unknown) {
  return vi.fn(
    (_input: RequestInfo | URL, _init?: RequestInit): Promise<Response> =>
      Promise.resolve(
        new Response(body === undefined ? null : JSON.stringify(body), {
          status,
          headers: { "content-type": "application/json" },
        }),
      ),
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("read endpoints", () => {
  it("getMe returns the user on 200 with credentials included", async () => {
    const fetchSpy = mockFetch(200, { id: "1", handle: "ada", isAdmin: false });
    vi.stubGlobal("fetch", fetchSpy);
    expect(await getMe()).toEqual({ id: "1", handle: "ada", isAdmin: false });
    expect(fetchSpy).toHaveBeenCalledWith("/api/me", { credentials: "include" });
  });

  it("getMe returns null on 401 (signed out)", async () => {
    vi.stubGlobal("fetch", mockFetch(401, undefined));
    expect(await getMe()).toBeNull();
  });

  it("getWall builds the sort+page query", async () => {
    const fetchSpy = mockFetch(200, { toys: [], nextPage: null });
    vi.stubGlobal("fetch", fetchSpy);
    await getWall("popular", 2);
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/toys?sort=popular&page=2",
      expect.objectContaining({ credentials: "include" }),
    );
  });

  it("getToy hits /api/toys/:id", async () => {
    const fetchSpy = mockFetch(200, { id: "abc", files: [], sources: [] });
    vi.stubGlobal("fetch", fetchSpy);
    await getToy("abc");
    expect(fetchSpy).toHaveBeenCalledWith("/api/toys/abc", expect.anything());
  });

  it("getProfile hits /api/users/:handle", async () => {
    const fetchSpy = mockFetch(200, { user: { handle: "ada", avatar: null }, toys: [] });
    vi.stubGlobal("fetch", fetchSpy);
    await getProfile("ada");
    expect(fetchSpy).toHaveBeenCalledWith("/api/users/ada", expect.anything());
  });

  it("read requests do NOT send the CSRF header", async () => {
    const fetchSpy = mockFetch(200, { toys: [], nextPage: null });
    vi.stubGlobal("fetch", fetchSpy);
    await getWall("recent", 0);
    expect(fetchSpy).not.toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ headers: expect.objectContaining({ "X-PPU-CSRF": "1" }) }),
    );
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
    expect(spy).toHaveBeenCalledWith(
      "/api/toys/abc/fork",
      expect.objectContaining({
        method: "POST",
        credentials: "include",
        headers: expect.objectContaining({ "X-PPU-CSRF": "1" }),
      }),
    );
  });

  it("addHeart PUTs, removeHeart DELETEs, both with CSRF", async () => {
    const spy = mockFetch(204, undefined);
    vi.stubGlobal("fetch", spy);
    const { addHeart, removeHeart } = await import("./apiClient");
    await addHeart("abc");
    await removeHeart("abc");
    expect(spy).toHaveBeenNthCalledWith(
      1,
      "/api/toys/abc/heart",
      expect.objectContaining({
        method: "PUT",
        headers: expect.objectContaining({ "X-PPU-CSRF": "1" }),
      }),
    );
    expect(spy).toHaveBeenNthCalledWith(
      2,
      "/api/toys/abc/heart",
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("logout POSTs to /api/auth/logout with CSRF", async () => {
    const spy = mockFetch(204, undefined);
    vi.stubGlobal("fetch", spy);
    const { logout } = await import("./apiClient");
    await logout();
    expect(spy).toHaveBeenCalledWith(
      "/api/auth/logout",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({ "X-PPU-CSRF": "1" }),
      }),
    );
  });
});

describe("write endpoints", () => {
  const saveBody: SaveToyBody = {
    title: "My Toy",
    description: "desc",
    files: [{ name: "main.lua", source: "-- code" }],
    sources: [{ name: "s1", kind: "builtin", builtinId: "b1", options: {}, meta: {}, payload: "YmFzZTY0" }],
  };

  it("createToy POSTs /api/toys with CSRF, JSON content-type, and body; returns id", async () => {
    const spy = mockFetch(200, { id: "new1" });
    vi.stubGlobal("fetch", spy);
    const { createToy } = await import("./apiClient");
    expect(await createToy(saveBody)).toEqual({ id: "new1" });
    expect(spy).toHaveBeenCalledWith(
      "/api/toys",
      expect.objectContaining({
        method: "POST",
        credentials: "include",
        headers: expect.objectContaining({
          "X-PPU-CSRF": "1",
          "content-type": "application/json",
        }),
        body: JSON.stringify(saveBody),
      }),
    );
  });

  it("updateToy PUTs /api/toys/:id with CSRF and JSON body; 204 resolves undefined", async () => {
    const spy = mockFetch(204, undefined);
    vi.stubGlobal("fetch", spy);
    const { updateToy } = await import("./apiClient");
    expect(await updateToy("abc", saveBody)).toBeUndefined();
    expect(spy).toHaveBeenCalledWith(
      "/api/toys/abc",
      expect.objectContaining({
        method: "PUT",
        credentials: "include",
        headers: expect.objectContaining({
          "X-PPU-CSRF": "1",
          "content-type": "application/json",
        }),
        body: JSON.stringify(saveBody),
      }),
    );
  });

  it("publishToy POSTs /api/toys/:id/publish with FormData body, CSRF, and no content-type override", async () => {
    const spy = mockFetch(200, { id: "abc", state: "published" });
    vi.stubGlobal("fetch", spy);
    const { publishToy } = await import("./apiClient");
    const clip = new Blob(["clipdata"], { type: "video/webm" });
    const thumb = new Blob(["thumbdata"], { type: "image/png" });
    const result = await publishToy("abc", { title: "My Toy", description: "desc" }, clip, thumb);
    expect(result).toEqual({ id: "abc", state: "published" });

    expect(spy).toHaveBeenCalledTimes(1);
    const [calledUrl, calledInit] = spy.mock.calls[0];
    if (!calledInit) throw new Error("fetch was called without an init argument");
    expect(calledUrl).toBe("/api/toys/abc/publish");
    expect(calledInit.method).toBe("POST");
    expect(calledInit.credentials).toBe("include");
    expect(calledInit.body).toBeInstanceOf(FormData);
    expect((calledInit.headers as Record<string, string>)["X-PPU-CSRF"]).toBe("1");
    expect(calledInit.headers).not.toHaveProperty("content-type");

    const fd = calledInit.body as FormData;
    expect(JSON.parse(fd.get("meta") as string)).toEqual({ title: "My Toy", description: "desc" });
    expect(fd.get("clip")).toBeInstanceOf(Blob);
    expect(fd.get("thumb")).toBeInstanceOf(Blob);
  });
});

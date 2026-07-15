// @vitest-environment jsdom
//
// jsdom (not the default node environment) so `location` exists: MSW resolves
// handlers registered with root-relative paths (e.g. "/api/me") against
// `location.href` — under plain "node" there's no location, so relative
// patterns never match a request's absolute URL.
import { describe, it, expect, afterEach } from "vitest";
import { http, HttpResponse } from "msw";
import { server } from "../mocks/server";
import { me } from "../fixtures";
import { sessionStore } from "./session";

afterEach(() => {
  sessionStore._resetForTests();
});

describe("sessionStore", () => {
  it("starts loading with no user", () => {
    expect(sessionStore.get()).toEqual({ user: null, loading: true });
  });

  it("refresh loads the current user and clears loading", async () => {
    await sessionStore.refresh();
    expect(sessionStore.get()).toEqual({ user: me, loading: false });
  });

  it("refresh on a signed-out session yields a null user", async () => {
    server.use(http.get("/api/me", () => new HttpResponse(null, { status: 401 })));
    await sessionStore.refresh();
    expect(sessionStore.get()).toEqual({ user: null, loading: false });
  });

  it("signOut calls the API then refreshes to null", async () => {
    let logoutHit = false;
    server.use(
      http.get("/api/me", () => new HttpResponse(null, { status: 401 })),
      http.post("/api/auth/logout", () => {
        logoutHit = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await sessionStore.signOut();
    expect(logoutHit).toBe(true);
    expect(sessionStore.get().user).toBeNull();
  });

  it("notifies subscribers on change", async () => {
    let hits = 0;
    const unsub = sessionStore.subscribe(() => hits++);
    await sessionStore.refresh();
    unsub();
    expect(hits).toBeGreaterThan(0);
  });
});

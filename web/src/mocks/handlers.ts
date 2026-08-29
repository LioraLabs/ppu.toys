/** MSW request handlers covering the full apiClient wire surface. Responses
 *  are built from `src/fixtures/` — the single source of truth for payload
 *  shapes — so handlers never duplicate fixture literals. */

import { http, HttpResponse } from "msw";

import { me, profile, toyFull, wallPage } from "../fixtures";

export const handlers = [
  http.get("/api/starter", () =>
    HttpResponse.json({
      name: "untitled toy",
      files: [{ name: "main.lua", source: "function frame(t, f)\n  apply_pokes()\nend\n" }],
    }),
  ),

  http.get("/api/me", () => HttpResponse.json(me)),

  http.get("/api/toys", () => HttpResponse.json(wallPage)),

  http.get("/api/highlights", () => HttpResponse.json({ toys: wallPage.toys })),

  http.get("/api/toys/:id", () => HttpResponse.json(toyFull)),

  http.get("/api/users/:handle", () => HttpResponse.json(profile)),

  http.post("/api/toys/:id/fork", () => HttpResponse.json({ id: "fork1" })),

  http.put("/api/toys/:id/heart", () => new HttpResponse(null, { status: 204 })),

  http.delete("/api/toys/:id/heart", () => new HttpResponse(null, { status: 204 })),

  http.post("/api/auth/logout", () => new HttpResponse(null, { status: 204 })),

  http.post("/api/toys", () => HttpResponse.json({ id: "new1", revision: 1 })),

  http.put("/api/toys/:id", () => HttpResponse.json({ revision: 2 })),

  http.post("/api/toys/:id/publish", () => HttpResponse.json({ id: "abc123", state: "published" })),
];

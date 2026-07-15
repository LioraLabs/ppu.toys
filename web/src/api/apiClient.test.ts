// @vitest-environment jsdom
//
// jsdom (not the default node environment) so `location` exists: MSW resolves
// handlers registered with root-relative paths (e.g. "/api/me") against
// `location.href` — under plain "node" there's no location, so relative
// patterns never match a request's absolute URL.
import { describe, it, expect } from "vitest";
import { http, HttpResponse } from "msw";
import { server } from "../mocks/server";
import { me, profile, toyFull, wallPage } from "../fixtures";
import {
  getMe,
  getWall,
  getToy,
  getProfile,
  forkToy,
  addHeart,
  removeHeart,
  logout,
  createToy,
  updateToy,
  publishToy,
} from "./apiClient";
import type { SaveToyBody } from "./apiClient";

// jsdom's Blob only implements slice()/size/type — no arrayBuffer()/text()/
// stream(). The real fetch (undici, patched by MSW) needs those to serialize
// a multipart FormData body; without them, publishToy's request hangs
// forever trying to read the parts. Patch them in: arrayBuffer() via
// FileReader (which jsdom implements fully against the same Blob data), then
// text()/stream() built on top.
if (typeof Blob.prototype.arrayBuffer !== "function") {
  Blob.prototype.arrayBuffer = function (this: Blob): Promise<ArrayBuffer> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as ArrayBuffer);
      reader.onerror = () => reject(reader.error);
      reader.readAsArrayBuffer(this);
    });
  };
}
if (typeof Blob.prototype.text !== "function") {
  Blob.prototype.text = function (this: Blob): Promise<string> {
    return this.arrayBuffer().then((buf) => new TextDecoder().decode(buf));
  };
}
if (typeof Blob.prototype.stream !== "function") {
  Blob.prototype.stream = function (this: Blob): ReadableStream<Uint8Array<ArrayBuffer>> {
    const self = this;
    return new ReadableStream<Uint8Array<ArrayBuffer>>({
      async start(controller) {
        controller.enqueue(new Uint8Array(await self.arrayBuffer()));
        controller.close();
      },
    });
  };
}

describe("read endpoints", () => {
  it("getMe returns the user on 200", async () => {
    expect(await getMe()).toEqual(me);
  });

  it("getMe returns null on 401 (signed out)", async () => {
    server.use(http.get("/api/me", () => new HttpResponse(null, { status: 401 })));
    expect(await getMe()).toBeNull();
  });

  it("getWall builds the sort+page query", async () => {
    let captured: Request | undefined;
    server.use(
      http.get("/api/toys", async ({ request }) => {
        captured = request.clone();
        return HttpResponse.json(wallPage);
      }),
    );
    const result = await getWall("popular", 2);
    expect(result).toEqual(wallPage);
    expect(new URL(captured!.url).search).toBe("?sort=popular&page=2");
  });

  it("getToy hits /api/toys/:id", async () => {
    expect(await getToy("abc123")).toEqual(toyFull);
  });

  it("getProfile hits /api/users/:handle", async () => {
    expect(await getProfile("ada")).toEqual(profile);
  });

  it("read requests do NOT send the CSRF header", async () => {
    let captured: Request | undefined;
    server.use(
      http.get("/api/toys", async ({ request }) => {
        captured = request.clone();
        return HttpResponse.json(wallPage);
      }),
    );
    await getWall("recent", 0);
    expect(captured!.headers.get("X-PPU-CSRF")).toBeNull();
  });

  it("throws on a 500", async () => {
    server.use(http.get("/api/toys/:id", () => new HttpResponse(null, { status: 500 })));
    await expect(getToy("x")).rejects.toThrow();
  });
});

describe("mutations send X-PPU-CSRF", () => {
  it("forkToy POSTs with the CSRF header and returns the new id", async () => {
    let captured: Request | undefined;
    server.use(
      http.post("/api/toys/:id/fork", async ({ request }) => {
        captured = request.clone();
        return HttpResponse.json({ id: "fork1" });
      }),
    );
    expect(await forkToy("abc")).toEqual({ id: "fork1" });
    expect(captured!.headers.get("X-PPU-CSRF")).toBe("1");
  });

  it("addHeart PUTs, removeHeart DELETEs, both with CSRF", async () => {
    let putCaptured: Request | undefined;
    let deleteCaptured: Request | undefined;
    server.use(
      http.put("/api/toys/:id/heart", async ({ request }) => {
        putCaptured = request.clone();
        return new HttpResponse(null, { status: 204 });
      }),
      http.delete("/api/toys/:id/heart", async ({ request }) => {
        deleteCaptured = request.clone();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await addHeart("abc");
    await removeHeart("abc");
    expect(putCaptured!.headers.get("X-PPU-CSRF")).toBe("1");
    expect(deleteCaptured!.headers.get("X-PPU-CSRF")).toBe("1");
  });

  it("logout POSTs to /api/auth/logout with CSRF", async () => {
    let captured: Request | undefined;
    server.use(
      http.post("/api/auth/logout", async ({ request }) => {
        captured = request.clone();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await logout();
    expect(captured!.headers.get("X-PPU-CSRF")).toBe("1");
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
    let captured: Request | undefined;
    server.use(
      http.post("/api/toys", async ({ request }) => {
        captured = request.clone();
        return HttpResponse.json({ id: "new1" });
      }),
    );
    expect(await createToy(saveBody)).toEqual({ id: "new1" });
    expect(captured!.headers.get("X-PPU-CSRF")).toBe("1");
    expect(captured!.headers.get("content-type")).toContain("application/json");
    expect(await captured!.json()).toEqual(saveBody);
  });

  it("updateToy PUTs /api/toys/:id with CSRF and JSON body; 204 resolves undefined", async () => {
    let captured: Request | undefined;
    server.use(
      http.put("/api/toys/:id", async ({ request }) => {
        captured = request.clone();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    expect(await updateToy("abc", saveBody)).toBeUndefined();
    expect(captured!.headers.get("X-PPU-CSRF")).toBe("1");
    expect(captured!.headers.get("content-type")).toContain("application/json");
    expect(await captured!.json()).toEqual(saveBody);
  });

  it("publishToy POSTs /api/toys/:id/publish with FormData body, CSRF, and no JSON content-type", async () => {
    let csrfHeader: string | null = null;
    let contentType: string | null = null;
    let fd: FormData | undefined;
    server.use(
      http.post("/api/toys/:id/publish", async ({ request }) => {
        csrfHeader = request.headers.get("X-PPU-CSRF");
        contentType = request.headers.get("content-type");
        fd = await request.formData();
        return HttpResponse.json({ id: "abc", state: "published" });
      }),
    );
    const clip = new Blob(["clipdata"], { type: "video/webm" });
    const thumb = new Blob(["thumbdata"], { type: "image/png" });
    const result = await publishToy("abc", { title: "My Toy", description: "desc" }, clip, thumb);
    expect(result).toEqual({ id: "abc", state: "published" });

    expect(csrfHeader).toBe("1");
    expect(contentType).toMatch(/^multipart\/form-data/);

    expect(JSON.parse(fd!.get("meta") as string)).toEqual({ title: "My Toy", description: "desc" });
    // The handler's `request` is parsed by the real fetch implementation
    // (undici), so its File/Blob parts are undici's classes, not jsdom's
    // `Blob` global — assert shape (present, not a plain string) rather than
    // `instanceof Blob`.
    const clipPart = fd!.get("clip");
    const thumbPart = fd!.get("thumb");
    expect(typeof clipPart).not.toBe("string");
    expect(typeof thumbPart).not.toBe("string");
    expect((clipPart as File).name).toBe("clip.webm");
    expect((thumbPart as File).name).toBe("thumb.png");
  });
});

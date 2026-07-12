/** The ONLY place `fetch` lives. Wraps the frozen S1 contract. Every request
 *  sends the session cookie (`credentials: "include"`); mutations add the
 *  `X-PPU-CSRF: 1` header the server requires. URLs are relative — Vite proxies
 *  /api + /blobs to ppu-server in dev, and prod is same-origin. */

export interface Me { id: string; handle: string; isAdmin: boolean }

export interface WallCard {
  id: string;
  title: string;
  author: { handle: string; avatar: string | null };
  thumbUrl: string;
  clipUrl: string;
  heartCount: number;
  hearted: boolean;
}

export interface WallPage { toys: WallCard[]; nextPage: number | null }

export interface ToyFile { name: string; source: string }

export interface ToySource {
  name: string;
  kind: string;
  builtinId: string | null;
  options: unknown;
  meta: unknown;
  payload: string | null; // base64, null for builtin-reference sources
}

export interface ToyFull {
  id: string;
  title: string;
  description: string;
  state: string;
  files: ToyFile[];
  sources: ToySource[];
  heartCount: number;
  hearted: boolean;
  forkedFrom: string | null;
  author: { id: string; handle: string; avatar: string | null };
}

export interface Profile {
  user: { handle: string; avatar: string | null };
  toys: WallCard[];
}

export type WallSort = "recent" | "popular";

/** Where the sign-in button points. A full-page navigation (302 → Discord),
 *  never a fetch. */
export const SIGN_IN_URL = "/api/auth/discord";

async function request<T>(url: string, init: RequestInit = {}): Promise<T> {
  const method = (init.method ?? "GET").toUpperCase();
  const mutating = method === "POST" || method === "PUT" || method === "DELETE";
  const res = await fetch(url, {
    ...init,
    credentials: "include",
    headers: {
      ...(mutating ? { "X-PPU-CSRF": "1" } : {}),
      ...init.headers,
    },
  });
  if (!res.ok) {
    throw new Error(`${method} ${url} → ${res.status}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

export async function getMe(): Promise<Me | null> {
  const res = await fetch("/api/me", { credentials: "include" });
  if (res.status === 401) return null;
  if (!res.ok) throw new Error(`GET /api/me → ${res.status}`);
  return res.json() as Promise<Me>;
}

export function getWall(sort: WallSort, page: number): Promise<WallPage> {
  return request<WallPage>(`/api/toys?sort=${sort}&page=${page}`);
}

export function getToy(id: string): Promise<ToyFull> {
  return request<ToyFull>(`/api/toys/${id}`);
}

export function getProfile(handle: string): Promise<Profile> {
  return request<Profile>(`/api/users/${handle}`);
}

export function forkToy(id: string): Promise<{ id: string }> {
  return request<{ id: string }>(`/api/toys/${id}/fork`, { method: "POST" });
}

export function addHeart(id: string): Promise<void> {
  return request<void>(`/api/toys/${id}/heart`, { method: "PUT" });
}

export function removeHeart(id: string): Promise<void> {
  return request<void>(`/api/toys/${id}/heart`, { method: "DELETE" });
}

export function logout(): Promise<void> {
  return request<void>("/api/auth/logout", { method: "POST" });
}

export interface SaveToyBody {
  title: string;
  description?: string;
  files: ToyFile[];
  sources: ToySource[];
}

export function createToy(body: SaveToyBody): Promise<{ id: string }> {
  return request<{ id: string }>("/api/toys", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function updateToy(id: string, body: SaveToyBody): Promise<void> {
  return request<void>(`/api/toys/${id}`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

export interface PublishMeta {
  title: string;
  description?: string;
}

export function publishToy(
  id: string,
  meta: PublishMeta,
  clip: Blob,
  thumb: Blob,
): Promise<{ id: string; state: string }> {
  const fd = new FormData();
  fd.append("meta", JSON.stringify(meta));
  fd.append("clip", clip, "clip.webm");
  fd.append("thumb", thumb, "thumb.png");
  return request<{ id: string; state: string }>(`/api/toys/${id}/publish`, {
    method: "POST",
    body: fd,
  });
}

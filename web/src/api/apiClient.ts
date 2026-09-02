/** The ONLY place `fetch` lives. Wraps the frozen S1 contract. Every request
 *  sends the session cookie (`credentials: "include"`); mutations add the
 *  `X-PPU-CSRF: 1` header the server requires. URLs are relative — Vite proxies
 *  /api + /blobs to ppu-server in dev, and prod is same-origin. */

export interface Me {
  id: string;
  handle: string;
  avatar: string | null;
  isAdmin: boolean;
}

export interface WallCard {
  id: string;
  title: string;
  author: { id: string; handle: string; avatar: string | null };
  thumbUrl: string;
  clipUrl: string;
  heartCount: number;
  hearted: boolean;
  createdAt: number;
}

export interface WallPage {
  toys: WallCard[];
  nextPage: number | null;
}

export interface ToyFile {
  name: string;
  source: string;
}

export interface StarterTemplate {
  name: string;
  files: ToyFile[];
}

export interface AdminOverview {
  toys: { id: string; title: string; state: string; author: string; created_at: number }[];
  featuredToys: { id: string; title: string; author: string }[];
  users: {
    id: string;
    handle: string;
    is_admin: boolean;
    banned: boolean;
    created_at: number;
    storage_bytes: number;
  }[];
  storage: { usedBytes: number; limitBytes: number; warning: boolean };
  featuredToyId: string;
  featuredToyIds: string[];
}

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
  revision: number;
  files: ToyFile[];
  sources: ToySource[];
  heartCount: number;
  hearted: boolean;
  forkedFrom: string | null;
  author: { id: string; handle: string; avatar: string | null };
}

export interface DraftInfo {
  id: string;
  title: string;
  createdAt: number;
}

export interface ApiToken {
  id: string;
  name: string;
  createdAt: number;
}

export interface Profile {
  user: { id: string; handle: string; avatar: string | null };
  toys: WallCard[];
  /** Present ONLY when the viewer is this profile's owner. */
  drafts?: DraftInfo[];
}

export type WallSort = "recent" | "popular";

/** Where the sign-in button points. A full-page navigation (302 → Discord),
 *  never a fetch. */
export const SIGN_IN_URL = "/api/auth/discord";

/** Full-page navigation into the Discord OAuth flow. A function (not a bare
 *  location.assign at the call site) so components stay testable — jsdom
 *  forbids stubbing window.location. */
export function goToSignIn(): void {
  window.location.assign(SIGN_IN_URL);
}

/** A non-2xx response. `status` lets callers branch (404 toy gone, 409
 *  revision/quota conflict, 429 update throttle) without parsing the message. */
export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

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
    throw new ApiError(`${method} ${url} → ${res.status}`, res.status);
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

export function getWall(sort: WallSort, page: number, query = ""): Promise<WallPage> {
  return request<WallPage>(
    `/api/toys?sort=${sort}&page=${page}&q=${encodeURIComponent(query.trim())}`,
  );
}

export function getHighlights(): Promise<{ toys: WallCard[] }> {
  return request<{ toys: WallCard[] }>("/api/highlights");
}

export function getToy(id: string): Promise<ToyFull> {
  return request<ToyFull>(`/api/toys/${id}`);
}

export function getStarterTemplate(): Promise<StarterTemplate> {
  return request<StarterTemplate>("/api/starter");
}

export function updateStarterTemplate(template: StarterTemplate): Promise<void> {
  return request<void>("/api/starter", {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(template),
  });
}

export function getAdminOverview(): Promise<AdminOverview> {
  return request<AdminOverview>("/api/admin");
}

export function getFeaturedToy(): Promise<{ id: string | null }> {
  return request<{ id: string | null }>("/api/featured");
}

export function setFeaturedToy(toy_id: string | null): Promise<void> {
  return request<void>("/api/admin/featured", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ toy_id }),
  });
}

export function setFeaturedToys(toy_ids: string[]): Promise<void> {
  return request<void>("/api/admin/featured-toys", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ toy_ids }),
  });
}

export function adminDeleteToy(id: string): Promise<void> {
  return request<void>(`/api/admin/toys/${id}`, { method: "DELETE" });
}

export function adminBanUser(discord_id: string): Promise<void> {
  return request<void>("/api/admin/ban", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ discord_id }),
  });
}

export function adminUnbanUser(id: string): Promise<void> {
  return request<void>(`/api/admin/ban/${id}`, { method: "DELETE" });
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

export function getTokens(): Promise<ApiToken[]> {
  return request<ApiToken[]>("/api/tokens");
}

export function createToken(name = "CLI"): Promise<ApiToken & { token: string }> {
  return request<ApiToken & { token: string }>("/api/tokens", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name }),
  });
}

export function deleteToken(id: string): Promise<void> {
  return request<void>(`/api/tokens/${id}`, { method: "DELETE" });
}

export interface SaveToyBody {
  title: string;
  description?: string;
  files: ToyFile[];
  sources: ToySource[];
  /** Create only: the published toy this one is a remix of. */
  forkedFrom?: string;
}

export function createToy(body: SaveToyBody): Promise<{ id: string; revision: number }> {
  return request<{ id: string; revision: number }>("/api/toys", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function updateToy(
  id: string,
  expectedRevision: number,
  body: SaveToyBody,
): Promise<{ revision: number }> {
  return request<{ revision: number }>(`/api/toys/${id}`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ ...body, expectedRevision }),
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

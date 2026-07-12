import { useSyncExternalStore } from "react";
import { getMe, logout, type Me } from "./apiClient";

export interface SessionState {
  user: Me | null;
  /** true until the first /api/me resolves — lets the nav avoid flashing
   *  "Sign in" before we know whether there's a session. */
  loading: boolean;
}

let state: SessionState = { user: null, loading: true };
const listeners = new Set<() => void>();

function set(next: SessionState) {
  state = next;
  for (const l of listeners) l();
}

export const sessionStore = {
  get: (): SessionState => state,
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },
  /** Load (or reload) the current user from the server. */
  async refresh(): Promise<void> {
    const user = await getMe();
    set({ user, loading: false });
  },
  /** Delete the server session, then reflect the signed-out state. */
  async signOut(): Promise<void> {
    await logout();
    await sessionStore.refresh();
  },
  _resetForTests(): void {
    set({ user: null, loading: true });
  },
};

export function useSession(): SessionState {
  return useSyncExternalStore(sessionStore.subscribe, sessionStore.get);
}

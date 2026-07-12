/** Binds a server draft id to the open-sketch `session` so it self-clears
 *  when the user switches workspace (a new session bumps openSketchStore's
 *  session counter — see openSketch.ts). A save after that switch mints a
 *  fresh draft instead of clobbering the previous toy. */

import { useSyncExternalStore } from "react";

let bound: { id: string; session: number } | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export const cloudDraft = {
  /** The bound draft id, or null if unbound or bound to a stale session. */
  current(session: number): string | null {
    return bound && bound.session === session ? bound.id : null;
  },

  /** Bind `id` to `session`. */
  set(id: string, session: number): void {
    bound = { id, session };
    emit();
  },

  /** Clear the binding. */
  clear(): void {
    if (bound) {
      bound = null;
      emit();
    }
  },

  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },

  /** Test hook: back to the boot state. */
  _resetForTests(): void {
    bound = null;
    listeners.clear();
  },
};

export function useCloudDraft(session: number): string | null {
  return useSyncExternalStore(cloudDraft.subscribe, () => cloudDraft.current(session));
}

import { useSyncExternalStore } from "react";

const STORAGE_KEY = "ppu.editor.vim";

/** Normalize an untrusted stored value. Plain keybindings are the default —
 *  vim is strictly opt-in. */
export function parseVim(raw: unknown): boolean {
  return raw === "1";
}

function load(): boolean {
  try {
    return parseVim(localStorage.getItem(STORAGE_KEY));
  } catch {
    return false; // storage unavailable (private mode / node)
  }
}

let vimOn = load();
const listeners = new Set<() => void>();

function set(next: boolean) {
  vimOn = next;
  try {
    localStorage.setItem(STORAGE_KEY, next ? "1" : "0");
  } catch {
    /* non-persistent is fine */
  }
  for (const l of listeners) l();
}

/** Editor input-mode setting: persisted, shared by every consumer (the tab-bar
 *  chip, the settings panel, and the editor itself). */
export const editorSettings = {
  vim: (): boolean => vimOn,
  setVim: (on: boolean): void => set(on),
  toggleVim: (): void => set(!vimOn),
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },
  _resetForTests(): void {
    vimOn = false;
  },
};

export function useVimMode(): boolean {
  return useSyncExternalStore(editorSettings.subscribe, editorSettings.vim);
}

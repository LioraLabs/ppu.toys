import { useEffect, useSyncExternalStore } from "react";

export type Theme = "dark" | "light";

const STORAGE_KEY = "ppu.theme";

/** Normalize an untrusted stored value to a Theme. Dark is the default. */
export function parseTheme(raw: unknown): Theme {
  return raw === "light" ? "light" : "dark";
}

export function nextTheme(t: Theme): Theme {
  return t === "dark" ? "light" : "dark";
}

function loadTheme(): Theme {
  try {
    return parseTheme(localStorage.getItem(STORAGE_KEY));
  } catch {
    return "dark"; // storage unavailable (private mode / node)
  }
}

/** Shared theme store: owns the `data-theme` attribute on <html> (tokens.css
 *  keys the light palette off [data-theme="light"]) and persists the choice.
 *  Module-level state so every consumer (Toolbar, SettingsPanel) stays in
 *  sync — the old per-hook local state desyncs with a second consumer. */
let theme: Theme = loadTheme();
const listeners = new Set<() => void>();

function apply(t: Theme) {
  if (typeof document !== "undefined") document.documentElement.dataset.theme = t;
  try {
    localStorage.setItem(STORAGE_KEY, t);
  } catch {
    /* non-persistent is fine */
  }
}

export const themeStore = {
  get: (): Theme => theme,
  toggle(): void {
    theme = nextTheme(theme);
    apply(theme);
    for (const l of listeners) l();
  },
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },
};

export function useTheme() {
  const t = useSyncExternalStore(themeStore.subscribe, themeStore.get);
  // (re)assert the attribute on mount — the first consumer applies the
  // persisted choice, later consumers are no-ops.
  useEffect(() => {
    apply(themeStore.get());
  }, []);
  return { theme: t, toggleTheme: themeStore.toggle };
}

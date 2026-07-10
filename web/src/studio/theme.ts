import { useLayoutEffect, useState } from "react";

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

/** Theme state: owns the `data-theme` attribute on <html> (tokens.css keys the
 *  light palette off [data-theme="light"]) and persists the choice. Holds local
 *  state and is intended for a single consumer (the Toolbar) — if a second
 *  consumer ever needs it, lift to a shared store. */
export function useTheme() {
  const [theme, setTheme] = useState<Theme>(loadTheme);
  useLayoutEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      /* non-persistent is fine */
    }
  }, [theme]);
  return { theme, toggleTheme: () => setTheme((t) => nextTheme(t)) };
}

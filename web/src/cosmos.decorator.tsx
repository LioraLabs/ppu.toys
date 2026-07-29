import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import "./styles/tokens.css";
import { worker } from "./mocks/browser";
import { parseTheme } from "./studio/theme";

const workerReady = worker.start({ onUnhandledRequest: "bypass", quiet: true }).catch((error) => {
  console.error(error);
});

export default function CosmosRoot({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let live = true;
    void workerReady.then(() => live && setReady(true));
    return () => {
      live = false;
    };
  }, []);

  // Deterministic theme for fixtures/screenshots: same source of truth as the
  // app's useTheme (localStorage, dark default) so a late-mounting ToolbarWired
  // can never flip the theme mid-shot — shoot seeds ppu.theme before load.
  useEffect(() => {
    try {
      document.documentElement.dataset.theme = parseTheme(localStorage.getItem("ppu.theme"));
    } catch {
      document.documentElement.dataset.theme = "dark";
    }
  }, []);

  useEffect(() => {
    if (ready) document.body.dataset.cosmosReady = "true";
    return () => {
      delete document.body.dataset.cosmosReady;
    };
  }, [ready]);

  return ready ? <>{children}</> : null;
}

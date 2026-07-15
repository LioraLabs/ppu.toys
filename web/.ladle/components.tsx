import { useEffect } from "react";
import type { GlobalProvider } from "@ladle/react";
import "../src/styles/tokens.css";
import { worker } from "../src/mocks/browser";

// Start the MSW browser worker once so page/wired stories that fetch (e.g.
// against apiClient) resolve against the shared handlers. Module-level so it
// only ever runs a single time, regardless of how many stories mount.
void worker.start({ onUnhandledRequest: "bypass", quiet: true });

// Global story wrapper — this is the `withTheme` seam. It reuses Ladle's
// built-in toolbar theme addon (globalState.theme) instead of adding any
// custom toolbar UI.
export const Provider: GlobalProvider = ({ children, globalState }) => {
  useEffect(() => {
    // globalState.theme can also be "auto" (Ladle's toolbar default); tokens.css
    // has no auto handling, so anything but "light" folds into the dark default.
    document.documentElement.dataset.theme = globalState.theme === "light" ? "light" : "dark";
  }, [globalState.theme]);

  return <>{children}</>;
};

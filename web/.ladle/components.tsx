import { useEffect } from "react";
import type { GlobalProvider } from "@ladle/react";
import "../src/styles/tokens.css";

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

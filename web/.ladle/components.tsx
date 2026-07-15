import { useEffect, useState } from "react";
import type { GlobalProvider } from "@ladle/react";
import "../src/styles/tokens.css";
import { worker } from "../src/mocks/browser";

// Start the MSW browser worker once so page/wired stories that fetch (e.g.
// against apiClient) resolve against the shared handlers. Module-level so it
// only ever runs a single time, regardless of how many stories mount. The
// promise is awaited by the Provider below before any story renders, so a
// page story's fetch can never race the service worker's activation.
const workerReady = worker
  .start({ onUnhandledRequest: "bypass", quiet: true })
  .catch((err) => {
    console.error(err);
  });

// Global story wrapper — this is the `withTheme` seam. It reuses Ladle's
// built-in toolbar theme addon (globalState.theme) instead of adding any
// custom toolbar UI.
export const Provider: GlobalProvider = ({ children, globalState }) => {
  // Hold every story until the MSW service worker is active and controlling the
  // page. Without this gate the first story mounts and fetches before the SW
  // has claimed the client, so requests escape the mock and page stories render
  // empty / errored in the built catalog (and under `shoot`).
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let live = true;
    void workerReady.then(() => live && setReady(true));
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    // globalState.theme can also be "auto" (Ladle's toolbar default); tokens.css
    // has no auto handling, so anything but "light" folds into the dark default.
    document.documentElement.dataset.theme = globalState.theme === "light" ? "light" : "dark";
  }, [globalState.theme]);

  if (!ready) return null;

  return <>{children}</>;
};

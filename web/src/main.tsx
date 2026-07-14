import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { initCore } from "./ppu/instance";

const root = document.getElementById("root")!;

// Load the WASM PPU core BEFORE importing the app: the studio's transport
// singleton renders its first frame against the core at module-eval time, so the
// core must be live before that graph is imported. App is imported dynamically
// for exactly that reason. There is no mock fallback — if the core can't load,
// there is no tool, so we show a hard error rather than a fake PPU.
initCore()
  .then(async () => {
    const { default: App } = await import("./App");
    createRoot(root).render(
      <StrictMode>
        <App />
      </StrictMode>
    );
  })
  .catch((err) => {
    console.error("Failed to load the PPU core:", err);
    root.innerHTML =
      '<div style="max-width:34rem;margin:15vh auto;padding:0 1.5rem;font:15px/1.6 system-ui,sans-serif;color:#ddd">' +
      '<h1 style="font-size:1.25rem;margin:0 0 .5rem">The PPU core failed to load</h1>' +
      "<p>ppu.toys runs entirely on a WebAssembly build of the SNES PPU. Your browser " +
      "couldn't load it, so there's nothing to render. Try a hard refresh, and make sure " +
      "WebAssembly isn't blocked by an extension or a restrictive network.</p>" +
      "</div>";
  });

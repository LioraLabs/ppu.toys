import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { bootstrapCore } from "./ppu/instance";

// Select the core (real WASM when VITE_USE_WASM is set, mock otherwise) before
// the first render so the Studio mounts against the chosen core.
bootstrapCore().then(() => {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>
  );
});

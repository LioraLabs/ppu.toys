/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { shouldBypassApiProxy } from "./src/viteProxy";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8080",
        // Cosmos roots Vite at src, making src/api/apiClient.ts available at
        // /api/apiClient.ts. Let Vite serve source modules under this otherwise
        // backend-owned prefix.
        bypass: (req) =>
          req.url && shouldBypassApiProxy(req.url) ? req.url : undefined,
      },
      "/blobs": "http://127.0.0.1:8080",
    },
  },
  test: {
    environment: "node",
    // Installs a stub PpuCore before any test imports the transport singleton
    // (see src/test/setup.ts) — real wasm can't init under node/jsdom.
    setupFiles: ["./src/test/setup.ts"],
  },
});

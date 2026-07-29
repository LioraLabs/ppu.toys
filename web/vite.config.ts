/// <reference types="vitest" />
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { shouldBypassApiProxy } from "./src/viteProxy";

export default defineConfig({
  plugins: [react()],
  // Cosmos changes Vite's root to src. Keep the MSW worker on the renderer's
  // origin instead of letting /mockServiceWorker.js fall through to index.html.
  publicDir: fileURLToPath(new URL("public", import.meta.url)),
  server: {
    proxy: (() => {
      // Overridable for the demo-seeding runbook (web/docs/seeding.md):
      // PPU_API_TARGET points the dev proxy at a real deployment and
      // PPU_API_COOKIE (e.g. "ppu_sess=<sid>") authenticates every proxied
      // request as the minted system account — the browser never sees the
      // credential.
      const target = process.env.PPU_API_TARGET ?? "http://127.0.0.1:8080";
      const cookie = process.env.PPU_API_COOKIE;
      const withAuth = {
        target,
        changeOrigin: true,
        configure: cookie
          ? (proxy: { on: (ev: string, cb: (proxyReq: { setHeader: (k: string, v: string) => void }) => void) => void }) => {
              proxy.on("proxyReq", (proxyReq) => proxyReq.setHeader("cookie", cookie));
            }
          : undefined,
      };
      return {
        "/api": {
          ...withAuth,
          // Cosmos roots Vite at src, making src/api/apiClient.ts available at
          // /api/apiClient.ts. Let Vite serve source modules under this otherwise
          // backend-owned prefix.
          bypass: (req: { url?: string }) =>
            req.url && shouldBypassApiProxy(req.url) ? req.url : undefined,
        },
        "/blobs": withAuth,
      };
    })(),
  },
  test: {
    environment: "node",
    // Installs a stub PpuCore before any test imports the transport singleton
    // (see src/test/setup.ts) — real wasm can't init under node/jsdom.
    setupFiles: ["./src/test/setup.ts"],
  },
});

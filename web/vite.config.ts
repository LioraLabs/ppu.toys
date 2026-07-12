/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/blobs": "http://127.0.0.1:8080",
    },
  },
  test: {
    environment: "node",
  },
});

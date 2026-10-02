import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// In development the API server runs on :8080; every server-owned path is proxied.
const backend = process.env.LECTERN_BACKEND ?? "http://localhost:8080";
const proxied = ["/api", "/auth", "/theme.css", "/server.css", "/branding", "/verify", "/ob"];

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: Object.fromEntries(proxied.map((p) => [p, { target: backend, changeOrigin: false }])),
  },
  build: {
    sourcemap: false,
    // No inline scripts or styles: the CSP forbids them.
    assetsInlineLimit: 0,
    // hls.js is loaded on demand, only for adaptive streams.
    chunkSizeWarningLimit: 700,
  },
  test: {
    environment: "jsdom",
  },
});

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// The SPA is served by the desktop backend (dccd-http) under the /m/ prefix.
// We build with `base: "/m/"` so asset URLs in the produced index.html are
// absolute under that path, and we proxy /api + /auth + /health in dev so the
// dev server can talk to a locally-running dccd-http on :9876.
export default defineConfig({
  base: "/m/",
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  server: {
    port: 5174,
    proxy: {
      "/api": "http://127.0.0.1:9876",
      "/auth": "http://127.0.0.1:9876",
      "/health": "http://127.0.0.1:9876",
      "/rpc": "http://127.0.0.1:9876",
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});

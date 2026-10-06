import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vitest/config";

// Frontend lives in ui/; Tauri loads dist/ (see src-tauri/tauri.conf.json).
// `tauri android dev` and `tauri ios dev` set TAURI_DEV_HOST to the address a
// phone can reach this machine on.
const devHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  root: "ui",
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./ui/src", import.meta.url)) },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: devHost || false,
    hmr: devHost ? { protocol: "ws", host: devHost, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  build: { outDir: "../dist", emptyOutDir: true, target: "safari16" },
  test: { environment: "jsdom", include: ["src/**/*.test.{ts,tsx}"] },
});

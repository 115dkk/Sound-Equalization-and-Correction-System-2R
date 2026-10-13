import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tauri serves the built files from ui/dist; the dev server must sit on the
// port tauri.conf.json's devUrl names.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});

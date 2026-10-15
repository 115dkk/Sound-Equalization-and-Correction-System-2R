import { resolve } from "node:path";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tauri serves the built files from ui/dist; the dev server must sit on the
// port tauri.conf.json's devUrl names.
//
// Two entries: index.html (the app) and gallery.html (the screen gallery,
// docs/DESIGN.md §7). `vite build` builds both; `vite build --mode app`
// (npm run build, which Tauri runs) builds the app alone so the gallery and
// its fixtures never ship in the installer.
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    rollupOptions: {
      input:
        mode === "app"
          ? { main: resolve(import.meta.dirname, "index.html") }
          : { main: resolve(import.meta.dirname, "index.html"), gallery: resolve(import.meta.dirname, "gallery.html") },
      // The libraries and the two catalogs go in their own chunks so no chunk
      // passes the 500 kB warning. All are static imports: the app still
      // loads at once, with nothing lazy to wait for.
      output: {
        codeSplitting: {
          groups: [
            { name: "vendor", test: /node_modules/ },
            { name: "catalogs", test: /src[\\/]i18n[\\/][a-z]+\.json$/ },
          ],
        },
      },
    },
  },
  test: {
    // DOM tests opt in with a `@vitest-environment happy-dom` docblock.
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    setupFiles: ["src/test/setup.ts"],
  },
}));

import { defineConfig } from "vite";
import ikaThemes from "./vite-plugin-ika-themes.js";
import fs from "node:fs";

// the launcher's version, as Tauri builds it
const IKA_VERSION = JSON.parse(fs.readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf8")).version;

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  // Evita que Vite limpie la consola y oculte errores de Rust/Tauri
  clearScreen: false,
  // themes generated from src/theme/palette.js (see the plugin)
  plugins: [ikaThemes()],
  define: { __IKA_VERSION__: JSON.stringify(IKA_VERSION) },
  // .env.local variables named IKA_* reach the frontend as import.meta.env.IKA_* (the CurseForge
  // API key); .env.local itself is never committed
  envPrefix: ["VITE_", "IKA_"],
  server: {
    // Puerto fijo para sincronizar con tauri.conf.json
    port: 5173,
    strictPort: true,
    // En Windows, usar 127.0.0.1 evita problemas de resolución IPv6 (::1) con localhost en WebView2
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 5174,
        }
      : undefined,
    watch: {
      // Ignorar cambios en src-tauri para evitar recargas innecesarias
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        console: "console.html",
      },
    },
  },
}));

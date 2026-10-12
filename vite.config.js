import { defineConfig } from "vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  // Evita que Vite limpie la consola y oculte errores de Rust/Tauri
  clearScreen: false,
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
}));

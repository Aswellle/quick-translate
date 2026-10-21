import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development and production
  clearScreen: false,

  server: {
    port: 5173,
    strictPort: true,
    // 钉在 IPv4：Node ≥17 对 `localhost` 的 DNS 解析常落在 ::1，Vite 便只绑
    // IPv6，而 tauri CLI 探测 devUrl（localhost:5173）时走 127.0.0.1 ——
    // 探测 180s 连不上，`npm run tauri dev` 直接失败。显式 host 消除歧义。
    host: "127.0.0.1",
    watch: {
      // Watch for changes in the Tauri source
      ignored: ["**/src-tauri/**"],
    },
  },

  envPrefix: ["VITE_", "TAURI_ENV_*"],

  build: {
    // Tauri supports es2021
    target:
      process.env.TAURI_ENV_PLATFORM == "windows"
        ? "chrome105"
        : "safari13",
    // Don't minify for debug builds
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    // Produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
}));

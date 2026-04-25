import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const isDemoMode = mode === "demo";
  const pagesBase = env.VITE_PAGES_BASE || "/lessAI/";

  return {
    plugins: [react()],
    server: {
      host: "0.0.0.0",
      port: 1420,
      strictPort: true
    },
    envPrefix: ["VITE_", "TAURI_"],
    clearScreen: false,
    base: isDemoMode ? pagesBase : "/"
  };
});

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (/node_modules\/(react|react-dom|scheduler)\//.test(id)) return "react";
          if (/node_modules\/(framer-motion|motion-dom|motion-utils)\//.test(id)) return "motion";
          if (id.includes("node_modules/lucide-react/")) return "icons";
          if (id.includes("node_modules/@tauri-apps/")) return "tauri";
          if (/node_modules\/(i18next|react-i18next)\//.test(id)) return "i18n";
          if (id.includes("node_modules/better-auth/")) return "auth";
          if (id.includes("node_modules/zustand/")) return "state";
          return "vendor";
        },
      },
    },
  },
}));

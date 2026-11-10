import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");
  const apiBaseUrl = (env.SAFI_API_BASE_URL ?? "").trim();

  return {
    plugins: [react()],
    define: {
      __SAFI_API_BASE_URL__: JSON.stringify(apiBaseUrl)
    },
    server: {
      host: true,
      port: 1420,
      strictPort: false,
      proxy: {
        "/v1": {
          target: "http://127.0.0.1:8080",
          changeOrigin: false
        }
      }
    },
    build: {
      target: "es2023",
      sourcemap: false
    }
  };
});

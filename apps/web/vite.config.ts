import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    host: true,
    port: 1420,
    strictPort: false
  },
  build: {
    target: "es2023",
    sourcemap: false
  }
});

import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

const RESERVED_RELEASE_HOSTS = new Set([
  "invalid",
  "test",
  "example",
  "local",
  "internal",
  "example.com",
  "example.org",
  "example.net"
]);

function isReservedReleaseHost(host: string): boolean {
  return (
    RESERVED_RELEASE_HOSTS.has(host) ||
    host.endsWith(".invalid") ||
    host.endsWith(".test") ||
    host.endsWith(".example") ||
    host.endsWith(".local") ||
    host.endsWith(".internal") ||
    host.endsWith(".arpa") ||
    host.endsWith(".example.com") ||
    host.endsWith(".example.org") ||
    host.endsWith(".example.net")
  );
}

function validateReleaseApiOrigin(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) {
    throw new Error("SAFI_API_BASE_URL is required when SAFI_DESKTOP_RELEASE_BUILD=1");
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error(`SAFI_API_BASE_URL is invalid: ${trimmed}`);
  }
  if (
    parsed.protocol !== "https:" ||
    parsed.username ||
    parsed.password ||
    parsed.pathname !== "/" ||
    parsed.search ||
    parsed.hash
  ) {
    throw new Error("SAFI_API_BASE_URL must be an https:// origin without path/query/credentials");
  }
  const host = parsed.hostname.toLowerCase();
  if (
    host === "localhost" ||
    host.endsWith(".localhost") ||
    /^[\d.]+$/.test(host) ||
    !host.includes(".") ||
    isReservedReleaseHost(host)
  ) {
    throw new Error(`SAFI_API_BASE_URL host is disallowed for release desktop builds: ${host}`);
  }
  return parsed.origin;
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");
  const rawApiBaseUrl = (process.env.SAFI_API_BASE_URL ?? env.SAFI_API_BASE_URL ?? "").trim();
  const apiBaseUrl =
    process.env.SAFI_DESKTOP_RELEASE_BUILD === "1"
      ? validateReleaseApiOrigin(rawApiBaseUrl)
      : rawApiBaseUrl;

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
          target: process.env.SAFI_DEV_API_PROXY ?? "http://127.0.0.1:8080",
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

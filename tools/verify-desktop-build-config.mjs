#!/usr/bin/env node
/**
 * Verifies desktop build origin parity and release fail-closed enforcement:
 *  1. Frontend release builds reject missing or reserved/placeholder SAFI_API_BASE_URL (e.g. `.invalid`).
 *  2. Tauri native release builds reject missing or reserved/placeholder SAFI_API_BASE_URL.
 *  3. Building @safi/web + safi-desktop (--release) with the shared CI verification origin embeds
 *     the exact same normalized origin in both the built JS assets and the compiled native binary.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const ROOT = process.cwd();
const WEB_DIST_ASSETS = path.join(ROOT, "apps", "web", "dist", "assets");
const TAURI_MANIFEST = path.join(ROOT, "apps", "desktop", "src-tauri", "Cargo.toml");
const RELEASE_BIN = path.join(
  ROOT,
  "apps",
  "desktop",
  "src-tauri",
  "target",
  "release",
  process.platform === "win32" ? "safi-desktop.exe" : "safi-desktop"
);

const DEFAULT_CI_VERIFY_ORIGIN = "https://ci-verify.safidata.dev";
const targetOrigin = (process.env.SAFI_API_BASE_URL ?? DEFAULT_CI_VERIFY_ORIGIN).trim();

function runStep(cmd, args, extraEnv, allowFailure = false) {
  const env = { ...process.env };
  for (const [k, v] of Object.entries(extraEnv)) {
    if (v === undefined) {
      delete env[k];
    } else {
      env[k] = v;
    }
  }
  const useShell = process.platform === "win32" && (cmd === "pnpm" || cmd.endsWith(".cmd"));
  const res = spawnSync(cmd, args, {
    cwd: ROOT,
    env,
    shell: useShell,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"]
  });
  if (!allowFailure && res.status !== 0) {
    throw new Error(
      `Command failed (${cmd} ${args.join(" ")}) with status ${res.status}:\n${res.stdout}\n${res.stderr}`
    );
  }
  return res;
}

console.log("[desktop-build-verify] Step 1: Reject missing/reserved SAFI_API_BASE_URL in Web release build");
const webMissing = runStep(
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  { SAFI_DESKTOP_RELEASE_BUILD: "1", SAFI_API_BASE_URL: "" },
  true
);
assert.notEqual(webMissing.status, 0, "Expected Web release build without SAFI_API_BASE_URL to fail");

const webInvalid = runStep(
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  { SAFI_DESKTOP_RELEASE_BUILD: "1", SAFI_API_BASE_URL: "https://api.safidata.invalid" },
  true
);
assert.notEqual(webInvalid.status, 0, "Expected Web release build with .invalid to fail");

console.log("[desktop-build-verify] Step 2: Build @safi/web with SAFI_API_BASE_URL=" + targetOrigin);
runStep("pnpm", ["-r", "--filter", "@safi/web", "run", "build"], {
  SAFI_DESKTOP_RELEASE_BUILD: "1",
  SAFI_API_BASE_URL: targetOrigin
});

const jsFiles = fs.readdirSync(WEB_DIST_ASSETS).filter((f) => f.endsWith(".js"));
assert(jsFiles.length > 0, "Expected compiled JS bundles in apps/web/dist/assets");
const frontendMatches = jsFiles.filter((file) =>
  fs.readFileSync(path.join(WEB_DIST_ASSETS, file), "utf8").includes(targetOrigin)
);
assert(
  frontendMatches.length > 0,
  `Expected compiled frontend JS in ${WEB_DIST_ASSETS} to contain ${targetOrigin}`
);
console.log(
  `[desktop-build-verify] Confirmed frontend bundle embeds ${targetOrigin} in: ${frontendMatches.join(", ")}`
);

console.log("[desktop-build-verify] Step 3: Reject missing/reserved SAFI_API_BASE_URL in Tauri Release build");
const cargoMissing = runStep(
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: undefined },
  true
);
assert.notEqual(cargoMissing.status, 0, "Expected Release build without SAFI_API_BASE_URL to fail");

const cargoInvalid = runStep(
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: "https://api.safidata.invalid" },
  true
);
assert.notEqual(cargoInvalid.status, 0, "Expected Release build with .invalid to fail");

console.log("[desktop-build-verify] Step 4: Compile Tauri Release binary with shared SAFI_API_BASE_URL");
runStep("cargo", ["build", "--manifest-path", TAURI_MANIFEST, "--release"], {
  SAFI_API_BASE_URL: targetOrigin
});
assert(fs.existsSync(RELEASE_BIN), `Expected compiled release binary at ${RELEASE_BIN}`);

const probe = runStep(RELEASE_BIN, ["--print-build-origin"], {});
const parsed = JSON.parse(probe.stdout.trim());
assert.equal(
  parsed.apiOrigin,
  targetOrigin,
  `Expected native release binary origin (${parsed.apiOrigin}) to match frontend/build origin (${targetOrigin})`
);

console.log(
  `[desktop-build-verify] PASS: Frontend dist AND Native Release binary share identical origin ${parsed.apiOrigin}`
);

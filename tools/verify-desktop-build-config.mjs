#!/usr/bin/env node
/**
 * Verifies desktop build origin separation, parity, and release fail-closed enforcement:
 *  1. Frontend release builds reject missing, reserved/placeholder, or non-HTTPS SAFI_API_BASE_URL
 *     AND SAFI_PUBLIC_WEB_ORIGIN independently.
 *  2. Tauri native release builds reject missing, reserved/placeholder, or non-HTTPS SAFI_API_BASE_URL
 *     AND SAFI_PUBLIC_WEB_ORIGIN independently.
 *  3. Building @safi/web + safi-desktop (--release) with distinct API and Public Web origins embeds
 *     the exact distinct normalized origins in both the built JS assets and the compiled native binary,
 *     and changing one origin never alters or breaks the other.
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

const DEFAULT_CI_API_ORIGIN = "https://ci-verify.safidata.dev";
const DEFAULT_CI_WEB_ORIGIN = "https://ci-web.safidata.dev";
const ALTERNATE_CI_WEB_ORIGIN = "https://portal-verify.safidata.dev";
const targetApiOrigin = (process.env.SAFI_API_BASE_URL ?? DEFAULT_CI_API_ORIGIN).trim();
const targetWebOrigin = (process.env.SAFI_PUBLIC_WEB_ORIGIN ?? DEFAULT_CI_WEB_ORIGIN).trim();
assert.notEqual(
  targetApiOrigin,
  targetWebOrigin,
  "SAFI_API_BASE_URL and SAFI_PUBLIC_WEB_ORIGIN must be tested with distinct origins"
);

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

function expectFailureWithDiagnostic(stepName, cmd, args, extraEnv, expectedDiagnostics) {
  const res = runStep(cmd, args, extraEnv, true);
  assert.notEqual(res.status, 0, `${stepName}: expected non-zero exit code`);
  const combined = `${res.stdout ?? ""}\n${res.stderr ?? ""}`;
  for (const snippet of expectedDiagnostics) {
    assert(
      combined.includes(snippet),
      `${stepName}: expected diagnostic substring ${JSON.stringify(snippet)} in output, got:\n${combined}`
    );
  }
  console.log(
    `[desktop-build-verify] Confirmed expected rejection (${stepName}, exit=${res.status}): ${expectedDiagnostics[expectedDiagnostics.length - 1]}`
  );
}

console.log(
  "[desktop-build-verify] Step 1: Reject missing, reserved, and non-HTTPS SAFI_API_BASE_URL & SAFI_PUBLIC_WEB_ORIGIN in Web release build"
);
expectFailureWithDiagnostic(
  "web-release-missing-api-origin",
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  { SAFI_DESKTOP_RELEASE_BUILD: "1", SAFI_API_BASE_URL: "", SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin },
  ["SAFI_API_BASE_URL is required when SAFI_DESKTOP_RELEASE_BUILD=1"]
);

expectFailureWithDiagnostic(
  "web-release-missing-web-origin",
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  { SAFI_DESKTOP_RELEASE_BUILD: "1", SAFI_API_BASE_URL: targetApiOrigin, SAFI_PUBLIC_WEB_ORIGIN: "" },
  ["SAFI_PUBLIC_WEB_ORIGIN is required when SAFI_DESKTOP_RELEASE_BUILD=1"]
);

expectFailureWithDiagnostic(
  "web-release-invalid-api-domain",
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  {
    SAFI_DESKTOP_RELEASE_BUILD: "1",
    SAFI_API_BASE_URL: "https://api.safidata.invalid",
    SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin
  },
  ["SAFI_API_BASE_URL host is disallowed for release desktop builds: api.safidata.invalid"]
);

expectFailureWithDiagnostic(
  "web-release-invalid-web-domain",
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  {
    SAFI_DESKTOP_RELEASE_BUILD: "1",
    SAFI_API_BASE_URL: targetApiOrigin,
    SAFI_PUBLIC_WEB_ORIGIN: "https://web.safidata.invalid"
  },
  ["SAFI_PUBLIC_WEB_ORIGIN host is disallowed for release desktop builds: web.safidata.invalid"]
);

expectFailureWithDiagnostic(
  "web-release-non-https-api",
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  {
    SAFI_DESKTOP_RELEASE_BUILD: "1",
    SAFI_API_BASE_URL: "http://api.safidata.dev",
    SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin
  },
  ["SAFI_API_BASE_URL must be an https:// origin without path/query/credentials"]
);

expectFailureWithDiagnostic(
  "web-release-non-https-web",
  "pnpm",
  ["-r", "--filter", "@safi/web", "run", "build"],
  {
    SAFI_DESKTOP_RELEASE_BUILD: "1",
    SAFI_API_BASE_URL: targetApiOrigin,
    SAFI_PUBLIC_WEB_ORIGIN: "http://safidata.dev"
  },
  ["SAFI_PUBLIC_WEB_ORIGIN must be an https:// origin without path/query/credentials"]
);

console.log(
  `[desktop-build-verify] Step 2: Build @safi/web with distinct origins (API=${targetApiOrigin}, WEB=${targetWebOrigin})`
);
runStep("pnpm", ["-r", "--filter", "@safi/web", "run", "build"], {
  SAFI_DESKTOP_RELEASE_BUILD: "1",
  SAFI_API_BASE_URL: targetApiOrigin,
  SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin
});

const jsFiles = fs.readdirSync(WEB_DIST_ASSETS).filter((f) => f.endsWith(".js"));
assert(jsFiles.length > 0, "Expected compiled JS bundles in apps/web/dist/assets");
const frontendApiMatches = jsFiles.filter((file) =>
  fs.readFileSync(path.join(WEB_DIST_ASSETS, file), "utf8").includes(targetApiOrigin)
);
const frontendWebMatches = jsFiles.filter((file) =>
  fs.readFileSync(path.join(WEB_DIST_ASSETS, file), "utf8").includes(targetWebOrigin)
);
assert(
  frontendApiMatches.length > 0,
  `Expected compiled frontend JS in ${WEB_DIST_ASSETS} to contain ${targetApiOrigin}`
);
assert(
  frontendWebMatches.length > 0,
  `Expected compiled frontend JS in ${WEB_DIST_ASSETS} to contain ${targetWebOrigin}`
);
console.log(
  `[desktop-build-verify] Confirmed frontend bundle embeds distinct API (${targetApiOrigin}) and Web (${targetWebOrigin}) origins`
);

console.log(
  "[desktop-build-verify] Step 3: Reject missing, reserved, and non-HTTPS SAFI_API_BASE_URL & SAFI_PUBLIC_WEB_ORIGIN in Tauri Release build"
);
expectFailureWithDiagnostic(
  "native-release-missing-api-origin",
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: undefined, SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin },
  ["SAFI_API_BASE_URL is required for release desktop builds"]
);

expectFailureWithDiagnostic(
  "native-release-missing-web-origin",
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: targetApiOrigin, SAFI_PUBLIC_WEB_ORIGIN: undefined },
  ["SAFI_PUBLIC_WEB_ORIGIN is required for release desktop builds"]
);

expectFailureWithDiagnostic(
  "native-release-invalid-api-domain",
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: "https://api.safidata.invalid", SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin },
  [
    "SAFI_API_BASE_URL is invalid for release builds: the release API origin must not use a reserved placeholder domain"
  ]
);

expectFailureWithDiagnostic(
  "native-release-invalid-web-domain",
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: targetApiOrigin, SAFI_PUBLIC_WEB_ORIGIN: "https://web.safidata.invalid" },
  [
    "SAFI_PUBLIC_WEB_ORIGIN is invalid for release builds: the release API origin must not use a reserved placeholder domain"
  ]
);

expectFailureWithDiagnostic(
  "native-release-non-https-api",
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: "http://api.safidata.dev", SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin },
  ["SAFI_API_BASE_URL is invalid for release builds: the API origin must start with https://"]
);

expectFailureWithDiagnostic(
  "native-release-non-https-web",
  "cargo",
  ["build", "--manifest-path", TAURI_MANIFEST, "--release"],
  { SAFI_API_BASE_URL: targetApiOrigin, SAFI_PUBLIC_WEB_ORIGIN: "http://safidata.dev" },
  ["SAFI_PUBLIC_WEB_ORIGIN is invalid for release builds: the API origin must start with https://"]
);

console.log("[desktop-build-verify] Step 4: Compile Tauri Release binary with distinct origins and prove independence");
runStep("cargo", ["build", "--manifest-path", TAURI_MANIFEST, "--release"], {
  SAFI_API_BASE_URL: targetApiOrigin,
  SAFI_PUBLIC_WEB_ORIGIN: ALTERNATE_CI_WEB_ORIGIN
});
const altProbe = JSON.parse(runStep(RELEASE_BIN, ["--print-build-origin"], {}).stdout.trim());
assert.equal(altProbe.apiOrigin, targetApiOrigin);
assert.equal(altProbe.publicWebOrigin, ALTERNATE_CI_WEB_ORIGIN);

runStep("cargo", ["build", "--manifest-path", TAURI_MANIFEST, "--release"], {
  SAFI_API_BASE_URL: targetApiOrigin,
  SAFI_PUBLIC_WEB_ORIGIN: targetWebOrigin
});
assert(fs.existsSync(RELEASE_BIN), `Expected compiled release binary at ${RELEASE_BIN}`);

const probe = runStep(RELEASE_BIN, ["--print-build-origin"], {});
const parsed = JSON.parse(probe.stdout.trim());
assert.equal(
  parsed.apiOrigin,
  targetApiOrigin,
  `Expected native release binary apiOrigin (${parsed.apiOrigin}) to match ${targetApiOrigin}`
);
assert.equal(
  parsed.publicWebOrigin,
  targetWebOrigin,
  `Expected native release binary publicWebOrigin (${parsed.publicWebOrigin}) to match ${targetWebOrigin}`
);

console.log(
  `[desktop-build-verify] PASS: Frontend dist AND Native Release binary share distinct validated origins (api=${parsed.apiOrigin}, web=${parsed.publicWebOrigin})`
);

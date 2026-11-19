#!/usr/bin/env node
/**
 * tools/verify-published-installer-download.mjs
 *
 * Verifies a REAL published Desktop installer artifact from Release Storage / CDN:
 * - Reads `apps/web/public/downloads/release-manifest.json` (or `SAFI_RELEASE_MANIFEST_URL`).
 * - Rejects relative `/downloads/...` paths and any non-HTTPS URL (except explicit test override).
 * - Downloads the real binary over the network and verifies exact `fileSizeBytes` and `SHA-256`.
 * - Exits cleanly with an explicit `BLOCKED BY EXTERNAL SETUP` message when `available === false`
 *   so an unpublished build is NEVER falsely marked as downloadable.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const manifestUrl = process.env.SAFI_RELEASE_MANIFEST_URL?.trim() || null;
let manifest;
if (manifestUrl) {
  const res = await fetch(manifestUrl, { headers: { Accept: "application/json" } });
  assert.equal(res.ok, true, `Failed to fetch release manifest from ${manifestUrl}: HTTP ${res.status}`);
  manifest = await res.json();
} else {
  manifest = JSON.parse(
    readFileSync(resolve("apps/web/public/downloads/release-manifest.json"), "utf8")
  );
}

const win = manifest?.platforms?.windows;
if (!win || !win.available || !win.downloadUrl) {
  console.log(
    "[verify-published-installer-download] BLOCKED BY EXTERNAL SETUP: release-manifest.json honestly reports available=false (no public HTTPS CDN/Release Storage URL published yet)."
  );
  process.exit(0);
}

const allowLocalHttp = process.env.SAFI_ALLOW_LOCAL_HTTP_RELEASE === "1";
if (!win.downloadUrl.startsWith("https://") && !(allowLocalHttp && win.downloadUrl.startsWith("http://127.0.0.1:"))) {
  throw new Error(`Published installer downloadUrl must be a real HTTPS URL, got: ${win.downloadUrl}`);
}
assert.ok(typeof win.fileSizeBytes === "number" && win.fileSizeBytes > 0, "fileSizeBytes must be > 0");
assert.ok(typeof win.sha256 === "string" && /^[0-9a-f]{64}$/i.test(win.sha256), "sha256 must be 64-char hex");

const resp = await fetch(win.downloadUrl);
assert.equal(resp.ok, true, `Download failed from ${win.downloadUrl}: HTTP ${resp.status}`);
const buf = Buffer.from(await resp.arrayBuffer());
assert.equal(buf.length, win.fileSizeBytes, `Downloaded size ${buf.length} != manifest ${win.fileSizeBytes}`);
const actualSha = createHash("sha256").update(buf).digest("hex");
assert.equal(actualSha, win.sha256.toLowerCase(), `SHA-256 mismatch: ${actualSha} != ${win.sha256}`);

console.log(
  `[verify-published-installer-download] PASS: downloaded ${buf.length} bytes from ${win.downloadUrl} matching SHA-256 ${actualSha}`
);

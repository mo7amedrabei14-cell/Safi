/**
 * `apps/web/src/lib/app-meta.ts` — single source for the strings the shell, the
 * settings sheet and the exported reports must agree on.
 */

export const SAFI_APP_VERSION = "1.0.0-s5-gold";

export const SAFI_BUILD_CHANNEL = "الإصدار الذهبي المعتمد";

/** Reports carry a BLAKE3 integrity digest; Ed25519 signing happens in the Rust core. */
export const SAFI_REPORT_INTEGRITY_NOTE =
  "هذا التقرير يحمل بصمة تحقق BLAKE3. التوقيع الرقمي Ed25519 يُطبَّق داخل نواة سطح المكتب بمفتاح الترخيص المحلي، ولا تُنشئه الواجهة أبدًا.";

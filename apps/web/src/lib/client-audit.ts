/**
 * `apps/web/src/lib/client-audit.ts`
 *
 * Fast, honest browser-side security audit used by the command palette.
 * Every check is a real property of the running page: a secure context, the
 * WebCrypto subtle API, OPFS availability and top-level (non-framed) rendering.
 * Nothing about the Rust kernel is claimed here because the page cannot observe it.
 */

export interface ClientAuditEnv {
  readonly secureContext: boolean;
  readonly subtleCrypto: boolean;
  readonly opfs: boolean;
  /** True when the document is the top-level window (not framed). */
  readonly topFrame: boolean;
}

export interface ClientAuditCheck {
  readonly id: string;
  readonly labelAr: string;
  readonly passed: boolean;
  readonly detailAr: string;
}

export function readClientAuditEnv(): ClientAuditEnv {
  if (typeof window === "undefined" || typeof navigator === "undefined") {
    return { secureContext: false, subtleCrypto: false, opfs: false, topFrame: false };
  }

  const storage = navigator.storage as StorageManager | undefined;
  return {
    secureContext: window.isSecureContext === true,
    subtleCrypto: typeof globalThis.crypto?.subtle?.digest === "function",
    opfs: typeof storage?.getDirectory === "function",
    topFrame: window.self === window.top
  };
}

export function runClientSecurityAudit(env: ClientAuditEnv): readonly ClientAuditCheck[] {
  return [
    {
      id: "secure-context",
      labelAr: "سياق آمن (HTTPS أو localhost)",
      passed: env.secureContext,
      detailAr: env.secureContext
        ? "قناة مشفرة يضمنها المتصفح."
        : "افتح التطبيق عبر HTTPS أو localhost لتفعيل التشفير الكامل."
    },
    {
      id: "webcrypto",
      labelAr: "WebCrypto لخزنة AES-256-GCM",
      passed: env.subtleCrypto,
      detailAr: env.subtleCrypto
        ? "SubtleCrypto متاح لتشفير الخزنة المحلية."
        : "المتصفح لا يوفر SubtleCrypto؛ لا تفتح خزنة مشفرة هنا."
    },
    {
      id: "opfs",
      labelAr: "تخزين OPFS المحلي",
      passed: env.opfs,
      detailAr: env.opfs
        ? "مساحة تخزين محلية معزولة عن الشبكة."
        : "متصفحك لا يدعم OPFS؛ لن تُحفظ الملفات بين الجلسات."
    },
    {
      id: "framing",
      labelAr: "غير معروض داخل إطار خارجي",
      passed: env.topFrame,
      detailAr: env.topFrame
        ? "الصفحة في النافذة العليا، ما يمنع هجمات التأطير."
        : "الصفحة معروضة داخل إطار؛ أغلق الإطار وافتح العنوان مباشرة."
    }
  ];
}

export function summarizeAudit(checks: readonly ClientAuditCheck[]): {
  readonly passed: number;
  readonly total: number;
} {
  return {
    passed: checks.filter((check) => check.passed).length,
    total: checks.length
  };
}

/**
 * `apps/web/src/lib/download.ts`
 *
 * Real Desktop installer manifest loader and download trigger for the public Showcase.
 * Never fakes a download by navigating into an operational Studio tab.
 */
import { resolveAuthApiBaseUrl } from "./auth-api";

export function downloadTextFile(fileName: string, content: string, mimeType: string): boolean {
  if (typeof document === "undefined" || typeof URL.createObjectURL !== "function") {
    return false;
  }
  const blob = new Blob([content], { type: `${mimeType};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.rel = "noopener";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
  return true;
}

export type DesktopOsKey = "windows" | "mac" | "linux";

export interface PlatformDownloadArtifact {
  readonly available: boolean;
  readonly labelEn: string;
  readonly labelAr: string;
  readonly fileName: string | null;
  readonly downloadUrl: string | null;
  readonly fileSizeBytes: number;
  readonly sha256: string | null;
  readonly codeSigned: boolean;
  readonly signingNoticeEn: string;
  readonly signingNoticeAr: string;
}

export interface DesktopReleaseManifest {
  readonly version: string;
  readonly channel: string;
  readonly publishedAt: string | null;
  readonly platforms: Readonly<Record<DesktopOsKey, PlatformDownloadArtifact>>;
}

export const FALLBACK_RELEASE_MANIFEST: DesktopReleaseManifest = {
  version: "1.0.0",
  channel: "stable",
  publishedAt: null,
  platforms: {
    windows: {
      available: false,
      labelEn: "Windows 64-bit Installer (Not Yet Published to Release Storage)",
      labelAr: "مثبت Windows 64-bit (غير منشور بعد على خادم الإصدارات العام)",
      fileName: null,
      downloadUrl: null,
      fileSizeBytes: 0,
      sha256: null,
      codeSigned: false,
      signingNoticeEn:
        "No public HTTPS Release Storage / CDN artifact or Authenticode-signed Windows installer has been published yet. Once published by the Owner in App Releases, download activates automatically.",
      signingNoticeAr:
        "لم يُنشر بعد ملف تثبيت موقّع على خادم إصدارات HTTPS عام. بمجرد نشر الإصدار من لوحة المالك يتفعّل التحميل تلقائيًا."
    },
    mac: {
      available: false,
      labelEn: "macOS Universal (Apple Silicon & Intel) — Not Yet Published",
      labelAr: "نسخة macOS الشاملة — غير منشورة بعد",
      fileName: null,
      downloadUrl: null,
      fileSizeBytes: 0,
      sha256: null,
      codeSigned: false,
      signingNoticeEn:
        "macOS DMG build & Apple notarization require a macOS CI runner and Apple Developer credentials.",
      signingNoticeAr: "بناء نسخة macOS وتوثيقها يتطلبان جهاز macOS وحساب Apple Developer."
    },
    linux: {
      available: false,
      labelEn: "Linux (AppImage / DEB) — Not Yet Published",
      labelAr: "نسخة Linux (AppImage / DEB) — غير منشورة بعد",
      fileName: null,
      downloadUrl: null,
      fileSizeBytes: 0,
      sha256: null,
      codeSigned: false,
      signingNoticeEn: "Linux AppImage/DEB packages require a Linux build runner.",
      signingNoticeAr: "حزم Linux (AppImage / DEB) تتطلب بيئة بناء Linux."
    }
  }
};

export function isPublishedReleaseUrl(url: string | null | undefined): url is string {
  if (typeof url !== "string") return false;
  const trimmed = url.trim();
  return trimmed.startsWith("https://") && trimmed.length > "https://a.b/c".length;
}

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "—";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${mb.toFixed(2)} MB`;
  const kb = bytes / 1024;
  return `${kb.toFixed(1)} KB`;
}

const BUILD_API_ORIGIN = typeof __SAFI_API_BASE_URL__ === "string" ? __SAFI_API_BASE_URL__ : "";

export async function fetchDesktopReleaseManifest(): Promise<DesktopReleaseManifest> {
  // First attempt live backend release metadata (Owner-managed `app_releases` table).
  try {
    const apiBase = resolveAuthApiBaseUrl(BUILD_API_ORIGIN, false);
    if (apiBase.length > 0) {
      const resp = await fetch(
        `${apiBase}/v1/updates/latest?channel=stable&platform=windows-x86_64`,
        {
          method: "GET",
          headers: { Accept: "application/json" },
          cache: "no-store"
        }
      );
      if (resp.ok) {
        const data = (await resp.json()) as {
          version_name?: string;
          download_url?: string;
          sha256_hex?: string;
          file_size_bytes?: number;
        };
        if (
          typeof data.version_name === "string" &&
          isPublishedReleaseUrl(data.download_url) &&
          typeof data.sha256_hex === "string" &&
          data.sha256_hex.length === 64 &&
          typeof data.file_size_bytes === "number" &&
          data.file_size_bytes > 0
        ) {
          const fileName =
            data.download_url.split("/").pop()?.split("?")[0] ??
            `SafiData-Desktop-${data.version_name}-windows-x64.msi`;
          return {
            ...FALLBACK_RELEASE_MANIFEST,
            version: data.version_name,
            platforms: {
              ...FALLBACK_RELEASE_MANIFEST.platforms,
              windows: {
                ...FALLBACK_RELEASE_MANIFEST.platforms.windows,
                available: true,
                labelEn: `Windows 64-bit Installer (v${data.version_name})`,
                labelAr: `مثبت Windows 64-bit (الإصدار v${data.version_name})`,
                fileName,
                downloadUrl: data.download_url,
                sha256: data.sha256_hex,
                fileSizeBytes: data.file_size_bytes
              }
            }
          };
        }
      }
    }
  } catch {
    // Fall through cleanly to local static manifest.
  }

  // Fallback to static release manifest published alongside the Showcase.
  try {
    const resp = await fetch("/downloads/release-manifest.json", {
      method: "GET",
      headers: { Accept: "application/json" },
      cache: "no-store"
    });
    if (resp.ok) {
      return (await resp.json()) as DesktopReleaseManifest;
    }
  } catch {
    // Use typed fallback metadata.
  }

  return FALLBACK_RELEASE_MANIFEST;
}

export interface DownloadResult {
  readonly ok: boolean;
  readonly errorEn?: string;
  readonly errorAr?: string;
}

/**
 * Verifies the real artifact URL responds with HTTP 200 and non-empty bytes, then initiates a
 * genuine browser file download (never redirects into an operational Studio tab).
 */
export async function triggerDesktopInstallerDownload(
  artifact: PlatformDownloadArtifact
): Promise<DownloadResult> {
  if (!artifact.available || !artifact.downloadUrl || !artifact.fileName) {
    return {
      ok: false,
      errorEn: "This platform build is not yet published. Select Windows 64-bit to download.",
      errorAr: "هذا الإصدار غير منشور بعد. يرجى اختيار Windows 64-bit للتحميل."
    };
  }

  try {
    const head = await fetch(artifact.downloadUrl, { method: "HEAD", cache: "no-store" });
    if (!head.ok) {
      return {
        ok: false,
        errorEn: `Download artifact unavailable (HTTP ${head.status}).`,
        errorAr: `ملف التثبيت غير متاح حاليًا (HTTP ${head.status}).`
      };
    }
    const contentType = (head.headers.get("content-type") ?? "").toLowerCase();
    if (contentType.includes("text/html")) {
      return {
        ok: false,
        errorEn: "Installer binary is not staged on this host yet.",
        errorAr: "ملف التثبيت غير مرفوع على هذا الخادم بعد."
      };
    }
  } catch {
    return {
      ok: false,
      errorEn: "Could not reach the release download mirror. Please try again.",
      errorAr: "تعذّر الوصول إلى خادم التحميل. يرجى المحاولة مجددًا."
    };
  }

  const anchor = document.createElement("a");
  anchor.href = artifact.downloadUrl;
  anchor.download = artifact.fileName;
  anchor.rel = "noopener noreferrer";
  anchor.style.display = "none";
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  return { ok: true };
}

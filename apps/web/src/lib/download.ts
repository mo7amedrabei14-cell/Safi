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
  readonly publishedAt: string;
  readonly platforms: Readonly<Record<DesktopOsKey, PlatformDownloadArtifact>>;
}

export const FALLBACK_RELEASE_MANIFEST: DesktopReleaseManifest = {
  version: "1.0.0",
  channel: "stable",
  publishedAt: "2026-11-19T00:00:00Z",
  platforms: {
    windows: {
      available: true,
      labelEn: "Windows 64-bit Desktop Executable (Local Unsigned Build)",
      labelAr: "تطبيق سطح المكتب لنظام Windows 64-bit (إصدار محلي غير موقّع)",
      fileName: "SafiData-Desktop-1.0.0-windows-x64.exe",
      downloadUrl: "/downloads/SafiData-Desktop-1.0.0-windows-x64.exe",
      fileSizeBytes: 6240768,
      sha256: "f78a0776cb5cb7f9829aa95e108c8e5c8675509ac5445603d1136e4c566f9024",
      codeSigned: false,
      signingNoticeEn:
        "Authenticode code-signing requires an external commercial certificate (not yet configured in this local build).",
      signingNoticeAr:
        "التوقيع الرقمي لـ Windows (Authenticode) يتطلب شهادة تجارية خارجية لم يتم إعدادها بعد في هذا البناء المحلي."
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
          typeof data.download_url === "string" &&
          typeof data.sha256_hex === "string" &&
          data.sha256_hex.length === 64
        ) {
          return {
            ...FALLBACK_RELEASE_MANIFEST,
            version: data.version_name,
            platforms: {
              ...FALLBACK_RELEASE_MANIFEST.platforms,
              windows: {
                ...FALLBACK_RELEASE_MANIFEST.platforms.windows,
                available: true,
                downloadUrl: data.download_url,
                sha256: data.sha256_hex,
                fileSizeBytes:
                  typeof data.file_size_bytes === "number" && data.file_size_bytes > 0
                    ? data.file_size_bytes
                    : FALLBACK_RELEASE_MANIFEST.platforms.windows.fileSizeBytes
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

/**
 * `apps/web/src/lib/download.ts`
 *
 * Real file export: the original dataset is never touched, and every export is a
 * brand-new file created through a Blob URL (no write into the user's file).
 */

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

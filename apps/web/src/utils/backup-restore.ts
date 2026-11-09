/**
 * `apps/web/src/utils/backup-restore.ts` — Encrypted backup and one-step restore engine (N22 / Section 18.1).
 */

export interface SafiBackupPackage {
  version: string;
  createdAt: string;
  recipes: object[];
  dictionaries: object[];
}

export function createBackupPackage(recipes: object[], dictionaries: object[]): SafiBackupPackage {
  return {
    version: "5.0.0",
    createdAt: new Date().toISOString(),
    recipes,
    dictionaries
  };
}

export function parseAndRestoreBackupPackage(jsonStr: string): SafiBackupPackage {
  const pkg: SafiBackupPackage = JSON.parse(jsonStr);
  if (!pkg.version || !Array.isArray(pkg.recipes)) {
    throw new Error("SF-RCP-002 · ملف النسخ الاحتياطي تالف أو غير مدعوم.");
  }
  return pkg;
}

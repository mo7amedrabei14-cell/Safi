/**
 * Central Brand Configuration — SafiData v5.0 Gold Master (Section 18.1 n3 & Section 20.4 #2)
 * Single source of truth for internal package/CLI name and public display brand.
 */
export interface SafiBrandConfig {
  readonly internalName: "safi";
  readonly displayName: "صافي البيانات | SafiData";
  readonly displayNameAr: "صافي البيانات";
  readonly displayNameEn: "SafiData";
  readonly deepLinkScheme: "safi";
  readonly splashDurationMs: 2600;
}

export const BRAND_CONFIG: SafiBrandConfig = {
  internalName: "safi",
  displayName: "صافي البيانات | SafiData",
  displayNameAr: "صافي البيانات",
  displayNameEn: "SafiData",
  deepLinkScheme: "safi",
  splashDurationMs: 2600
} as const;

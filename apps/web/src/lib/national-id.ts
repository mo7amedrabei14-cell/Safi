/**
 * `apps/web/src/lib/national-id.ts`
 *
 * Official Egyptian National ID (14-digit) Validator & Enricher (Spec Section 20.3).
 * Extracts birth date, governorate name, gender, and verifies the Modulo-11 check digit.
 */
import { normalizeDigits } from "./arabic-normalize";

export const OFFICIAL_GOV_CODES: Readonly<Record<string, string>> = {
  "01": "القاهرة",
  "02": "الإسكندرية",
  "03": "بورسعيد",
  "04": "السويس",
  "11": "دمياط",
  "12": "الدقهلية",
  "13": "الشرقية",
  "14": "القليوبية",
  "15": "كفر الشيخ",
  "16": "الغربية",
  "17": "المنوفية",
  "18": "البحيرة",
  "19": "الإسماعيلية",
  "21": "الجيزة",
  "22": "بني سويف",
  "23": "الفيوم",
  "24": "المنيا",
  "25": "أسيوط",
  "26": "سوهاج",
  "27": "قنا",
  "28": "أسوان",
  "29": "الأقصر",
  "31": "البحر الأحمر",
  "32": "الوادي الجديد",
  "33": "مطروح",
  "34": "شمال سيناء",
  "35": "جنوب سيناء",
  "88": "خارج الجمهورية"
};

export interface EgyptianNationalIdInfo {
  readonly cleanId: string;
  readonly birthDate: string;
  readonly governorateCode: string;
  readonly governorateNameAr: string;
  readonly genderAr: "ذكر" | "أنثى";
  readonly checksumMatches: boolean;
  readonly expectedCheckDigit: number;
}

export type NationalIdResult =
  | { readonly ok: true; readonly info: EgyptianNationalIdInfo }
  | { readonly ok: false; readonly code: string; readonly messageAr: string };

const WEIGHTS = [2, 7, 6, 5, 4, 3, 2, 7, 6, 5, 4, 3, 2] as const;

export function validateEgyptianNationalId(raw: string): NationalIdResult {
  const clean = normalizeDigits(raw).replace(/\s+/g, "");
  if (!/^\d{14}$/.test(clean)) {
    return {
      ok: false,
      code: "SF-CLN-014",
      messageAr: "SF-CLN-014: الرقم القومي يجب أن يتكون من 14 رقمًا بالضبط"
    };
  }

  const centuryDigit = Number(clean[0]);
  const yearBase = centuryDigit === 2 ? 1900 : centuryDigit === 3 ? 2000 : null;
  if (yearBase === null) {
    return {
      ok: false,
      code: "SF-CLN-015",
      messageAr: "SF-CLN-015: رمز القرن غير صالح (يجب أن يبدأ بـ 2 أو 3)"
    };
  }

  const year = yearBase + Number(clean.slice(1, 3));
  const month = Number(clean.slice(3, 5));
  const day = Number(clean.slice(5, 7));

  const dateObj = new Date(Date.UTC(year, month - 1, day));
  if (
    month < 1 ||
    month > 12 ||
    day < 1 ||
    day > 31 ||
    dateObj.getUTCFullYear() !== year ||
    dateObj.getUTCMonth() + 1 !== month ||
    dateObj.getUTCDate() !== day
  ) {
    return {
      ok: false,
      code: "SF-CLN-016",
      messageAr: "SF-CLN-016: تاريخ الميلاد داخل الرقم القومي غير صحيح"
    };
  }

  const govCode = clean.slice(7, 9);
  const govName = OFFICIAL_GOV_CODES[govCode];
  if (govName === undefined) {
    return {
      ok: false,
      code: "SF-CLN-017",
      messageAr: "SF-CLN-017: كود المحافظة داخل الرقم القومي غير معتمد"
    };
  }

  const genderDigit = Number(clean[12]);
  const genderAr = genderDigit % 2 === 1 ? "ذكر" : "أنثى";

  let sum = 0;
  for (let i = 0; i < 13; i += 1) {
    sum += Number(clean[i]) * (WEIGHTS[i] ?? 0);
  }
  const rem = sum % 11;
  const expectedCheckDigit = rem <= 1 ? 1 - rem : 11 - rem;
  const actualCheckDigit = Number(clean[13]);

  const birthDate = `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;

  return {
    ok: true,
    info: {
      cleanId: clean,
      birthDate,
      governorateCode: govCode,
      governorateNameAr: govName,
      genderAr,
      checksumMatches: expectedCheckDigit === actualCheckDigit,
      expectedCheckDigit
    }
  };
}

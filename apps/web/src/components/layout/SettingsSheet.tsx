/**
 * Glass settings sheet opened from the dock.
 * All user-facing copy follows the selected interface locale.
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import { HardDrive, MonitorSmartphone, ShieldCheck, WifiOff, X } from "lucide-react";
import { SAFI_APP_VERSION, SAFI_BUILD_CHANNEL } from "../../lib/app-meta";

export interface SettingsSheetProps {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly lang?: "ar" | "en";
}

const SETTINGS_FACTS = [
  {
    id: "version",
    labelAr: "إصدار التطبيق",
    labelEn: "Application version",
    valueAr: "الإصدار وقناة البناء الحالية.",
    valueEn: "Current application version and build channel."
  },
  {
    id: "storage",
    labelAr: "التخزين",
    labelEn: "Storage",
    valueAr: "محلي على جهازك؛ الخزنة مشفّرة AES-256-GCM ولا تغادر الملفات الجهاز.",
    valueEn:
      "Stored locally on your device. The vault is encrypted with AES-256-GCM and your files stay on-device."
  },
  {
    id: "network",
    labelAr: "الشبكة",
    labelEn: "Network",
    valueAr: "نواة المحرك بلا وصول شبكي؛ يُفحص ذلك تلقائيًا في كل بوابة CI.",
    valueEn: "The processing engine has no network access; CI verifies this automatically."
  },
  {
    id: "motion",
    labelAr: "الحركة",
    labelEn: "Motion",
    valueAr:
      "يتبع إعداد النظام تلقائيًا؛ عند تفعيل تقليل الحركة تتوقف الأنيميشن وتظهر الحالة النهائية.",
    valueEn:
      "Follows your system preference. Reduced Motion disables animations and displays their final state."
  },
  {
    id: "trust",
    labelAr: "حارس Zero Trust",
    labelEn: "Zero Trust guard",
    valueAr: "لا تُنفَّذ أي صيغة أو ماكرو، وكل ملف يُفحص قبل التحليل بسقوف للذاكرة والوقت.",
    valueEn:
      "Formulas and macros are never executed. Files are checked before parsing with memory and time limits."
  }
] as const;

const HIGHLIGHTS = [
  { id: "offline", labelAr: "بدون إنترنت", labelEn: "Offline" },
  { id: "local", labelAr: "معالجة محلية", labelEn: "On-device processing" },
  { id: "signed", labelAr: "تحديثات موقّعة", labelEn: "Signed updates" }
] as const;

export function SettingsSheet({ open, onClose, lang = "en" }: SettingsSheetProps): ReactNode {
  const isAr = lang === "ar";
  const closeRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) return;
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="safi-palette-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="safi-glass safi-settings"
        role="dialog"
        aria-modal="true"
        aria-labelledby="safi-settings-title"
        dir={isAr ? "rtl" : "ltr"}
      >
        <header className="safi-settings-header">
          <h2 id="safi-settings-title" className="safi-card-title">
            <ShieldCheck size={16} aria-hidden="true" /> {isAr ? "الإعدادات" : "Settings"}
          </h2>
          <button
            ref={closeRef}
            type="button"
            className="safi-icon-button"
            aria-label={isAr ? "إغلاق الإعدادات" : "Close settings"}
            onClick={onClose}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </header>

        <div className="safi-chip-row">
          {HIGHLIGHTS.map((highlight) => (
            <span key={highlight.id} className="safi-chip safi-chip--accent">
              {highlight.id === "offline" ? (
                <WifiOff size={13} aria-hidden="true" />
              ) : highlight.id === "local" ? (
                <HardDrive size={13} aria-hidden="true" />
              ) : (
                <MonitorSmartphone size={13} aria-hidden="true" />
              )}
              {isAr ? highlight.labelAr : highlight.labelEn}
            </span>
          ))}
        </div>

        <dl className="safi-settings-list">
          {SETTINGS_FACTS.map((fact) => (
            <div key={fact.id} className="safi-settings-row" data-settings-fact={fact.id}>
              <dt>{isAr ? fact.labelAr : fact.labelEn}</dt>
              <dd>
                {fact.id === "version"
                  ? `${SAFI_APP_VERSION} — ${SAFI_BUILD_CHANNEL}`
                  : isAr
                    ? fact.valueAr
                    : fact.valueEn}
              </dd>
            </div>
          ))}
        </dl>

        <button type="button" className="safi-close-button" onClick={onClose}>
          {isAr ? "إغلاق" : "Close"}
        </button>
      </div>
    </div>
  );
}

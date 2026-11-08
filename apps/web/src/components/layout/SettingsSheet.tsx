/**
 * `apps/web/src/components/layout/SettingsSheet.tsx`
 *
 * Glass settings sheet opened from the dock. It only states facts the page can
 * observe (version, local storage, motion policy) — kernel guarantees such as the
 * zero-network engine are described as CI-enforced, not as something the browser
 * checked.
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import { HardDrive, MonitorSmartphone, ShieldCheck, WifiOff, X } from "lucide-react";
import { SAFI_APP_VERSION, SAFI_BUILD_CHANNEL } from "../../lib/app-meta";

export interface SettingsSheetProps {
  readonly open: boolean;
  readonly onClose: () => void;
}

const SETTINGS_FACTS: readonly {
  readonly id: string;
  readonly label: string;
  readonly value: string;
}[] = [
  {
    id: "version",
    label: "إصدار التطبيق",
    value: `${SAFI_APP_VERSION} — ${SAFI_BUILD_CHANNEL}`
  },
  {
    id: "storage",
    label: "التخزين",
    value: "محلي على جهازك؛ الخزنة مشفّرة AES-256-GCM ولا تغادر الملفات الجهاز."
  },
  {
    id: "network",
    label: "الشبكة",
    value: "نواة المحرك بلا وصول شبكي؛ يُفحص ذلك تلقائيًا في كل بوابة CI."
  },
  {
    id: "motion",
    label: "الحركة",
    value:
      "تتبع إعداد النظام تلقائيًا؛ عند تفعيل تقليل الحركة تتوقف كل الأنيميشن وتظهر الحالة النهائية."
  },
  {
    id: "trust",
    label: "حارس Zero Trust",
    value: "لا تُنفَّذ أي صيغة أو ماكرو، وكل ملف يُفحص قبل المحلل بسقوف ذاكرة ووقت."
  }
];

const HIGHLIGHTS: readonly { readonly id: string; readonly label: string }[] = [
  { id: "offline", label: "بدون إنترنت" },
  { id: "local", label: "معالجة محلية" },
  { id: "signed", label: "تحديثات موقّعة" }
];

export function SettingsSheet({ open, onClose }: SettingsSheetProps): ReactNode {
  const closeRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
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

  if (!open) {
    return null;
  }

  return (
    <div
      className="safi-palette-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        className="safi-glass safi-settings"
        role="dialog"
        aria-modal="true"
        aria-labelledby="safi-settings-title"
        dir="rtl"
      >
        <header className="safi-settings-header">
          <h2 id="safi-settings-title" className="safi-card-title">
            <ShieldCheck size={16} aria-hidden="true" /> الإعدادات
          </h2>
          <button
            ref={closeRef}
            type="button"
            className="safi-icon-button"
            aria-label="إغلاق الإعدادات"
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
              {highlight.label}
            </span>
          ))}
        </div>

        <dl className="safi-settings-list">
          {SETTINGS_FACTS.map((fact) => (
            <div key={fact.id} className="safi-settings-row" data-settings-fact={fact.id}>
              <dt>{fact.label}</dt>
              <dd>{fact.value}</dd>
            </div>
          ))}
        </dl>

        <button type="button" className="safi-close-button" onClick={onClose}>
          إغلاق
        </button>
      </div>
    </div>
  );
}

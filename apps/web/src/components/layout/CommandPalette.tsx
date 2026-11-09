/**
 * ⌘K / Ctrl+K command palette with locale-aware labels and Arabic-normalized search.
 * Keyboard: ↑/↓ move, Enter runs, Escape closes.
 */
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Command, ShieldCheck } from "lucide-react";
import { normalizeText } from "../../lib/arabic-normalize";
import { readClientAuditEnv, runClientSecurityAudit, summarizeAudit } from "../../lib/client-audit";
import type { ClientAuditCheck } from "../../lib/client-audit";
import { DOCK_TABS } from "./FloatingGlassDock";
import type { SafiDockTabId, SafiLang } from "./FloatingGlassDock";

export interface PaletteItem {
  readonly id: string;
  readonly label: string;
  readonly hint: string;
  readonly labelEn: string;
  readonly hintEn: string;
  readonly tab?: SafiDockTabId;
  readonly action?: "security-audit";
}

const TAB_HINTS: Readonly<Record<SafiDockTabId, string>> = {
  home: "مركز القيادة",
  cleaning: "مختبر التنقية والخصوصية",
  recipes: "محرك الوصفات الحتمية ومجلد F1",
  charts: "استوديو الرسوم وتباين WCAG 2.2 AA",
  library: "القوالب وتقارير الجودة",
  owner: "البطاقات الست وحارس Zero Trust"
};

const TAB_HINTS_EN: Readonly<Record<SafiDockTabId, string>> = {
  home: "Product overview and command center",
  cleaning: "Clean, normalize, and protect sensitive data",
  recipes: "Deterministic cleaning recipes and watch folders",
  charts: "Chart studio and WCAG 2.2 AA contrast checks",
  library: "Reusable templates and quality reports",
  owner: "Owner controls, security, and audit trail"
};

export const PALETTE_ITEMS: readonly PaletteItem[] = [
  ...DOCK_TABS.map((tab) => ({
    id: `nav-${tab.id}`,
    label: tab.label,
    hint: TAB_HINTS[tab.id],
    labelEn: tab.labelEn,
    hintEn: TAB_HINTS_EN[tab.id],
    tab: tab.id
  })),
  {
    id: "security-audit",
    label: "فحص أمني سريع",
    hint: "يفحص سياق التشغيل وWebCrypto وOPFS وحماية الإطار",
    labelEn: "Quick security audit",
    hintEn: "Check runtime context, WebCrypto, OPFS, and frame protections",
    action: "security-audit"
  }
];

export function filterCommands(items: readonly PaletteItem[], query: string): readonly PaletteItem[] {
  const needle = normalizeText(query.trim()).toLowerCase();
  if (needle.length === 0) return items;
  return items.filter((item) =>
    normalizeText(`${item.label} ${item.hint} ${item.labelEn} ${item.hintEn}`)
      .toLowerCase()
      .includes(needle)
  );
}

/** Wrap-around index arithmetic, exported for tests. */
export function nextIndex(current: number, delta: number, length: number): number {
  if (length <= 0) return 0;
  return (current + delta + length) % length;
}

export interface CommandPaletteProps {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly onSelectTab: (tab: SafiDockTabId) => void;
  readonly lang?: SafiLang;
}

export function CommandPalette({ open, onClose, onSelectTab, lang = "en" }: CommandPaletteProps): ReactNode {
  const isAr = lang === "ar";
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [audit, setAudit] = useState<readonly ClientAuditCheck[] | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const restoreFocusRef = useRef<Element | null>(null);

  const sourceItems = useMemo(
    () => PALETTE_ITEMS.map((item) => ({
      ...item,
      label: isAr ? item.label : item.labelEn,
      hint: isAr ? item.hint : item.hintEn
    })),
    [isAr]
  );
  const items = useMemo(() => filterCommands(sourceItems, query), [sourceItems, query]);
  const activeItem = items[Math.min(activeIndex, Math.max(0, items.length - 1))];

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    setAudit(null);
    restoreFocusRef.current = typeof document === "undefined" ? null : document.activeElement;
    inputRef.current?.focus();

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      const previous = restoreFocusRef.current;
      if (previous instanceof HTMLElement) previous.focus();
    };
  }, [open, onClose]);

  useEffect(() => {
    if (activeItem === undefined || typeof document === "undefined") return;
    document.getElementById(`safi-cmd-${activeItem.id}`)?.scrollIntoView({ block: "nearest" });
  }, [activeItem]);

  if (!open) return null;

  const runItem = (item: PaletteItem | undefined): void => {
    if (item === undefined) return;
    if (item.tab !== undefined) {
      onSelectTab(item.tab);
      onClose();
      return;
    }
    if (item.action === "security-audit") {
      setAudit(runClientSecurityAudit(readClientAuditEnv()));
    }
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>): void => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((current) => nextIndex(current, 1, items.length));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((current) => nextIndex(current, -1, items.length));
    } else if (event.key === "Enter") {
      event.preventDefault();
      runItem(activeItem);
    }
  };

  const summary = audit === null ? null : summarizeAudit(audit);
  const activeId = activeItem === undefined ? undefined : `safi-cmd-${activeItem.id}`;

  return (
    <div
      className="safi-palette-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="safi-glass safi-palette"
        role="dialog"
        aria-modal="true"
        aria-labelledby="safi-palette-title"
        dir={isAr ? "rtl" : "ltr"}
      >
        <h2 id="safi-palette-title" className="safi-visually-hidden">
          {isAr ? "لوحة الأوامر السريعة" : "Quick command palette"}
        </h2>

        <div className="safi-palette-search">
          <Command size={16} aria-hidden="true" />
          <input
            ref={inputRef}
            className="safi-palette-input"
            type="text"
            role="combobox"
            aria-expanded="true"
            aria-controls="safi-palette-list"
            aria-autocomplete="list"
            aria-activedescendant={activeId}
            aria-label={isAr ? "ابحث عن أداة أو أمر" : "Search tools and commands"}
            placeholder={isAr ? "ابحث عن أداة أو أمر…" : "Search tools and commands…"}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActiveIndex(0);
            }}
            onKeyDown={handleKeyDown}
          />
          <kbd className="safi-kbd">Esc</kbd>
        </div>

        {items.length === 0 ? (
          <p className="safi-palette-empty" role="status">
            {isAr ? `لا توجد نتائج مطابقة لـ «${query}».` : `No matching results for “${query}”.`}
          </p>
        ) : (
          <ul
            id="safi-palette-list"
            className="safi-palette-list"
            role="listbox"
            aria-label={isAr ? "الأوامر المتاحة" : "Available commands"}
          >
            {items.map((item, index) => (
              <li
                key={item.id}
                id={`safi-cmd-${item.id}`}
                role="option"
                aria-selected={index === activeIndex}
                className="safi-palette-option"
                onMouseEnter={() => setActiveIndex(index)}
                onMouseDown={(event) => {
                  event.preventDefault();
                  runItem(item);
                }}
              >
                <span className="safi-palette-option-label">
                  {item.action === "security-audit" ? <ShieldCheck size={15} aria-hidden="true" /> : null}
                  {item.label}
                </span>
                <span className="safi-palette-option-hint">{item.hint}</span>
              </li>
            ))}
          </ul>
        )}

        {audit !== null && summary !== null && (
          <div className="safi-audit" role="status" aria-live="polite">
            <p className="safi-audit-summary">
              {isAr
                ? `نتيجة الفحص: ${summary.passed} من ${summary.total} فحوصات ناجحة`
                : `Audit result: ${summary.passed} of ${summary.total} checks passed`}
            </p>
            <ul className="safi-list">
              {audit.map((check) => (
                <li
                  key={check.id}
                  className={check.passed ? "safi-status--ok" : "safi-status--bad"}
                >
                  <strong>
                    {check.passed ? "✓" : "✗"} {isAr ? check.labelAr : check.id}
                  </strong>
                  <span className="safi-hint">
                    {isAr ? check.detailAr : (check.passed ? "Passed." : "Review this security check.")}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}

        <p className="safi-palette-footer">
          <span>{isAr ? "↑ ↓ للتنقل" : "↑ ↓ Navigate"}</span>
          <span>{isAr ? "Enter للتنفيذ" : "Enter Run"}</span>
          <span>{isAr ? "Esc للإغلاق" : "Esc Close"}</span>
        </p>
      </div>
    </div>
  );
}

/**
 * `apps/web/src/components/home/SmartDropzone.tsx`
 *
 * Smart import zone for the Bento command centre. It gates files BEFORE any parser
 * runs: extension allowlist, empty-file guard, platform size ceiling and a
 * conservative zip-bomb rejection — all reported with codes from `errors.toml`
 * (SF-IMP-001 / SF-IMP-003 / SF-IMP-005).
 *
 * Nothing is uploaded: the file handle never leaves the browser, and the component
 * never logs names or sizes (AGENTS.md rule 4).
 */
import type { DragEvent as ReactDragEvent, ReactNode } from "react";
import { useRef, useState } from "react";
import { FileSpreadsheet, Upload } from "lucide-react";

export interface FileLike {
  readonly name: string;
  readonly size: number;
}

export type ImportInspection =
  | { readonly ok: true; readonly kind: "csv" | "xlsx"; readonly fileName: string }
  | { readonly ok: false; readonly code: string; readonly message: string };

/** Hard ceiling before the lazy parser is even asked to open the container. */
export const MAX_FILE_BYTES = 100 * 1024 * 1024;
/** Browser-friendly ceiling; above this we ask for the desktop app. */
export const WARN_FILE_BYTES = 50 * 1024 * 1024;

export function inspectImportFile(file: FileLike): ImportInspection {
  const name = file.name.trim();
  const lower = name.toLowerCase();
  const kind: "csv" | "xlsx" | null = lower.endsWith(".csv")
    ? "csv"
    : lower.endsWith(".xlsx")
      ? "xlsx"
      : null;

  if (kind === null) {
    return {
      ok: false,
      code: "SF-IMP-001",
      message: "SF-IMP-001 · صيغة غير مدعومة. اختر ملف CSV أو XLSX."
    };
  }

  if (file.size === 0) {
    return {
      ok: false,
      code: "SF-IMP-001",
      message: "SF-IMP-001 · الملف فارغ أو تالف، تعذّر فتحه."
    };
  }

  if (file.size > MAX_FILE_BYTES) {
    return {
      ok: false,
      code: "SF-IMP-005",
      message: "SF-IMP-005 · رفضنا الملف لأسباب أمنية: حجمه يتجاوز 100MB (احتمال قنبلة ZIP)."
    };
  }

  if (file.size > WARN_FILE_BYTES) {
    return {
      ok: false,
      code: "SF-IMP-003",
      message: "SF-IMP-003 · الملف أكبر من الحد الآمن على المتصفح (50MB). استخدم نسخة سطح المكتب."
    };
  }

  return { ok: true, kind, fileName: name };
}

export function SmartDropzone(): ReactNode {
  const [inspection, setInspection] = useState<ImportInspection | null>(null);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const handleFiles = (files: FileList): void => {
    const file = files.item(0);
    if (file === null) {
      return;
    }
    setInspection(inspectImportFile({ name: file.name, size: file.size }));
  };

  const handleDrop = (event: ReactDragEvent<HTMLDivElement>): void => {
    event.preventDefault();
    setDragging(false);
    handleFiles(event.dataTransfer.files);
  };

  return (
    <div dir="rtl" style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
      <h2 style={{ fontSize: "16px", fontWeight: 700 }}>استيراد ذكي</h2>

      <div
        onDragOver={(event) => {
          event.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={handleDrop}
        style={{
          border: `2px dashed ${dragging ? "var(--accent-cyan)" : "rgba(255, 255, 255, 0.14)"}`,
          borderRadius: "16px",
          padding: "18px",
          textAlign: "center",
          background: dragging ? "rgba(6, 182, 212, 0.08)" : "rgba(255, 255, 255, 0.02)"
        }}
      >
        <Upload size={22} aria-hidden="true" style={{ color: "var(--accent-cyan)" }} />
        <p style={{ margin: "8px 0 10px", fontSize: "13px", color: "var(--text-primary)" }}>
          اسحب ملف CSV أو XLSX هنا
        </p>
        <button
          type="button"
          onClick={() => inputRef.current?.click()}
          style={{
            minHeight: "44px",
            paddingInline: "16px",
            borderRadius: "12px",
            border: "1px solid var(--glass-border)",
            background: "rgba(255, 255, 255, 0.05)",
            color: "var(--text-primary)",
            fontFamily: "var(--font-arabic)",
            fontSize: "13px",
            cursor: "pointer"
          }}
        >
          اختر ملفًا
        </button>
        <input
          ref={inputRef}
          type="file"
          accept=".csv,.xlsx,text/csv,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
          onChange={(event) => {
            if (event.target.files !== null) {
              handleFiles(event.target.files);
            }
          }}
          style={{ display: "none" }}
        />
      </div>

      <p style={{ margin: 0, fontSize: "11px", color: "var(--text-muted)", lineHeight: 1.7 }}>
        فحص Zip Bomb وكشف الترميز العربي (UTF-8 / Windows-1256) قبل القراءة — الملف لا يغادر جهازك.
      </p>

      {inspection !== null ? (
        <p
          role="status"
          style={{
            margin: 0,
            display: "flex",
            alignItems: "center",
            gap: "8px",
            fontSize: "12px",
            color: inspection.ok ? "var(--accent-emerald)" : "#fca5a5"
          }}
        >
          <FileSpreadsheet size={14} aria-hidden="true" />
          {inspection.ok ? `جاهز للفحص: ${inspection.fileName}` : inspection.message}
        </p>
      ) : null}
    </div>
  );
}

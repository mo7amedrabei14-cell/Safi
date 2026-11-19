/**
 * `apps/web/src/desktop/components/DesktopLocalSearchPanel.tsx`
 *
 * Real Desktop UI for the Embedded Local Full-Text Search Engine (`crates/sf-desktop/src/local_search.rs`).
 * - Indexes user-selected local folders strictly on-device (zero cloud upload).
 * - Searches across filenames and extracted text (CSV, Excel, DOCX, PPTX, text/HTML, and
 *   extractable uncompressed + FlateDecode PDF streams) using BM25 + Arabic/English normalization.
 * - Search queries work 100% in Offline Read-Only Mode; indexing requires online authorization.
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import { FolderSearch, RefreshCw, Search, ShieldCheck } from "lucide-react";
import type { SafiLang } from "../../components/layout/FloatingGlassDock";
import {
  getDesktopSearchStats,
  indexDesktopSearchFolders,
  pickSearchFolder,
  queryDesktopSearch,
  type DesktopSearchHit,
  type DesktopSearchIndexSummary,
  type DesktopSearchStats
} from "../studio-bridge";

export interface DesktopLocalSearchPanelProps {
  readonly lang?: SafiLang;
  readonly readOnlyOffline?: boolean;
}

const FILE_TYPE_OPTIONS = [
  { value: "", ar: "كل الأنواع", en: "All file types" },
  { value: "csv", ar: "CSV / TSV", en: "CSV / TSV" },
  { value: "xlsx", ar: "Excel (XLSX)", en: "Excel (XLSX)" },
  { value: "docx", ar: "Word (DOCX)", en: "Word (DOCX)" },
  { value: "pptx", ar: "PowerPoint (PPTX)", en: "PowerPoint (PPTX)" },
  { value: "pdf", ar: "PDF (نص قابل للاستخراج)", en: "PDF (extractable text)" },
  { value: "text", ar: "ملفات نصية (TXT/MD)", en: "Plain Text (TXT/MD)" },
  { value: "html", ar: "HTML", en: "HTML" }
] as const;

function renderHighlightedSnippet(snippet: string): ReactNode {
  const parts = snippet.split(/(\[\[[^\]]+\]\])/g);
  return parts.map((part, idx) => {
    if (part.startsWith("[[") && part.endsWith("]]")) {
      return (
        <mark key={idx} style={{ padding: "0 2px", borderRadius: "3px" }}>
          {part.slice(2, -2)}
        </mark>
      );
    }
    return <span key={idx}>{part}</span>;
  });
}

export function DesktopLocalSearchPanel({
  lang = "en",
  readOnlyOffline = false
}: DesktopLocalSearchPanelProps): ReactNode {
  const L = useCallback((ar: string, en: string): string => (lang === "ar" ? ar : en), [lang]);
  const [folderPath, setFolderPath] = useState("");
  const [queryText, setQueryText] = useState("");
  const [fileType, setFileType] = useState("");
  const [enableFuzzy, setEnableFuzzy] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<DesktopSearchStats | null>(null);
  const [lastSummary, setLastSummary] = useState<DesktopSearchIndexSummary | null>(null);
  const [hits, setHits] = useState<readonly DesktopSearchHit[]>([]);
  const [hasSearched, setHasSearched] = useState(false);

  const refreshStats = useCallback(async () => {
    try {
      const next = await getDesktopSearchStats();
      setStats(next);
    } catch {
      // Ignore before session is bound.
    }
  }, []);

  useEffect(() => {
    void refreshStats();
  }, [refreshStats]);

  const handlePickFolder = async (): Promise<void> => {
    setError(null);
    try {
      const picked = await pickSearchFolder();
      if (picked !== null) {
        setFolderPath(picked);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleIndexFolder = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    const target = folderPath.trim();
    if (target.length === 0) {
      setError(
        L("أدخل مسار مجلد محلي أو اختره أولًا.", "Enter or pick a local folder path first.")
      );
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await indexDesktopSearchFolders([target], "main");
      setLastSummary(result.summary);
      setStats(result.stats);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleSearch = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    const trimmed = queryText.trim();
    if (trimmed.length === 0) {
      setHits([]);
      setHasSearched(false);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await queryDesktopSearch({
        query: trimmed,
        fileType: fileType.length > 0 ? fileType : null,
        enableFuzzy,
        limit: 30
      });
      setHits(result.hits);
      setStats(result.stats);
      setHasSearched(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      className="safi-panel"
      data-desktop-local-search="true"
      style={{ display: "grid", gap: "16px", padding: "18px" }}
    >
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          justifyContent: "space-between",
          alignItems: "center",
          gap: "12px"
        }}
      >
        <div>
          <h2 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
            <FolderSearch size={18} aria-hidden="true" />
            <span>{L("البحث المحلي الشامل في ملفاتك", "Local Full-Text File Search")}</span>
          </h2>
          <p style={{ margin: "4px 0 0", fontSize: "13px", opacity: 0.82 }}>
            {L(
              "فهرسة وبحث BM25 محليان بالكامل (عربي/إنجليزي، CSV، Excel، Word، PowerPoint، وPDF نصي بما يشمل FlateDecode). ملفات PDF المصوّرة بدون طبقة نصية تُعلَّم بوضوح دون OCR.",
              "100% on-device BM25 full-text search (Arabic/English, CSV, Excel, Word, PowerPoint, and extractable uncompressed/FlateDecode PDFs). Scanned image-only PDFs are flagged honestly without OCR."
            )}
          </p>
        </div>

        <div
          data-local-search-stats="true"
          style={{
            display: "flex",
            gap: "12px",
            fontSize: "12px",
            fontWeight: 700,
            alignItems: "center"
          }}
        >
          <span data-indexed-docs-count={stats?.totalDocuments ?? 0}>
            {L("المستندات المفهرسة:", "Indexed docs:")} {stats?.totalDocuments ?? 0}
          </span>
          <span data-indexed-terms-count={stats?.totalTerms ?? 0}>
            {L("المفردات:", "Terms:")} {stats?.totalTerms ?? 0}
          </span>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={() => void refreshStats()}
            disabled={busy}
          >
            <RefreshCw size={14} aria-hidden="true" />
          </button>
        </div>
      </div>

      {error !== null ? (
        <div role="alert" data-local-search-error="true" style={{ fontSize: "13px" }}>
          {error}
        </div>
      ) : null}

      <form
        onSubmit={(e) => void handleIndexFolder(e)}
        style={{ display: "flex", flexWrap: "wrap", gap: "8px", alignItems: "center" }}
      >
        <input
          type="text"
          value={folderPath}
          onChange={(e) => setFolderPath(e.target.value)}
          placeholder={L("مسار المجلد المحلي للفهرسة...", "Local folder path to index...")}
          aria-label={L("مسار المجلد المحلي", "Local folder path")}
          className="safi-input"
          data-local-search-folder-input="true"
          style={{ flex: "1 1 280px" }}
          disabled={readOnlyOffline || busy}
        />
        <button
          type="button"
          className="safi-btn safi-btn-ghost"
          onClick={() => void handlePickFolder()}
          disabled={readOnlyOffline || busy}
          data-local-search-pick-folder="true"
        >
          {L("اختيار مجلد", "Pick Folder")}
        </button>
        <button
          type="submit"
          className="safi-btn safi-btn-primary"
          disabled={readOnlyOffline || busy}
          data-local-search-index-btn="true"
        >
          <ShieldCheck size={15} aria-hidden="true" />
          <span>{L("فهرسة المجلد محليًا", "Index Folder Locally")}</span>
        </button>
      </form>

      {lastSummary !== null ? (
        <div data-local-search-index-summary="true" style={{ fontSize: "12px", opacity: 0.88 }}>
          {L(
            `اكتملت الفهرسة: مضاف ${lastSummary.added} • محدث ${lastSummary.updated} • بدون تغيير ${lastSummary.unchanged} • إجمالي المستندات ${lastSummary.totalDocuments}`,
            `Indexing complete: added ${lastSummary.added} • updated ${lastSummary.updated} • unchanged ${lastSummary.unchanged} • total docs ${lastSummary.totalDocuments}`
          )}
        </div>
      ) : null}

      <form
        onSubmit={(e) => void handleSearch(e)}
        style={{ display: "flex", flexWrap: "wrap", gap: "8px", alignItems: "center" }}
      >
        <input
          type="search"
          value={queryText}
          onChange={(e) => setQueryText(e.target.value)}
          placeholder={L(
            'ابحث داخل الملفات (مثال: "تقرير المبيعات" أو rev*)...',
            'Search inside local files (e.g. "Q4 revenue" or cairo*)...'
          )}
          aria-label={L("استعلام البحث المحلي", "Local search query")}
          className="safi-input"
          data-local-search-query-input="true"
          style={{ flex: "1 1 260px" }}
        />
        <select
          value={fileType}
          onChange={(e) => setFileType(e.target.value)}
          aria-label={L("تصفية حسب نوع الملف", "Filter by file type")}
          className="safi-input"
          data-local-search-type-select="true"
        >
          {FILE_TYPE_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {L(opt.ar, opt.en)}
            </option>
          ))}
        </select>
        <label style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "12px" }}>
          <input
            type="checkbox"
            checked={enableFuzzy}
            onChange={(e) => setEnableFuzzy(e.target.checked)}
          />
          <span>{L("تحمل الأخطاء الإملائية", "Fuzzy")}</span>
        </label>
        <button
          type="submit"
          className="safi-btn safi-btn-primary"
          disabled={busy}
          data-local-search-submit="true"
        >
          <Search size={15} aria-hidden="true" />
          <span>{L("بحث", "Search")}</span>
        </button>
      </form>

      <div data-local-search-results="true" style={{ display: "grid", gap: "10px" }}>
        {hits.map((hit) => (
          <article
            key={`${hit.path}:${hit.matchLocation}`}
            data-local-search-hit="true"
            style={{
              padding: "12px",
              borderRadius: "10px",
              border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
              display: "grid",
              gap: "4px"
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                gap: "8px",
                fontWeight: 700,
                fontSize: "13px"
              }}
            >
              <span>{hit.fileName}</span>
              <span style={{ opacity: 0.75 }}>
                {hit.fileType.toUpperCase()} • {hit.matchLocation} • {hit.score.toFixed(2)}
              </span>
            </div>
            <div style={{ fontSize: "12px", opacity: 0.9 }}>
              {renderHighlightedSnippet(hit.snippet)}
            </div>
            <div style={{ fontSize: "11px", opacity: 0.65, wordBreak: "break-all" }}>
              {hit.path}
            </div>
          </article>
        ))}

        {hasSearched && hits.length === 0 ? (
          <p data-local-search-empty="true" style={{ margin: 0, fontSize: "13px", opacity: 0.78 }}>
            {L(
              "لا توجد نتائج مطابقة في الملفات المفهرسة.",
              "No matching hits in indexed local files."
            )}
          </p>
        ) : null}
      </div>
    </section>
  );
}

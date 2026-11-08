/**
 * `apps/web/src/components/studios/CleaningStudio.tsx`
 *
 * Live Arabic normalization + PII laboratory. Everything runs in this page:
 * normalization mirrors the Rust engine, PII is masked, and tokens come from the
 * audited BLAKE3 implementation with a per-tenant salt. Nothing is uploaded and
 * nothing is logged.
 */
import type { ChangeEvent, ReactNode } from "react";
import { useMemo, useState } from "react";
import { Fingerprint, ShieldCheck, Sparkles } from "lucide-react";
import { normalizeLines } from "../../lib/arabic-normalize";
import {
  PII_KIND_LABELS,
  countByKind,
  detectPii,
  maskText,
  maskValue,
  tokenizeValue
} from "../../lib/pii";
import type { PiiKind } from "../../lib/pii";
import { DecryptedMonoText } from "../ui/DecryptedMonoText";
import { MagneticButton } from "../ui/MagneticButton";
import { SpotlightCard } from "../ui/SpotlightCard";

export const SAMPLE_CLEANING_TEXT = [
  "الاسم: أَحْمَدُ محمد عبد الله",
  "الهاتف: ٠١٠١٢٣٤٥٦٧٨",
  "البريد: ahmed.mohamed@example.com",
  "الرقم القومي: 29801011234567",
  "العنوان: ١٢ شارع التحرير ـــ الجيزة",
  "المبلغ: ١٥٠٠ ج.م"
].join("\n");

export const DEFAULT_TOKEN_SALT = "safi-local-tenant";

interface TokenRow {
  readonly kind: PiiKind;
  readonly masked: string;
  readonly token: string;
}

interface Analysis {
  readonly normalized: string;
  readonly masked: string;
  readonly rows: readonly TokenRow[];
  readonly counts: ReturnType<typeof countByKind>;
  readonly total: number;
}

function analyze(raw: string, salt: string): Analysis {
  const normalized = normalizeLines(raw);
  const matches = detectPii(normalized);
  return {
    normalized,
    masked: maskText(normalized, matches),
    rows: matches.map((match) => ({
      kind: match.kind,
      masked: maskValue(match.kind, match.value),
      token: tokenizeValue(match.kind, match.value, salt)
    })),
    counts: countByKind(matches),
    total: matches.length
  };
}

export function CleaningStudio(): ReactNode {
  const [draft, setDraft] = useState(SAMPLE_CLEANING_TEXT);
  const [submitted, setSubmitted] = useState(SAMPLE_CLEANING_TEXT);
  const [salt, setSalt] = useState(DEFAULT_TOKEN_SALT);

  const analysis = useMemo(() => analyze(submitted, salt), [submitted, salt]);
  const activeKinds = useMemo(
    () =>
      (Object.keys(analysis.counts) as Array<keyof typeof analysis.counts>).filter(
        (kind) => analysis.counts[kind] > 0
      ),
    [analysis.counts]
  );

  const handleDraft = (event: ChangeEvent<HTMLTextAreaElement>): void =>
    setDraft(event.target.value);
  const handleSalt = (event: ChangeEvent<HTMLInputElement>): void => setSalt(event.target.value);

  return (
    <section className="safi-studio" aria-labelledby="safi-cleaning-title" dir="rtl">
      <header className="safi-studio-header">
        <span className="safi-eyebrow">مختبر التنقية والخصوصية</span>
        <h1 id="safi-cleaning-title" className="safi-studio-title">
          التنظيف وPII
        </h1>
        <p className="safi-studio-lead">
          الصق نصًا عربيًا لتشاهد التطبيع القياسي، وإخفاء البيانات الشخصية، والترميز الحتمي — كل ذلك
          داخل هذا الجهاز.
        </p>
      </header>

      <div className="safi-grid-2">
        <SpotlightCard padding={22}>
          <label className="safi-field-label" htmlFor="safi-cleaning-input">
            النص التجريبي (مثال ببيانات وهمية)
          </label>
          <textarea
            id="safi-cleaning-input"
            className="safi-textarea"
            value={draft}
            onChange={handleDraft}
            rows={8}
            spellCheck={false}
          />

          <label className="safi-field-label" htmlFor="safi-token-salt">
            ملح الترميز (Salt) — يغيّر البصمات دون تغيير النص
          </label>
          <input
            id="safi-token-salt"
            className="safi-input safi-mono"
            value={salt}
            onChange={handleSalt}
            dir="ltr"
            spellCheck={false}
          />

          <div className="safi-actions">
            <MagneticButton onClick={() => setSubmitted(draft)} ariaLabel="شغّل التنقية محليًا">
              <Sparkles size={16} aria-hidden="true" />
              شغّل التنقية محليًا
            </MagneticButton>
            <MagneticButton
              variant="glass"
              onClick={() => {
                setDraft(SAMPLE_CLEANING_TEXT);
                setSubmitted(SAMPLE_CLEANING_TEXT);
                setSalt(DEFAULT_TOKEN_SALT);
              }}
              ariaLabel="استعادة المثال"
            >
              استعادة المثال
            </MagneticButton>
          </div>

          <p className="safi-hint">
            لا يُرسل هذا النص إلى أي خادم، ولا يُكتب في السجلات. المعالجة الفعلية للملفات تتم في
            نواة Rust المعزولة.
          </p>
        </SpotlightCard>

        <SpotlightCard padding={22}>
          <h2 className="safi-card-title">نتيجة التطبيع العربي</h2>
          <div className="safi-compare">
            <div>
              <span className="safi-mini-label">قبل</span>
              <pre className="safi-pre" dir="auto">
                {submitted}
              </pre>
            </div>
            <div>
              <span className="safi-mini-label">بعد (sf-arabic)</span>
              <pre className="safi-pre" dir="auto" data-testid="safi-normalized">
                {analysis.normalized}
              </pre>
            </div>
          </div>

          <h2 className="safi-card-title" style={{ marginTop: "20px" }}>
            <ShieldCheck size={16} aria-hidden="true" /> الإخفاء الجزئي
          </h2>
          <pre className="safi-pre" dir="auto" data-testid="safi-masked">
            {analysis.masked}
          </pre>
        </SpotlightCard>
      </div>

      <SpotlightCard padding={22}>
        <h2 className="safi-card-title">
          <Fingerprint size={16} aria-hidden="true" /> الاستبدال الرمزي الحتمي (BLAKE3)
        </h2>

        <div className="safi-chip-row" role="status" aria-live="polite">
          <span className="safi-chip">{analysis.total} حقلًا شخصيًا</span>
          {activeKinds.map((kind) => (
            <span key={kind} className="safi-chip safi-chip--accent" data-pii-kind={kind}>
              {PII_KIND_LABELS[kind]}: {analysis.counts[kind]}
            </span>
          ))}
        </div>

        {analysis.rows.length === 0 ? (
          <p className="safi-hint">
            لا توجد بيانات شخصية مطابقة في هذا النص. جرّب رقم هاتف أو بريدًا إلكترونيًا.
          </p>
        ) : (
          <div className="safi-table-wrap">
            <table className="safi-table">
              <thead>
                <tr>
                  <th scope="col">النوع</th>
                  <th scope="col">القيمة المخفية</th>
                  <th scope="col">الرمز (Token)</th>
                </tr>
              </thead>
              <tbody>
                {analysis.rows.map((entry, index) => (
                  <tr key={`${entry.kind}-${index}`} data-token-row={entry.kind}>
                    <td>{PII_KIND_LABELS[entry.kind]}</td>
                    <td className="safi-mono">{entry.masked}</td>
                    <td>
                      <DecryptedMonoText value={entry.token} title={entry.token} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <p className="safi-hint">
          الرمز = ‎BLAKE3(salt : النوع : القيمة)‎ بـ16 بايت. نفس الملح والقيمة يعطيان دائمًا نفس
          الرمز، فيبقى الربط بين الجداول ممكنًا داخل جهازك، ويستحيل خارجه.
        </p>
      </SpotlightCard>
    </section>
  );
}

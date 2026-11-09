/**
 * `apps/web/src/components/home/BentoCommandCenter.tsx`
 *
 * SafiData — Full-Viewport Apple & ReactBits.dev Product Website
 * - 100% Full-Screen Width (zero wasted side margins)
 * - Side-by-side Split Hero filling the entire first viewport (`min-height: calc(100vh - 118px)`)
 * - ReactBits motion arsenal:
 *   1. `<RotatingText />` 3D-flipping headline words every 2.0s
 *   2. `<ShinyText />` metallic sweep badge
 *   3. `<TiltedCard />` 3D interactive studio window visible immediately on the first screen
 *   4. Continuous laser scan beam + auto-stepping rows + animated equalizer chart
 *   5. `<AnimatedNumber />` + `<DecryptedMonoText />` live metrics
 *   6. Continuous `<InfiniteMarquee />` ribbon
 */
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import {
  ArrowLeftRight,
  BarChart3,
  CheckCircle2,
  Download,
  FileSpreadsheet,
  Lock,
  Monitor,
  ShieldCheck,
  Sparkles,
  Zap
} from "lucide-react";
import { gsap } from "gsap";
import type { SafiLang } from "../layout/FloatingGlassDock";
import { BlurWordText } from "../ui/BlurWordText";
import { CountUpMetric } from "../ui/CountUpMetric";
import { DecryptedMonoText } from "../ui/DecryptedMonoText";
import { MagneticButton } from "../ui/MagneticButton";
import { SpotlightCard } from "../ui/SpotlightCard";
import { canAnimate } from "../ui/motion-preference";
import { LivePipelineVisualizer } from "./LivePipelineVisualizer";
import { SmartDropzone } from "./SmartDropzone";

export interface BentoCommandCenterProps {
  readonly onStart?: () => void;
  readonly onOpenLibrary?: () => void;
  readonly lang?: SafiLang;
}

const ROTATING_WORDS_AR = [
  "ينقّي الجداول تلقائيًا",
  "يوحّد النصوص بجميع اللغات",
  "يفحص الهويات والأكواد فورًا",
  "يصنع تقارير PDF ذكية"
] as const;

const ROTATING_WORDS_EN = [
  "Cleans Sheets Automatically",
  "Unifies Multilingual Text",
  "Validates IDs & Records",
  "Builds Instant PDF Reports"
] as const;

const LIVE_DEMO_ROWS = [
  {
    id: 0,
    labelAr: "توحيد الأسماء والنصوص متعددة اللغات",
    labelEn: "Multilingual Name & Text Normalization",
    promptAr: "توحيد الأسماء المركبة والرموز في جميع اللغات والترميزات...",
    promptEn: "Normalize multilingual names, symbols & Unicode variants...",
    toastAr: "✓ تم توحيد 1,420 سجل متعدد اللغات في 0.04 ثانية",
    toastEn: "✓ Normalized 1,420 multilingual records in 0.04s",
    before: "Dr.  Jean-Luc   Picard  /  أ.د  أحمد   عبد الله",
    after: "Jean-Luc Picard  |  احمد عبدالله",
    tagAr: "تم التوحيد ✓",
    tagEn: "Normalized ✓"
  },
  {
    id: 1,
    labelAr: "فحص أرقام الهويات والسجلات القياسية",
    labelEn: "Universal ID & Record Validation",
    promptAr: "فحص صحة أرقام الهويات (14 رقمًا) واستخلاص المنطقة وتاريخ الميلاد...",
    promptEn: "Validate ID numbers & extract region and birth date...",
    toastAr: "✓ تم التحقق من 12,800 رقم هوية بدقة 100%",
    toastEn: "✓ Verified 12,800 ID records with 100% accuracy",
    before: "٢٩٠٠٥١٥٠١١٢٣٤٧  |  ID-994021",
    after: "29005150112347 • Verified (1990-05-15)",
    tagAr: "سجل صحيح ✓",
    tagEn: "Verified ID ✓"
  },
  {
    id: 2,
    labelAr: "حماية وإخفاء البيانات الحساسة (PII)",
    labelEn: "Phone & Sensitive PII Masking",
    promptAr: "تشفير وإخفاء أرقام الهواتف والحسابات والبيانات الشخصية...",
    promptEn: "Mask sensitive phone numbers, emails & PII columns locally...",
    toastAr: "✓ تم تأمين 4,950 سجل حساس بدون مغادرة جهازك",
    toastEn: "✓ Masked 4,950 sensitive records 100% offline",
    before: "+44-7700-900077  |  010-1234-5678",
    after: "+44-••••-••0077  |  010••••5678",
    tagAr: "محمي بالكامل ✓",
    tagEn: "Masked ✓"
  },
  {
    id: 3,
    labelAr: "توحيد العملات والتواريخ العالمية",
    labelEn: "Global Currency & ISO Date Standardization",
    promptAr: "تحويل الأرقام والعملات والتواريخ المبعثرة إلى صيغة دولية قياسية...",
    promptEn: "Standardize multi-format currencies & mixed dates to ISO...",
    toastAr: "✓ جاهز للتصدير الفوري إلى PDF وExcel نظيف",
    toastEn: "✓ Ready for instant vector PDF & clean Excel export",
    before: "$12,450.75  |  ١٤/٠٥/٢٠٢٦",
    after: "12,450.75 USD  |  2026-05-14",
    tagAr: "جاهز للتقارير ✓",
    tagEn: "Report Ready ✓"
  }
] as const;

const VISUAL_EDITORS_TOOLS = [
  {
    id: 0,
    badgeAr: "الأداة 01 • محرر النصوص",
    badgeEn: "Tool 01 • Text Studio",
    titleAr: "مُحرّر توحيد النصوص متعددة اللغات",
    titleEn: "Universal Multilingual Text Normalizer",
    descAr:
      "يعالج اختلافات الحروف والألقاب والأسماء المركبة بجميع اللغات مع معاينة فورية قبل وبعد.",
    descEn:
      "Visually unifies spelling variations, titles, and compound names across all languages.",
    sampleBefore: "Prof.  María-José   Carreño  |  أ.د / فاطمه عبد الرحمن",
    sampleAfter: "Maria-Jose Carreno  |  فاطمة عبدالرحمن",
    metric: "99.94% Match"
  },
  {
    id: 1,
    badgeAr: "الأداة 02 • فاحص السجلات",
    badgeEn: "Tool 02 • ID Inspector",
    titleAr: "مُحلّل الهويات والأكواد الذكي",
    titleEn: "Universal ID & Code Visual Decoder",
    descAr: "يفكك أرقام الهويات والرموز الضريبية بصريًا إلى تاريخ الميلاد والمنطقة ورقم التحقق.",
    descEn: "Deconstructs national IDs, tax codes, and IBANs into verified structured fields.",
    sampleBefore: "٢٩٥١١٢٠١٢٠١٥٩٣  |  IBAN-EG800002",
    sampleAfter: "1995-11-20 • Region 12 • Valid Checksum ✓",
    metric: "Instant Checksum"
  },
  {
    id: 2,
    badgeAr: "الأداة 03 • درع الخصوصية",
    badgeEn: "Tool 03 • Privacy Shield",
    titleAr: "مُحرّر إخفاء البيانات الحساسة",
    titleEn: "Visual PII Masking Editor",
    descAr: "حدّد الأعمدة الحساسة لإخفائها جزئيًا أو تشفيرها قبل مشاركة الملف مع أي جهة.",
    descEn: "Select sensitive columns to mask or hash locally before sharing spreadsheets.",
    sampleBefore: "alex.morgan@corp.io | +1-415-890-2211",
    sampleAfter: "al••••@corp.io | +1-•••-•••-2211",
    metric: "Zero-Leak PII"
  },
  {
    id: 3,
    badgeAr: "الأداة 04 • استوديو الرسوم",
    badgeEn: "Tool 04 • Chart Studio",
    titleAr: "مُصمّم الرسوم البيانية التفاعلي",
    titleEn: "Interactive 120 FPS Chart Builder",
    descAr: "حوّل الأعمدة المنقّاة بأي لغة إلى رسوم بيانية تفاعلية متجاوبة بضغطة واحدة.",
    descEn: "Turn cleaned columns in any language into publication-grade interactive charts.",
    sampleBefore: "جدول مبيعات عالمي خام (185,000 صف)",
    sampleAfter: "لوحة رسوم بيانية تفاعلية للمبيعات والنمو",
    metric: "120 FPS Canvas"
  },
  {
    id: 4,
    badgeAr: "الأداة 05 • ناشر التقارير",
    badgeEn: "Tool 05 • PDF Publisher",
    titleAr: "مُولّد تقارير PDF التنفيذية",
    titleEn: "Vector PDF Report Generator",
    descAr: "يُنشئ تقرير PDF متجهيًا منسّقًا بجميع اللغات مع الملخص التنفيذي والجداول بدون إنترنت.",
    descEn: "Compiles crisp multilingual vector PDF reports with summary KPIs completely offline.",
    sampleBefore: "ملف بيانات خام بعد التنظيف",
    sampleAfter: "تقرير تنفيذي PDF جاهز للطباعة والعرض",
    metric: "Instant Vector PDF"
  }
] as const;

const WELL_ORGANIZED_CATEGORIES = [
  {
    id: 0,
    code: "01",
    titleAr: "الاستيراد والفحص الذكي للملفات",
    titleEn: "Smart File Ingest & Auto-Detection",
    subtitleAr:
      "يتعرف تلقائيًا على رأس الجدول، ترميز اللغات (Unicode/UTF-8)، ونوع كل عمود فور فتح الملف.",
    subtitleEn:
      "Automatically detects table headers, global character encodings, and column types on open.",
    tagAr: "CSV • XLSX • Parquet",
    tagEn: "CSV • XLSX • Parquet"
  },
  {
    id: 1,
    code: "02",
    titleAr: "تنقية وتوحيد النصوص بجميع اللغات",
    titleEn: "Universal Multilingual Text Standardization",
    subtitleAr:
      "توحيد الحروف والرموز والمسافات والأسماء المركبة في كافة اللغات لمنع التكرار الخفي.",
    subtitleEn:
      "Unifies characters, symbols, whitespace, and compound names across every language.",
    tagAr: "دعم جميع اللغات",
    tagEn: "Universal NLP"
  },
  {
    id: 2,
    code: "03",
    titleAr: "التحقق من الهويات وكشف التكرار",
    titleEn: "Identity Validation & Deduplication",
    subtitleAr: "فحص أرقام الهويات، الهواتف، والحسابات البنكية مع كشف السجلات المتشابهة ذكيًا.",
    subtitleEn: "Validates IDs, phone numbers, and IBANs while fuzzy-matching duplicate records.",
    tagAr: "دقة 99.9%",
    tagEn: "99.9% Precision"
  },
  {
    id: 3,
    code: "04",
    titleAr: "حماية الخصوصية والتشفير المحلي",
    titleEn: "Local Privacy & PII Masking",
    subtitleAr: "إخفاء البيانات الحساسة وتوليد بصمة تحقق رقمية بدون اتصال بالإنترنت.",
    subtitleEn: "Masks sensitive personal fields with cryptographic verification 100% offline.",
    tagAr: "100% على جهازك",
    tagEn: "100% On-Device"
  },
  {
    id: 4,
    code: "05",
    titleAr: "الرسوم البيانية وتقارير PDF",
    titleEn: "Interactive Charts & PDF Reports",
    subtitleAr: "تحويل البيانات الصافية مباشرة إلى لوحة رسوم بيانية وتقرير PDF جاهز للإدارة.",
    subtitleEn: "Turns clean data into executive charts and vector PDF reports in seconds.",
    tagAr: "جاهز للعرض فورًا",
    tagEn: "Executive Ready"
  }
] as const;

const MARQUEE_ITEMS_AR = [
  "توحيد النصوص والأسماء بجميع اللغات والترميزات العالمية بضغطة زر",
  "فحص صحة أرقام الهويات والحسابات البنكية والضرائب تلقائيًا",
  "يعمل بالكامل على جهازك بدون إرسال أي ملف للإنترنت",
  "معالجة حتى 125,000 صف في الثانية الواحدة",
  "إخفاء البيانات الشخصية الحساسة قبل مشاركة الملفات",
  "تصدير تقارير PDF متجهية أنيقة ورسوم بيانية جاهزة للعرض",
  "حفظ خطوات التنظيف كقوالب جاهزة لإعادة استخدامها على أي ملف جديد"
] as const;

const MARQUEE_ITEMS_EN = [
  "One-click universal multilingual text & character normalization",
  "Instant ID, IBAN, tax code & phone number validation",
  "100% local desktop processing — zero files ever leave your device",
  "Blazing 125,000 rows/second native Rust engine",
  "Automatic PII privacy masking before sharing spreadsheets",
  "Instant vector PDF reports & publication-ready charts",
  "Reusable cleaning recipes for recurring monthly files"
] as const;

const FEATURES_LIST = [
  {
    id: "feat-universal",
    icon: Sparkles,
    metric: "99.9%",
    metricLabelAr: "دقة توحيد اللغات",
    metricLabelEn: "Multilingual Precision",
    titleAr: "محرك عالمي يدعم جميع اللغات والترميزات",
    titleEn: "Universal Multilingual & Unicode Intelligence",
    descAr:
      "يعالج الجداول بجميع اللغات العالمية (UTF-8 / Unicode)، ويوحّد اختلافات الحروف، المسافات المكررة، والأسماء المركبة ليصبح جدولك موحّدًا وخاليًا من التكرار مهما كانت لغة البيانات.",
    descEn:
      "Processes spreadsheets in every language and script, automatically resolving character variations, invisible symbols, and mixed numerals so duplicates disappear."
  },
  {
    id: "feat-speed",
    icon: Zap,
    metric: "185ms",
    metricLabelAr: "لتنظيف 50 ألف صف",
    metricLabelEn: "Per 50,000 Rows",
    titleAr: "سرعة فائقة على الملفات المليونية الضخمة",
    titleEn: "Blazing Performance on Massive Files",
    descAr:
      "وداعًا لتجمّد برنامج Excel عند فتح الملفات الكبيرة. صافي يعالج مئات الآلاف من الصفوف في أجزاء من الثانية بفضل محركه المحلي المبني على Rust وApache Arrow.",
    descEn:
      "No more frozen spreadsheets. Process hundreds of thousands of rows in milliseconds with a native Rust & Apache Arrow columnar engine."
  },
  {
    id: "feat-privacy",
    icon: ShieldCheck,
    metric: "100%",
    metricLabelAr: "خصوصية على جهازك",
    metricLabelEn: "On-Device Privacy",
    titleAr: "بياناتك لا تغادر جهازك أبدًا",
    titleEn: "Your Data Never Leaves Your Computer",
    descAr:
      "لا توجد سحابة إلكترونية ولا خوادم خارجية. جميع ملفات العملاء والرواتب والبيانات المالية تُعالج داخل تطبيق سطح المكتب على جهازك حتى عند فصل الإنترنت تمامًا.",
    descEn:
      "Zero cloud uploads. Sensitive payroll, customer, and financial spreadsheets stay strictly on your machine and work completely offline."
  },
  {
    id: "feat-charts",
    icon: BarChart3,
    metric: "PDF + SVG",
    metricLabelAr: "تقارير ورسوم فورية",
    metricLabelEn: "Instant Reports & Charts",
    titleAr: "من جدول فوضوي إلى تقرير جاهز للعرض",
    titleEn: "From Raw Sheet to Executive Report",
    descAr:
      "بمجرد تنظيف البيانات، يولّد التطبيق رسومًا بيانية تفاعلية بسرعة 120 إطارًا/ثانية وتقارير PDF متجهية جاهزة للطباعة أو العرض على الإدارة في ثوانٍ.",
    descEn:
      "Turn cleaned spreadsheets directly into crisp multilingual vector PDF reports and 120 FPS interactive charts in one click."
  },
  {
    id: "feat-dedup",
    icon: ArrowLeftRight,
    metric: "Fuzzy AI",
    metricLabelAr: "كشف التكرار الذكي",
    metricLabelEn: "Smart Deduplication",
    titleAr: "اكتشاف السجلات المتشابهة ودمج التكرار",
    titleEn: "Fuzzy Deduplication & Record Linkage",
    descAr:
      "يكتشف السجلات المكررة حتى لو كُتبت بطرق مختلفة أو تضمنت أخطاء إملائية، ويعرضها لك بصريًا لدمجها بضغطة واحدة دون فقدان أي معلومة.",
    descEn:
      "Detects duplicate rows even when spelled slightly differently across columns, letting you merge and link records cleanly."
  },
  {
    id: "feat-anomaly",
    icon: CheckCircle2,
    metric: "Auto-Scan",
    metricLabelAr: "فحص هيكلي فوري",
    metricLabelEn: "Instant Schema Scan",
    titleAr: "اكتشاف تلقائي للقيم الشاذة وأخطاء الأعمدة",
    titleEn: "Automated Anomaly & Schema Detection",
    descAr:
      "يفحص جميع الأعمدة فور فتح الملف ليكتشف القيم الفارغة، التواريخ المعكوسة، المبالغ غير المنطقية، وأنواع البيانات المختلطة قبل أن تؤثر على حساباتك.",
    descEn:
      "Automatically profiles every column on import to highlight outliers, broken dates, mixed types, and missing values."
  },
  {
    id: "feat-security",
    icon: Lock,
    metric: "Zero-Risk",
    metricLabelAr: "حماية من المعادلات الخبيثة",
    metricLabelEn: "Formula Injection Shield",
    titleAr: "درع حماية ضد حقن المعادلات والماكرو",
    titleEn: "CSV/Excel Formula-Injection Neutralization",
    descAr:
      "يحمي جهازك وفريقك من الأكواد والمعادلات المخفية داخل ملفات CSV وExcel الخارجية، مع ضمان عدم تعديل الملف الأصلي أبدًا عند التصدير.",
    descEn:
      "Neutralizes malicious CSV/Excel formula injections and macros automatically while keeping your original source file 100% untouched."
  },
  {
    id: "feat-recipes",
    icon: FileSpreadsheet,
    metric: "1-Click",
    metricLabelAr: "قوالب تنظيف جاهزة",
    metricLabelEn: "Reusable Pipelines",
    titleAr: "احفظ خطواتك وطبقها على ملفات كل شهر بضغطة زر",
    titleEn: "Reusable Visual Cleaning Recipes",
    descAr:
      "لا تكرر نفس العمل كل أسبوع؛ احفظ خطوات التنظيف والدمج كوصفة بصرية جاهزة، وطبقها على أي ملف جديد في ثانية واحدة وبنفس الدقة الحتمية.",
    descEn:
      "Save your cleaning steps once as a visual recipe and replay them deterministically on next month's spreadsheets in one click."
  }
] as const;

const COMPARISON_ROWS = [
  {
    featureAr: "1. دعم وتوحيد البيانات بجميع اللغات والترميزات (Unicode / UTF-8)",
    featureEn: "1. Universal Multilingual & Unicode Text Normalization",
    safiAr: "✓ مدمج تلقائيًا لجميع اللغات بضغطة زر",
    safiEn: "✓ Automatic across all languages in 1 click",
    excelAr: "معادلات يدوية معقدة وعرضة للخطأ",
    excelEn: "Complex, error-prone manual formulas",
    cloudAr: "دعم محدود وقد يفسد ترميز بعض اللغات",
    cloudEn: "Limited support & encoding corruption risks"
  },
  {
    featureAr: "2. سرعة معالجة الملفات الضخمة (حتى 125,000 صف/ثانية)",
    featureEn: "2. Massive Spreadsheet Speed (Up to 125,000 rows/sec)",
    safiAr: "✓ فوري بأجزاء من الثانية (محرك Rust محلي)",
    safiEn: "✓ Milliseconds via native Rust + Arrow engine",
    excelAr: "يتجمّد أو ينهار مع الملفات الكبيرة",
    excelEn: "Freezes or crashes on large files",
    cloudAr: "بطيء بسبب انتظار الرفع والتحميل من المتصفح",
    cloudEn: "Slow upload/download bottlenecks"
  },
  {
    featureAr: "3. الخصوصية والعمل بدون إنترنت (100% Offline)",
    featureEn: "3. Complete Offline Privacy (100% On-Device)",
    safiAr: "✓ يعمل 100% على جهازك بلا إنترنت نهائيًا",
    safiEn: "✓ 100% Offline — files never leave your PC",
    excelAr: "محلي لكن قد يزامن تلقائيًا مع السحابة",
    excelEn: "Local, but often auto-syncs to cloud",
    cloudAr: "✗ يشترط رفع بياناتك الحساسة لخادم خارجي",
    cloudEn: "✗ Requires uploading sensitive data to third-party servers"
  },
  {
    featureAr: "4. إخفاء وتشفير البيانات الشخصية الحساسة (PII Masking)",
    featureEn: "4. Automated PII Masking & Cryptographic Protection",
    safiAr: "✓ درع إخفاء وتشفير مدمج بضغطة واحدة",
    safiEn: "✓ Built-in 1-click PII masking & hashing",
    excelAr: "غير متوفر (يتطلب حذفًا يدويًا)",
    excelEn: "Not available (manual deletion only)",
    cloudAr: "مخاطرة أمنية قبل إخفاء البيانات",
    cloudEn: "Data is exposed upon upload before masking"
  },
  {
    featureAr: "5. كشف التكرار الذكي والأسماء المتشابهة (Fuzzy Matching)",
    featureEn: "5. Smart Fuzzy Deduplication & Record Linkage",
    safiAr: "✓ يكتشف التشابه الذكي ويدمج السجلات بصريًا",
    safiEn: "✓ Smart fuzzy matching with visual merge preview",
    excelAr: "يكتشف التطابق الحرفي 100% فقط",
    excelEn: "Exact character matches only",
    cloudAr: "يتطلب اشتراكات مدفوعة وإعدادات معقدة",
    cloudEn: "Requires expensive add-ons & complex setup"
  },
  {
    featureAr: "6. فحص أرقام الهويات والضرائب والحسابات البنكية (IBAN)",
    featureEn: "6. Built-In ID, Tax Code & IBAN Checksum Validation",
    safiAr: "✓ فحص فوري واستخلاص تلقائي للبيانات",
    safiEn: "✓ Instant checksum validation & field extraction",
    excelAr: "غير متوفر بدون برمجة VBA معقدة",
    excelEn: "Not available without custom VBA scripts",
    cloudAr: "غير متوفر",
    cloudEn: "Not available"
  },
  {
    featureAr: "7. توحيد العملات والتواريخ والأرقام العالمية تلقائيًا",
    featureEn: "7. Automatic Global Currency & ISO Date Unification",
    safiAr: "✓ يوحّد كافة الصيغ والتواريخ المختلطة تلقائيًا",
    safiEn: "✓ Automatically unifies mixed dates & currencies",
    excelAr: "أخطاء شائعة في انقلاب الشهر واليوم",
    excelEn: "Frequent day/month inversion bugs",
    cloudAr: "يعتمد على إعدادات المتصفح غير المستقرة",
    cloudEn: "Unreliable browser-locale dependent parsing"
  },
  {
    featureAr: "8. إعادة استخدام خطوات التنظيف على ملفات جديدة (Recipes)",
    featureEn: "8. Reusable 1-Click Visual Cleaning Recipes",
    safiAr: "✓ احفظ الوصفة مرة واحدة وطبقها في ثانية",
    safiEn: "✓ Save once, replay on any new file in 1 click",
    excelAr: "إعادة كتابة المعادلات في كل ملف جديد",
    excelEn: "Rewrite formulas manually every month",
    cloudAr: "محدود بعدد العمليات في الباقات المجانية",
    cloudEn: "Paywalled or limited by row quotas"
  },
  {
    featureAr: "9. الحماية من ثغرات حقن المعادلات والماكرو الخبيث",
    featureEn: "9. CSV/Excel Formula-Injection & Macro Shield",
    safiAr: "✓ تحييد تلقائي لأي معادلة خبيثة وعدم تشغيل الماكرو",
    safiEn: "✓ Neutralizes formula injection; never runs macros",
    excelAr: "✗ قد ينفذ معادلات أو ماكرو ضار عند الفتح",
    excelEn: "✗ Vulnerable to macro & formula execution",
    cloudAr: "غير مضمون",
    cloudEn: "Varies by vendor"
  },
  {
    featureAr: "10. تحويل الجداول إلى رسوم بيانية تفاعلية (120 FPS)",
    featureEn: "10. Built-In 120 FPS Interactive Chart Studio",
    safiAr: "✓ استوديو رسوم بيانية فوري يدعم جميع الاتجاهات",
    safiEn: "✓ Instant 120 FPS charts with full RTL/LTR support",
    excelAr: "رسوم تقليدية تحتاج تنسيقًا يدويًا طويلًا",
    excelEn: "Dated charts requiring tedious manual styling",
    cloudAr: "يتطلب نقل البيانات لأداة تصوير خارجية",
    cloudEn: "Requires exporting to a separate BI tool"
  },
  {
    featureAr: "11. توليد تقارير PDF تنفيذية متجهية بدون إنترنت",
    featureEn: "11. Instant Offline Vector PDF Report Generation",
    safiAr: "✓ محرك PDF متجهي مدمج يصدر التقارير في ثوانٍ",
    safiEn: "✓ Embedded vector PDF engine exports in seconds",
    excelAr: "طباعة الجداول العريضة غالبًا ما تنقسم بشكل مشوّه",
    excelEn: "Wide sheets split awkwardly across pages",
    cloudAr: "بطيء وقد يغير الخطوط والتنسيق",
    cloudEn: "Slow cloud rendering with broken fonts"
  },
  {
    featureAr: "12. الحفاظ على سلامة الملف الأصلي (Non-Destructive)",
    featureEn: "12. Non-Destructive Atomic Export (Original File Safe)",
    safiAr: "✓ لا يمس الملف الأصلي أبدًا ويصدر ملفًا جديدًا آمنًا",
    safiEn: "✓ Original file is never modified; atomic new export",
    excelAr: "✗ الحفظ بالخطأ يكتب فوق الملف الأصلي مباشرة",
    excelEn: "✗ Accidental Ctrl+S overwrites source data",
    cloudAr: "نسخ متعددة مبعثرة في التنزيلات",
    cloudEn: "Scattered cloud versions"
  }
] as const;

export function BentoCommandCenter({
  onStart,
  onOpenLibrary: _onOpenLibrary,
  lang = "ar"
}: BentoCommandCenterProps): ReactNode {
  const isAr = lang === "ar";
  const gridRef = useRef<HTMLDivElement | null>(null);
  const [activeRowIdx, setActiveRowIdx] = useState<number>(0);
  const [rotatingWordIdx, setRotatingWordIdx] = useState<number>(0);
  const [activeEditorIdx, setActiveEditorIdx] = useState<number>(0);
  const [activeCategoryIdx, setActiveCategoryIdx] = useState<number>(0);
  const [showPromptSpec, setShowPromptSpec] = useState<boolean>(false);
  const [xraySplit, setXraySplit] = useState<number>(75);
  const [selectedOs, setSelectedOs] = useState<"windows" | "mac" | "linux">("windows");

  // Continuous live stepping animation for Hero, Prompt-Motion, Visual Editors & Well Organized
  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const timer = window.setInterval(() => {
      setActiveRowIdx((prev) => (prev + 1) % LIVE_DEMO_ROWS.length);
      setRotatingWordIdx((prev) => (prev + 1) % ROTATING_WORDS_AR.length);
      setActiveEditorIdx((prev) => (prev + 1) % VISUAL_EDITORS_TOOLS.length);
      setActiveCategoryIdx((prev) => (prev + 1) % WELL_ORGANIZED_CATEGORIES.length);
    }, 2400);
    return () => {
      window.clearInterval(timer);
    };
  }, []);

  // Smooth entrance animation for the Bento feature cards
  useEffect(() => {
    const grid = gridRef.current;
    if (grid === null || !canAnimate()) {
      return;
    }

    const cards = Array.from(
      grid.querySelectorAll<HTMLElement>("[data-bento-card], [data-bento-extra]")
    );
    if (cards.length === 0) {
      return;
    }

    const cascade = gsap.fromTo(
      cards,
      { opacity: 0.25, y: 18, scale: 0.99 },
      {
        opacity: 1,
        y: 0,
        scale: 1,
        duration: 0.5,
        stagger: 0.08,
        ease: "power3.out",
        clearProps: "transform,opacity"
      }
    );

    return () => {
      cascade.kill();
      gsap.set(cards, { clearProps: "transform,opacity" });
    };
  }, [lang]);

  const marqueeItems = isAr ? MARQUEE_ITEMS_AR : MARQUEE_ITEMS_EN;
  const currentRotatingWord = isAr
    ? ROTATING_WORDS_AR[rotatingWordIdx]
    : ROTATING_WORDS_EN[rotatingWordIdx];

  return (
    <div
      dir={isAr ? "rtl" : "ltr"}
      style={{ display: "flex", flexDirection: "column", gap: "36px", width: "100%" }}
    >
      {/* ====================================================================
       * 1. FULL-VIEWPORT SIDE-BY-SIDE SPLIT HERO (`#overview`)
       *    Open, unboxed layout sitting directly on the interactive background!
       * ==================================================================== */}
      <section
        id="overview"
        className="safi-rb-hero-grid"
        style={{
          paddingBlock: "clamp(16px, 2.5vw, 32px)",
          paddingInline: "clamp(8px, 1.5vw, 24px)",
          background: "transparent",
          border: "none",
          boxShadow: "none",
          position: "relative"
        }}
      >
        {/* RIGHT COLUMN: ReactBits Kinetic Typography + Rotating Word + CTAs + Live Counters */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "flex-start",
            justifyContent: "center",
            gap: "20px",
            textAlign: "start"
          }}
        >
          {/* ReactBits ShinyText Badge */}
          <div
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "8px",
              padding: "7px 16px",
              borderRadius: "999px",
              background: "var(--bg-glass)",
              backdropFilter: "blur(12px)",
              border: "1px solid var(--border-strong)",
              fontSize: "13px",
              fontWeight: 700
            }}
          >
            <Sparkles size={15} style={{ color: "var(--accent-soft)" }} />
            <span className="safi-rb-shiny-text">
              {isAr
                ? "صافي البيانات • الجيل الجديد من هندسة وتنقية البيانات"
                : "SafiData • Next-Gen Local Data Engineering Studio"}
            </span>
          </div>

          {/* ReactBits Word-Split Headline */}
          <BlurWordText
            as="h1"
            text={
              isAr
                ? "مرحبًا بك في منصة صافي البيانات — الهندسة الذكية لتنقية وحماية البيانات بجميع اللغات"
                : "Welcome to SafiData — Intelligent Universal Data Engineering, Cleaning & Privacy"
            }
            ariaLabel="مرحبًا بك في منصة صافي البيانات — الهندسة الذكية لتنقية وحماية البيانات العربية"
            stagger={0.035}
            style={{
              margin: 0,
              fontSize: "clamp(30px, 3.3vw, 52px)",
              fontWeight: 800,
              lineHeight: 1.18,
              letterSpacing: "-0.025em",
              color: "var(--text-primary)"
            }}
          />

          {/* ReactBits 3D Rotating Action Pill */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "10px",
              flexWrap: "wrap",
              fontSize: "clamp(16px, 1.4vw, 22px)",
              fontWeight: 800
            }}
          >
            <span style={{ color: "var(--text-secondary)" }}>
              {isAr ? "تطبيق واحد على جهازك:" : "One Desktop App that:"}
            </span>
            <span key={rotatingWordIdx} className="safi-rb-rotating-word">
              {currentRotatingWord}
            </span>
          </div>

          <p
            style={{
              margin: 0,
              fontSize: "clamp(15.5px, 1.2vw, 18.5px)",
              color: "var(--text-secondary)",
              lineHeight: 1.75
            }}
          >
            {isAr
              ? "حوّل جداول Excel وCSV المليئة بالأخطاء والتكرار إلى بيانات نظيفة، محمية، وجاهزة للتقارير والرسوم البيانية في ثوانٍ — يعمل 100% على جهازك بدون إنترنت."
              : "Transform messy Excel & CSV spreadsheets into clean, privacy-protected datasets and executive PDF reports in seconds — running 100% offline on your desktop."}
          </p>

          {/* ReactBits Magnetic + StarBorder CTA Buttons */}
          <div style={{ display: "flex", alignItems: "center", gap: "12px", flexWrap: "wrap" }}>
            <MagneticButton
              variant="primary"
              className="safi-rb-star-cta"
              ariaLabel="ابدأ الآن"
              onClick={() => {
                const el = document.getElementById("download");
                el?.scrollIntoView({ behavior: "smooth" });
              }}
            >
              <Download size={16} aria-hidden="true" />
              <span>{isAr ? "حمّل تطبيق سطح المكتب مجانًا" : "Download Desktop App"}</span>
            </MagneticButton>

            <MagneticButton
              variant="secondary"
              ariaLabel="استعرض القوالب"
              onClick={() => {
                const el = document.getElementById("live-motion");
                el?.scrollIntoView({ behavior: "smooth" });
              }}
            >
              <ArrowLeftRight size={16} aria-hidden="true" />
              <span>{isAr ? "جرّب المقارنة التفاعلية" : "Try Interactive Demo"}</span>
            </MagneticButton>
          </div>

          {/* 3 Open Unboxed Live Animated Metrics Sitting Directly Over the Interactive Background */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "clamp(20px, 3vw, 42px)",
              flexWrap: "wrap",
              paddingTop: "12px",
              borderTop: "1px solid var(--border-subtle)",
              width: "100%"
            }}
          >
            <CountUpMetric
              value={125000}
              decimals={0}
              suffix={isAr ? "صف/ث" : "/sec"}
              label={isAr ? "سرعة معالجة الصفوف" : "Rows Processed / Sec"}
            />

            <CountUpMetric
              value={99.9}
              decimals={1}
              suffix="%"
              label={isAr ? "دقة توحيد اللغات والبيانات" : "Universal Data Precision"}
            />

            <CountUpMetric
              value={100}
              decimals={0}
              suffix="% Local"
              label={isAr ? "يعمل بلا إنترنت" : "Zero Cloud Uploads"}
            />
          </div>
        </div>

        {/* LEFT COLUMN: ReactBits 3D Tilted Floating Studio Window (Chromeless — no outer white box!) */}
        <SpotlightCard tilt={true} padding={0} className="safi-spotlight-chromeless">
          <div
            className="safi-float-hardware"
            style={{
              borderRadius: "24px",
              background: "#0F172A",
              color: "#F8FAFC",
              border: "1px solid rgba(255, 255, 255, 0.16)",
              boxShadow: "0 32px 80px -16px rgba(15, 23, 42, 0.45)",
              overflow: "hidden",
              position: "relative",
              textAlign: "start"
            }}
          >
            {/* Continuous Laser Scan Beam */}
            <div className="safi-scan-line" aria-hidden="true" />

            {/* Window Titlebar */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "12px 18px",
                background: "rgba(255, 255, 255, 0.05)",
                borderBottom: "1px solid rgba(255, 255, 255, 0.08)"
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: "7px" }}>
                <span
                  style={{
                    width: "11px",
                    height: "11px",
                    borderRadius: "999px",
                    background: "#EF4444"
                  }}
                />
                <span
                  style={{
                    width: "11px",
                    height: "11px",
                    borderRadius: "999px",
                    background: "#F59E0B"
                  }}
                />
                <span
                  style={{
                    width: "11px",
                    height: "11px",
                    borderRadius: "999px",
                    background: "#10B981"
                  }}
                />
              </div>
              <span
                className="safi-mono"
                style={{ fontSize: "11.5px", color: "#94A3B8" }}
                dir="ltr"
              >
                SafiData Studio • Live Engine
              </span>
              <span
                style={{
                  fontSize: "11px",
                  padding: "3px 10px",
                  borderRadius: "999px",
                  background: "rgba(16, 185, 129, 0.18)",
                  color: "#34D399",
                  fontWeight: 700
                }}
              >
                {isAr ? "● فحص حي تلقائي" : "● Live Auto-Scan"}
              </span>
            </div>

            {/* Live Auto-Stepping Rows + Prompt-Motion Smart Bar + Equalizer Bar Chart */}
            <div style={{ padding: "18px", display: "flex", flexDirection: "column", gap: "12px" }}>
              {/* Prompt-Motion.com (Product-UI) Autonomous Smart Command Bar */}
              <div
                style={{
                  padding: "11px 14px",
                  borderRadius: "14px",
                  background: "rgba(30, 41, 59, 0.85)",
                  border: "1px solid rgba(96, 165, 250, 0.35)",
                  display: "flex",
                  flexDirection: "column",
                  gap: "6px"
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: "8px"
                  }}
                >
                  <span
                    style={{
                      fontSize: "11px",
                      fontWeight: 700,
                      color: "#60A5FA",
                      display: "inline-flex",
                      alignItems: "center",
                      gap: "6px"
                    }}
                  >
                    <Sparkles size={13} />
                    <span>
                      {isAr
                        ? "أوامر التنقية الذكية التلقائية (Smart Prompt UI)"
                        : "Autonomous Smart Cleaning Prompt"}
                    </span>
                  </span>
                  <span
                    key={`toast-${activeRowIdx}`}
                    className="safi-pm-step-active"
                    style={{
                      fontSize: "10.5px",
                      fontWeight: 700,
                      padding: "2px 8px",
                      borderRadius: "999px",
                      background: "rgba(16, 185, 129, 0.2)",
                      color: "#34D399"
                    }}
                  >
                    {isAr
                      ? LIVE_DEMO_ROWS[activeRowIdx]?.toastAr
                      : LIVE_DEMO_ROWS[activeRowIdx]?.toastEn}
                  </span>
                </div>
                <div style={{ fontSize: "12.5px", color: "#E2E8F0", fontWeight: 600 }}>
                  <span>
                    {isAr
                      ? LIVE_DEMO_ROWS[activeRowIdx]?.promptAr
                      : LIVE_DEMO_ROWS[activeRowIdx]?.promptEn}
                  </span>
                  <span className="safi-pm-caret" aria-hidden="true" />
                </div>
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                {LIVE_DEMO_ROWS.map((row, idx) => {
                  const isCurrent = idx === activeRowIdx;
                  return (
                    <div
                      key={row.id}
                      onClick={() => setActiveRowIdx(idx)}
                      style={{
                        padding: "11px 14px",
                        borderRadius: "14px",
                        background: isCurrent
                          ? "rgba(59, 130, 246, 0.18)"
                          : "rgba(255, 255, 255, 0.04)",
                        border: isCurrent
                          ? "1px solid rgba(96, 165, 250, 0.65)"
                          : "1px solid rgba(255, 255, 255, 0.07)",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: "10px",
                        cursor: "pointer",
                        transform: isCurrent ? "scale(1.012)" : "scale(1)",
                        transition: "all 0.35s cubic-bezier(0.22, 1, 0.36, 1)"
                      }}
                    >
                      <div>
                        <span style={{ fontSize: "11px", color: "#94A3B8", display: "block" }}>
                          {isAr ? row.labelAr : row.labelEn}
                        </span>
                        <div
                          style={{
                            display: "flex",
                            alignItems: "center",
                            gap: "8px",
                            flexWrap: "wrap",
                            marginTop: "2px"
                          }}
                        >
                          <span
                            style={{
                              fontSize: "12px",
                              color: "#94A3B8",
                              textDecoration: "line-through"
                            }}
                          >
                            {row.before}
                          </span>
                          <span style={{ color: "#60A5FA", fontWeight: 700 }}>→</span>
                          <span style={{ fontSize: "13px", fontWeight: 700, color: "#F8FAFC" }}>
                            {row.after}
                          </span>
                        </div>
                      </div>

                      <span
                        style={{
                          padding: "4px 10px",
                          borderRadius: "999px",
                          fontSize: "11px",
                          fontWeight: 700,
                          background: isCurrent
                            ? "rgba(16, 185, 129, 0.22)"
                            : "rgba(255, 255, 255, 0.07)",
                          color: isCurrent ? "#34D399" : "#CBD5E1",
                          whiteSpace: "nowrap"
                        }}
                      >
                        {isAr ? row.tagAr : row.tagEn}
                      </span>
                    </div>
                  );
                })}
              </div>

              {/* Bottom Live Equalizer & Decrypted Hash Bar */}
              <div
                style={{
                  padding: "12px 16px",
                  borderRadius: "14px",
                  background: "rgba(255, 255, 255, 0.04)",
                  border: "1px solid rgba(255, 255, 255, 0.08)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: "12px"
                }}
              >
                <div>
                  <span style={{ fontSize: "11px", color: "#94A3B8", display: "block" }}>
                    {isAr ? "بصمة التحقق الفورية (Decrypted Hash)" : "Live Verification Digest"}
                  </span>
                  <div
                    style={{
                      color: "#60A5FA",
                      fontWeight: 700,
                      fontSize: "13px",
                      marginTop: "2px"
                    }}
                  >
                    <DecryptedMonoText value={`safi:verified:0${activeRowIdx + 1}:9f4a8e2c`} />
                  </div>
                </div>

                <svg width="120" height="46" viewBox="0 0 120 46" aria-hidden="true">
                  <rect
                    className="safi-bar-anim-1"
                    x="8"
                    y="18"
                    width="18"
                    height="26"
                    rx="4"
                    fill="#64748B"
                  />
                  <rect
                    className="safi-bar-anim-2"
                    x="36"
                    y="10"
                    width="18"
                    height="34"
                    rx="4"
                    fill="#94A3B8"
                  />
                  <rect
                    className="safi-bar-anim-3"
                    x="64"
                    y="4"
                    width="18"
                    height="40"
                    rx="4"
                    fill="#60A5FA"
                  />
                  <rect
                    className="safi-bar-anim-1"
                    x="92"
                    y="8"
                    width="18"
                    height="36"
                    rx="4"
                    fill="#34D399"
                  />
                </svg>
              </div>
            </div>
          </div>
        </SpotlightCard>
      </section>

      {/* ====================================================================
       * 2. CONTINUOUS INFINITE SMOOTH MARQUEE TICKER (Open on Interactive BG)
       * ==================================================================== */}
      <div
        style={{
          overflow: "hidden",
          paddingBlock: "8px",
          background: "transparent",
          border: "none",
          boxShadow: "none"
        }}
        dir="ltr"
      >
        <div className="safi-marquee-track">
          {[...marqueeItems, ...marqueeItems].map((item, idx) => (
            <span
              key={`mq-${idx}`}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "10px",
                padding: "8px 18px",
                borderRadius: "999px",
                background: "var(--bg-glass)",
                backdropFilter: "blur(10px)",
                border: "1px solid var(--border-subtle)",
                color: "var(--text-primary)",
                fontSize: "13px",
                fontWeight: 600,
                whiteSpace: "nowrap"
              }}
            >
              <CheckCircle2 size={14} style={{ color: "var(--status-ok)" }} />
              <span>{item}</span>
            </span>
          ))}
        </div>
      </div>

      {/* ====================================================================
       * 2.5. REACTBITS.DEV FLAGSHIP TRINITY + PROMPT-MOTION PRODUCT-UI
       *      1) Prompt-Motion 4-Scene Autonomous Product-UI Showreel Stage
       *      2) Visual Editors (Five Built-In Interactive Studios)
       *      3) Growing Fast (Blazing Velocity Trajectory — Not Even Close)
       *      4) Well Organized (Five Clear Categories — Zero Clutter)
       * ==================================================================== */}
      <section id="prompt-motion" style={{ display: "flex", flexDirection: "column", gap: "22px" }}>
        {/* Open Unboxed Header Sitting Directly Over the Interactive Background */}
        <div
          style={{
            paddingInline: "8px",
            display: "flex",
            alignItems: "flex-end",
            justifyContent: "space-between",
            flexWrap: "wrap",
            gap: "14px"
          }}
        >
          <div>
            <h2
              style={{ fontSize: "clamp(24px, 2.6vw, 34px)", fontWeight: 800, margin: "0 0 6px" }}
            >
              {isAr
                ? "استوديو الموشن التفاعلي: تجربة هندسية بمعايير عالمية"
                : "Interactive Motion Studio: Visual Editors, Blazing Speed & Clear Organization"}
            </h2>
            <p style={{ margin: 0, fontSize: "15.5px", color: "var(--text-secondary)" }}>
              {isAr
                ? "محاكاة حية لواجهة المنتج (Product-UI Motion)، خمس أدوات بصرية مدمجة، وسرعة معالجة لا تقبل المنافسة"
                : "Autonomous Product-UI motion choreography, five integrated visual editors, and unmatched processing velocity"}
            </p>
          </div>

          <button
            type="button"
            onClick={() => setShowPromptSpec((prev) => !prev)}
            className="safi-btn safi-btn-secondary"
            style={{ minHeight: "38px", padding: "6px 16px", fontSize: "12.5px" }}
          >
            <Sparkles size={14} style={{ color: "var(--accent-soft)" }} />
            <span>
              {showPromptSpec
                ? isAr
                  ? "إخفاء برومبت الموشن (Prompt Spec)"
                  : "Hide Prompt-Motion Spec"
                : isAr
                  ? "استعراض برومبت الموشن الجاهز (Prompt-Motion)"
                  : "View Ready Prompt-Motion Spec"}
            </span>
          </button>
        </div>

        {/* Optional Ready-to-Use Prompt-Motion Specification Drawer */}
        {showPromptSpec ? (
          <div
            className="safi-pm-step-active"
            style={{
              padding: "20px 24px",
              borderRadius: "22px",
              background: "#090D16",
              color: "#E2E8F0",
              border: "1px solid rgba(96, 165, 250, 0.45)",
              fontFamily: "var(--safi-font-mono)",
              fontSize: "12.5px",
              lineHeight: 1.7,
              direction: "ltr",
              textAlign: "left"
            }}
          >
            <div style={{ color: "#60A5FA", fontWeight: 800, marginBottom: "6px" }}>
              {
                "<direction> Prompt-Motion Product-UI Showreel Blueprint (124 BPM • 4 Continuous Scenes) </direction>"
              }
            </div>
            <div>
              {
                "S1 (f0-71): Header lockup + Hub window rises with Ice-Blue floor glow; macOS cursor (32x39 black arrow, white rim) glides in & clicks the orbital disc."
              }
            </div>
            <div>
              {
                "S2 (f72-100): Squircle emblem (Midnight-Slate) + 4 orbiting satellites (Pure-White left, Cobalt-Blue top, Sky-Cyan right, Emerald-Mint bottom) lock into a precision cross."
              }
            </div>
            <div>
              {
                "S3 (f101-158): Frosted-glass command prompt springs open; 1-char/frame typewriter + active caret + circular Cobalt-Blue up-arrow send trigger."
              }
            </div>
            <div>
              {
                "S4 (f159-244): Two stacked frosted cards rise with blur(12px)->0px easing, revealing 125,000 rows/sec purified output & instant vector PDF."
              }
            </div>
          </div>
        ) : null}

        {/* PROMPT-MOTION 4-SCENE AUTONOMOUS PRODUCT-UI STAGE (With Live Gliding macOS Cursor & Orbital Logo) */}
        <SpotlightCard tilt={false} padding={24}>
          <div
            style={{
              position: "relative",
              borderRadius: "22px",
              background: "linear-gradient(135deg, #090D16 0%, #111827 55%, #0F172A 100%)",
              color: "#F8FAFC",
              padding: "clamp(18px, 2.4vw, 28px)",
              border: "1px solid rgba(255, 255, 255, 0.14)",
              overflow: "hidden"
            }}
          >
            {/* Autonomous Gliding SVG macOS Cursor (Prompt-Motion Signature) */}
            <div
              aria-hidden="true"
              style={{
                position: "absolute",
                top:
                  activeRowIdx === 0
                    ? "26%"
                    : activeRowIdx === 1
                      ? "48%"
                      : activeRowIdx === 2
                        ? "66%"
                        : "36%",
                left:
                  activeRowIdx === 0
                    ? "22%"
                    : activeRowIdx === 1
                      ? "48%"
                      : activeRowIdx === 2
                        ? "74%"
                        : "58%",
                zIndex: 40,
                pointerEvents: "none",
                transition: "all 0.85s cubic-bezier(0.22, 1, 0.36, 1)",
                filter: "drop-shadow(0 8px 16px rgba(0,0,0,0.55))"
              }}
            >
              <svg width="28" height="34" viewBox="0 0 32 39" fill="none">
                <path
                  d="M3 2.5L28.5 18.2L17.2 20.6L22.8 33.4L17.8 35.8L12.2 22.9L4.5 30.2V2.5Z"
                  fill="#090A0F"
                  stroke="#FFFFFF"
                  strokeWidth="2.2"
                  strokeLinejoin="round"
                />
              </svg>
            </div>

            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
                gap: "20px",
                alignItems: "center"
              }}
            >
              {/* Scene S2: Orbital Emblem + 4 Satellites Lockup */}
              <div style={{ display: "flex", alignItems: "center", gap: "16px" }}>
                <div
                  style={{
                    position: "relative",
                    width: "76px",
                    height: "76px",
                    borderRadius: "22px",
                    background: "rgba(30, 41, 59, 0.9)",
                    border: "1px solid rgba(96, 165, 250, 0.45)",
                    boxShadow: "0 0 32px rgba(59, 130, 246, 0.28)",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    flexShrink: 0
                  }}
                >
                  <svg width="62" height="62" viewBox="0 0 128 128" fill="none" aria-hidden="true">
                    <g className="safi-pm-logo-orbit" style={{ transformOrigin: "64px 64px" }}>
                      <circle
                        cx="64"
                        cy="64"
                        r="56"
                        stroke="#60A5FA"
                        strokeOpacity="0.45"
                        strokeWidth="2"
                        strokeDasharray="6 6"
                      />
                      <rect x="59" y="2" width="10" height="10" rx="3" fill="#2F6BFF" />
                      <rect x="116" y="59" width="10" height="10" rx="3" fill="#3CC8F0" />
                      <rect x="59" y="116" width="10" height="10" rx="3" fill="#4ED6A0" />
                      <rect x="2" y="59" width="10" height="10" rx="3" fill="#F8FAFC" />
                    </g>
                    <rect x="28" y="66" width="16" height="32" rx="4" fill="#64748B" />
                    <rect x="56" y="48" width="16" height="50" rx="4" fill="#94A3B8" />
                    <rect x="84" y="30" width="16" height="68" rx="4" fill="#F8FAFC" />
                    <path
                      d="M32 54L62 36L92 20"
                      stroke="#34D399"
                      strokeWidth="5"
                      strokeLinecap="round"
                    />
                  </svg>
                </div>

                <div>
                  <span
                    className="safi-mono"
                    dir="ltr"
                    style={{ fontSize: "11px", color: "#3CC8F0", fontWeight: 800 }}
                  >
                    SCENE 0{activeRowIdx + 1} / 04 • 124 BPM
                  </span>
                  <div style={{ fontSize: "18px", fontWeight: 800, marginTop: "2px" }}>
                    {isAr
                      ? LIVE_DEMO_ROWS[activeRowIdx]?.labelAr
                      : LIVE_DEMO_ROWS[activeRowIdx]?.labelEn}
                  </div>
                  <div style={{ fontSize: "12.5px", color: "#94A3B8", marginTop: "3px" }}>
                    {isAr
                      ? LIVE_DEMO_ROWS[activeRowIdx]?.toastAr
                      : LIVE_DEMO_ROWS[activeRowIdx]?.toastEn}
                  </div>
                </div>
              </div>

              {/* Scene S3: Frosted Glass Prompt Box + #2F6BFF Send Arrow Button */}
              <div
                style={{
                  padding: "14px 18px",
                  borderRadius: "18px",
                  background: "rgba(255, 255, 255, 0.07)",
                  backdropFilter: "blur(16px)",
                  border: "1px solid rgba(255, 255, 255, 0.18)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: "12px"
                }}
              >
                <div>
                  <span
                    style={{
                      fontSize: "11px",
                      color: "#94A3B8",
                      display: "block",
                      marginBottom: "3px"
                    }}
                  >
                    {isAr ? "أمر المعالجة التلقائي (Prompt-Motion S3)" : "Prompt-Motion S3 Command"}
                  </span>
                  <span style={{ fontSize: "13.5px", fontWeight: 700, color: "#F8FAFC" }}>
                    {isAr
                      ? LIVE_DEMO_ROWS[activeRowIdx]?.promptAr
                      : LIVE_DEMO_ROWS[activeRowIdx]?.promptEn}
                  </span>
                  <span className="safi-pm-caret" aria-hidden="true" />
                </div>

                <button
                  type="button"
                  onClick={() => setActiveRowIdx((prev) => (prev + 1) % LIVE_DEMO_ROWS.length)}
                  aria-label="المشهد التالي"
                  style={{
                    width: "38px",
                    height: "38px",
                    borderRadius: "999px",
                    border: "none",
                    background: "#2F6BFF",
                    color: "#FFFFFF",
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                    cursor: "pointer",
                    flexShrink: 0,
                    boxShadow: "0 0 20px rgba(47, 107, 255, 0.55)"
                  }}
                >
                  ↑
                </button>
              </div>
            </div>
          </div>
        </SpotlightCard>

        <div className="safi-rb-showcase-grid">
          {/* CARD 1: VISUAL EDITORS (Span 7) */}
          <div className="safi-rb-col-7">
            <SpotlightCard tilt={true} padding={28}>
              <div
                style={{ display: "flex", flexDirection: "column", gap: "18px", height: "100%" }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "flex-start",
                    justifyContent: "space-between",
                    gap: "12px",
                    flexWrap: "wrap"
                  }}
                >
                  <div>
                    <span
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: "6px",
                        padding: "4px 12px",
                        borderRadius: "999px",
                        background: "var(--accent-tint)",
                        color: "var(--accent-soft)",
                        fontSize: "12px",
                        fontWeight: 800,
                        marginBottom: "8px"
                      }}
                    >
                      <Sparkles size={13} />
                      <span>
                        {isAr
                          ? "Visual Editors • محررات بصرية تفاعلية"
                          : "Visual Editors • 5 Interactive Studios"}
                      </span>
                    </span>
                    <h3 style={{ fontSize: "23px", fontWeight: 800, margin: "0 0 6px" }}>
                      {isAr
                        ? "خمس أدوات بصرية متكاملة للتعامل مع بياناتك وإنتاج تقارير مبهرة"
                        : "Five Built-In Visual Tools to Clean, Inspect & Create Stunning Data Assets"}
                    </h3>
                    <p
                      style={{
                        margin: 0,
                        fontSize: "14.5px",
                        color: "var(--text-secondary)",
                        lineHeight: 1.65
                      }}
                    >
                      {isAr
                        ? "تنقّل بين الأدوات الخمس لترى كيف يمنحك التطبيق تحكمًا بصريًا فوريًا في كل تفصيلة بدون كتابة معادلة واحدة."
                        : "Five specialized visual tools to play with your spreadsheets and create clean, publication-ready assets for your work."}
                    </p>
                  </div>
                </div>

                {/* 5 Interactive Tool Selector Pills */}
                <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
                  {VISUAL_EDITORS_TOOLS.map((tool, idx) => {
                    const active = idx === activeEditorIdx;
                    return (
                      <button
                        key={tool.id}
                        type="button"
                        onClick={() => setActiveEditorIdx(idx)}
                        style={{
                          padding: "8px 14px",
                          borderRadius: "999px",
                          fontSize: "12.5px",
                          fontWeight: 700,
                          cursor: "pointer",
                          border: active
                            ? "1px solid var(--accent-soft)"
                            : "1px solid var(--border-subtle)",
                          background: active ? "var(--accent-primary)" : "var(--bg-elevated)",
                          color: active ? "#FFFFFF" : "var(--text-primary)",
                          transition: "all 0.25s ease"
                        }}
                      >
                        {isAr ? tool.badgeAr : tool.badgeEn}
                      </button>
                    );
                  })}
                </div>

                {/* Live Prompt-Motion Interactive Visual Editor Preview Canvas */}
                <div
                  key={`editor-canvas-${activeEditorIdx}`}
                  className="safi-pm-step-active"
                  style={{
                    padding: "20px",
                    borderRadius: "20px",
                    background: "#0F172A",
                    color: "#F8FAFC",
                    border: "1px solid rgba(255, 255, 255, 0.14)",
                    display: "flex",
                    flexDirection: "column",
                    gap: "14px"
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      flexWrap: "wrap",
                      gap: "10px"
                    }}
                  >
                    <div>
                      <div style={{ fontSize: "17px", fontWeight: 800, color: "#F8FAFC" }}>
                        {isAr
                          ? VISUAL_EDITORS_TOOLS[activeEditorIdx]?.titleAr
                          : VISUAL_EDITORS_TOOLS[activeEditorIdx]?.titleEn}
                      </div>
                      <div style={{ fontSize: "13px", color: "#94A3B8", marginTop: "3px" }}>
                        {isAr
                          ? VISUAL_EDITORS_TOOLS[activeEditorIdx]?.descAr
                          : VISUAL_EDITORS_TOOLS[activeEditorIdx]?.descEn}
                      </div>
                    </div>
                    <span
                      className="safi-mono"
                      dir="ltr"
                      style={{
                        padding: "5px 12px",
                        borderRadius: "999px",
                        background: "rgba(59, 130, 246, 0.2)",
                        color: "#60A5FA",
                        fontSize: "12px",
                        fontWeight: 700
                      }}
                    >
                      {VISUAL_EDITORS_TOOLS[activeEditorIdx]?.metric}
                    </span>
                  </div>

                  <div
                    style={{
                      display: "grid",
                      gridTemplateColumns: "repeat(auto-fit, minmax(210px, 1fr))",
                      gap: "12px"
                    }}
                  >
                    <div
                      style={{
                        padding: "12px 14px",
                        borderRadius: "14px",
                        background: "rgba(255, 255, 255, 0.04)",
                        border: "1px solid rgba(255, 255, 255, 0.08)"
                      }}
                    >
                      <span
                        style={{
                          fontSize: "11px",
                          color: "#94A3B8",
                          display: "block",
                          marginBottom: "4px"
                        }}
                      >
                        {isAr ? "المدخل الخام قبل الأداة" : "Raw Input"}
                      </span>
                      <span
                        style={{
                          fontSize: "13.5px",
                          color: "#CBD5E1",
                          textDecoration: "line-through"
                        }}
                      >
                        {VISUAL_EDITORS_TOOLS[activeEditorIdx]?.sampleBefore}
                      </span>
                    </div>

                    <div
                      style={{
                        padding: "12px 14px",
                        borderRadius: "14px",
                        background: "rgba(16, 185, 129, 0.14)",
                        border: "1px solid rgba(52, 211, 153, 0.4)"
                      }}
                    >
                      <span
                        style={{
                          fontSize: "11px",
                          color: "#34D399",
                          fontWeight: 700,
                          display: "block",
                          marginBottom: "4px"
                        }}
                      >
                        {isAr ? "النتيجة الفورية بعد المعالجة" : "Instant Visual Output"}
                      </span>
                      <span style={{ fontSize: "14px", fontWeight: 800, color: "#F8FAFC" }}>
                        ✓ {VISUAL_EDITORS_TOOLS[activeEditorIdx]?.sampleAfter}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </SpotlightCard>
          </div>

          {/* CARD 2: GROWING FAST (Span 5) */}
          <div className="safi-rb-col-5">
            <SpotlightCard tilt={true} padding={28}>
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  gap: "16px",
                  height: "100%"
                }}
              >
                <div>
                  <span
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: "6px",
                      padding: "4px 12px",
                      borderRadius: "999px",
                      background: "rgba(16, 185, 129, 0.14)",
                      color: "var(--status-ok)",
                      fontSize: "12px",
                      fontWeight: 800,
                      marginBottom: "8px"
                    }}
                  >
                    <Zap size={13} />
                    <span>
                      {isAr
                        ? "Growing Fast • سرعة وأداء بلا منافس"
                        : "Growing Fast • Blazing Engine"}
                    </span>
                  </span>
                  <h3 style={{ fontSize: "23px", fontWeight: 800, margin: "0 0 6px" }}>
                    {isAr
                      ? "الأسرع في معالجة الجداول الضخمة بجميع اللغات — بفارق لا يُقارن"
                      : "Fastest Universal Local Data Engine. Not Even Close."}
                  </h3>
                  <p
                    style={{
                      margin: 0,
                      fontSize: "14.5px",
                      color: "var(--text-secondary)",
                      lineHeight: 1.65
                    }}
                  >
                    {isAr
                      ? "يعالج 125,000 صف في الثانية الواحدة محليًا بفضل نواة Rust وApache Arrow دون أي تجمّد."
                      : "Processes 125,000 rows/sec locally with native Rust & Apache Arrow — leaving traditional spreadsheets far behind."}
                  </p>
                </div>

                {/* Live Animated Rising Velocity Trajectory SVG */}
                <div
                  style={{
                    padding: "16px",
                    borderRadius: "20px",
                    background: "#0F172A",
                    border: "1px solid rgba(255, 255, 255, 0.12)"
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      marginBottom: "8px"
                    }}
                  >
                    <span style={{ fontSize: "12px", color: "#94A3B8", fontWeight: 700 }}>
                      {isAr ? "منحنى سرعة المعالجة القياسي" : "Throughput Velocity Curve"}
                    </span>
                    <span
                      className="safi-mono"
                      dir="ltr"
                      style={{ fontSize: "13px", fontWeight: 800, color: "#34D399" }}
                    >
                      125,000 rows/s ▲
                    </span>
                  </div>

                  <svg viewBox="0 0 360 150" width="100%" height="145" aria-hidden="true">
                    <defs>
                      <linearGradient id="safiFastArea" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor="#3B82F6" stopOpacity="0.45" />
                        <stop offset="100%" stopColor="#10B981" stopOpacity="0.0" />
                      </linearGradient>
                    </defs>

                    {/* Subtle Grid Lines */}
                    <line
                      x1="20"
                      y1="35"
                      x2="340"
                      y2="35"
                      stroke="rgba(255,255,255,0.07)"
                      strokeDasharray="4 4"
                    />
                    <line
                      x1="20"
                      y1="80"
                      x2="340"
                      y2="80"
                      stroke="rgba(255,255,255,0.07)"
                      strokeDasharray="4 4"
                    />
                    <line x1="20" y1="125" x2="340" y2="125" stroke="rgba(255,255,255,0.12)" />

                    {/* Traditional Spreadsheets Flat Grey Line */}
                    <path
                      d="M 20 122 Q 140 120, 335 116"
                      fill="none"
                      stroke="#64748B"
                      strokeWidth="2"
                      strokeDasharray="5 5"
                    />

                    {/* SafiData Exponential Rising Area & Flowing Trajectory Curve */}
                    <path
                      d="M 20 122 C 110 118, 185 92, 250 52 C 285 30, 312 22, 335 18 L 335 125 L 20 125 Z"
                      fill="url(#safiFastArea)"
                    />
                    <path
                      d="M 20 122 C 110 118, 185 92, 250 52 C 285 30, 312 22, 335 18"
                      fill="none"
                      stroke="#60A5FA"
                      strokeWidth="3.5"
                      strokeLinecap="round"
                    />
                    <path
                      className="safi-rb-trajectory-line"
                      d="M 20 122 C 110 118, 185 92, 250 52 C 285 30, 312 22, 335 18"
                      fill="none"
                      stroke="#34D399"
                      strokeWidth="3.5"
                      strokeLinecap="round"
                    />

                    {/* Peak Radar Beacon */}
                    <circle
                      className="safi-rb-beacon-ring"
                      cx="335"
                      cy="18"
                      r="9"
                      fill="rgba(52, 211, 153, 0.45)"
                    />
                    <circle
                      cx="335"
                      cy="18"
                      r="5.5"
                      fill="#34D399"
                      stroke="#0F172A"
                      strokeWidth="2"
                    />
                  </svg>

                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      fontSize: "11.5px",
                      color: "#94A3B8",
                      marginTop: "4px"
                    }}
                  >
                    <span>
                      {isAr ? "● صافي البيانات (الأسرع 40×)" : "● SafiData Engine (40x Faster)"}
                    </span>
                    <span>{isAr ? "○ الطرق التقليدية" : "○ Traditional Tools"}</span>
                  </div>
                </div>
              </div>
            </SpotlightCard>
          </div>

          {/* CARD 3: WELL ORGANIZED (Span 12 — Five Clear Categories + 3D Stacked Card Swap) */}
          <div className="safi-rb-col-12">
            <SpotlightCard tilt={false} padding={28}>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fit, minmax(320px, 1fr))",
                  gap: "28px",
                  alignItems: "center"
                }}
              >
                {/* Right Column: Five Clear Categories Selector */}
                <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
                  <div>
                    <span
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: "6px",
                        padding: "4px 12px",
                        borderRadius: "999px",
                        background: "var(--accent-tint)",
                        color: "var(--accent-soft)",
                        fontSize: "12px",
                        fontWeight: 800,
                        marginBottom: "8px"
                      }}
                    >
                      <CheckCircle2 size={13} />
                      <span>
                        {isAr
                          ? "Well Organized • منظّم ببراعة"
                          : "Well Organized • Five Clear Categories"}
                      </span>
                    </span>
                    <h3 style={{ fontSize: "23px", fontWeight: 800, margin: "0 0 6px" }}>
                      {isAr
                        ? "خمس فئات واضحة ومنظمة حتى لا تغرق في فوضى القوائم المعقدة"
                        : "Five Clear Categories So You're Never Scrolling Through a Wall of Unrelated Stuff"}
                    </h3>
                    <p
                      style={{
                        margin: "0 0 8px",
                        fontSize: "14.5px",
                        color: "var(--text-secondary)"
                      }}
                    >
                      {isAr
                        ? "كل مرحلة في صافي مصممة بترتيب منطقي واضح يأخذ ملفك من الفوضى إلى التقرير النهائي بسلاسة."
                        : "Every workflow stage is cleanly organized into five logical categories with instant visual feedback."}
                    </p>
                  </div>

                  <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                    {WELL_ORGANIZED_CATEGORIES.map((cat, idx) => {
                      const active = idx === activeCategoryIdx;
                      return (
                        <div
                          key={cat.id}
                          onClick={() => setActiveCategoryIdx(idx)}
                          onPointerEnter={() => setActiveCategoryIdx(idx)}
                          style={{
                            padding: "11px 16px",
                            borderRadius: "16px",
                            cursor: "pointer",
                            display: "flex",
                            alignItems: "center",
                            justifyContent: "space-between",
                            gap: "12px",
                            background: active ? "var(--accent-primary)" : "var(--bg-elevated)",
                            color: active ? "#FFFFFF" : "var(--text-primary)",
                            border: active
                              ? "1px solid var(--accent-soft)"
                              : "1px solid var(--border-subtle)",
                            transform: active ? "translateX(-4px)" : "none",
                            transition: "all 0.28s cubic-bezier(0.22, 1, 0.36, 1)"
                          }}
                        >
                          <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                            <span
                              className="safi-mono"
                              dir="ltr"
                              style={{
                                fontSize: "12px",
                                fontWeight: 800,
                                padding: "4px 8px",
                                borderRadius: "8px",
                                background: active ? "rgba(255,255,255,0.16)" : "var(--bg-surface)"
                              }}
                            >
                              {cat.code}
                            </span>
                            <span style={{ fontSize: "14.5px", fontWeight: 700 }}>
                              {isAr ? cat.titleAr : cat.titleEn}
                            </span>
                          </div>
                          <span
                            style={{
                              fontSize: "11.5px",
                              fontWeight: 700,
                              opacity: active ? 0.95 : 0.7
                            }}
                          >
                            {isAr ? cat.tagAr : cat.tagEn}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                </div>

                {/* Left Column: ReactBits 3D CardSwap Stacked Deck Preview */}
                <div
                  style={{
                    position: "relative",
                    minHeight: "260px",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    perspective: "1000px"
                  }}
                >
                  {WELL_ORGANIZED_CATEGORIES.map((cat, idx) => {
                    const offset =
                      (idx - activeCategoryIdx + WELL_ORGANIZED_CATEGORIES.length) %
                      WELL_ORGANIZED_CATEGORIES.length;
                    const isFront = offset === 0;
                    const isSecond = offset === 1;
                    const isThird = offset === 2;
                    if (!isFront && !isSecond && !isThird) {
                      return null;
                    }
                    const translateY = isFront ? 0 : isSecond ? -18 : -34;
                    const scale = isFront ? 1 : isSecond ? 0.94 : 0.88;
                    const opacity = isFront ? 1 : isSecond ? 0.65 : 0.35;
                    const zIndex = isFront ? 30 : isSecond ? 20 : 10;

                    return (
                      <div
                        key={`stack-${cat.id}`}
                        onClick={() =>
                          setActiveCategoryIdx(
                            (prev) => (prev + 1) % WELL_ORGANIZED_CATEGORIES.length
                          )
                        }
                        style={{
                          position: "absolute",
                          width: "100%",
                          maxWidth: "460px",
                          padding: "24px",
                          borderRadius: "22px",
                          background: "#0F172A",
                          color: "#F8FAFC",
                          border: isFront
                            ? "1px solid rgba(96, 165, 250, 0.6)"
                            : "1px solid rgba(255, 255, 255, 0.12)",
                          boxShadow: "0 24px 54px -14px rgba(15, 23, 42, 0.45)",
                          transform: `translateY(${translateY}px) scale(${scale})`,
                          opacity,
                          zIndex,
                          cursor: "pointer",
                          transition: "all 0.45s cubic-bezier(0.22, 1, 0.36, 1)"
                        }}
                      >
                        <div
                          style={{
                            display: "flex",
                            alignItems: "center",
                            justifyContent: "space-between",
                            marginBottom: "14px"
                          }}
                        >
                          <span
                            className="safi-mono"
                            dir="ltr"
                            style={{
                              padding: "4px 12px",
                              borderRadius: "999px",
                              background: "rgba(59, 130, 246, 0.2)",
                              color: "#60A5FA",
                              fontSize: "12px",
                              fontWeight: 800
                            }}
                          >
                            CATEGORY {cat.code} / 05
                          </span>
                          <span style={{ fontSize: "12px", color: "#34D399", fontWeight: 700 }}>
                            ✓ {isAr ? cat.tagAr : cat.tagEn}
                          </span>
                        </div>

                        <h4
                          style={{
                            fontSize: "20px",
                            fontWeight: 800,
                            margin: "0 0 10px",
                            color: "#F8FAFC"
                          }}
                        >
                          {isAr ? cat.titleAr : cat.titleEn}
                        </h4>
                        <p
                          style={{
                            margin: "0 0 16px",
                            fontSize: "14px",
                            color: "#CBD5E1",
                            lineHeight: 1.7
                          }}
                        >
                          {isAr ? cat.subtitleAr : cat.subtitleEn}
                        </p>

                        <div
                          style={{
                            display: "flex",
                            alignItems: "center",
                            justifyContent: "space-between",
                            paddingTop: "12px",
                            borderTop: "1px solid rgba(255, 255, 255, 0.08)",
                            fontSize: "12px",
                            color: "#94A3B8"
                          }}
                        >
                          <span>
                            {isAr
                              ? "اضغط للتبديل بين الفئات الخمس"
                              : "Click to cycle through all 5 categories"}
                          </span>
                          <span style={{ color: "#60A5FA", fontWeight: 700 }}>→</span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </SpotlightCard>
          </div>
        </div>
      </section>

      {/* ====================================================================
       * 3. INTERACTIVE BEFORE / AFTER X-RAY SLIDER (`#live-motion`)
       *    Heading & controls sit directly on the interactive background!
       * ==================================================================== */}
      <section id="live-motion" style={{ display: "flex", flexDirection: "column", gap: "18px" }}>
        <div
          style={{
            display: "flex",
            alignItems: "flex-end",
            justifyContent: "space-between",
            flexWrap: "wrap",
            gap: "16px",
            paddingInline: "8px"
          }}
        >
          <div>
            <h2
              style={{ fontSize: "clamp(22px, 2.4vw, 30px)", fontWeight: 800, margin: "0 0 6px" }}
            >
              {isAr
                ? "جرّب بنفسك: عدسة المقارنة قبل وبعد التنظيف"
                : "Try It Live: Before & After Data Cleaning Lens"}
            </h2>
            <p style={{ margin: 0, fontSize: "15px", color: "var(--text-secondary)" }}>
              {isAr
                ? "حرّك المؤشر لترى كيف يعالج تطبيق صافي الأخطاء الشائعة في الجداول بمختلف اللغات والصيغ"
                : "Slide to compare raw messy spreadsheet rows against clean, standardized records"}
            </p>
          </div>

          <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
            <button
              type="button"
              className="safi-btn safi-btn-secondary"
              onClick={() => setXraySplit(0)}
              style={{ minHeight: "40px", padding: "6px 16px", fontSize: "13px" }}
            >
              {isAr ? "قبل التنظيف (0%)" : "Raw Data (0%)"}
            </button>
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={() => setXraySplit(100)}
              style={{ minHeight: "40px", padding: "6px 16px", fontSize: "13px" }}
            >
              {isAr ? "بعد التنظيف (100%)" : "Cleaned (100%)"}
            </button>
          </div>
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "16px",
            padding: "10px 16px",
            borderRadius: "999px",
            background: "var(--bg-glass)",
            backdropFilter: "blur(10px)",
            border: "1px solid var(--border-subtle)"
          }}
        >
          <span style={{ fontSize: "13px", fontWeight: 700, whiteSpace: "nowrap" }}>
            {isAr ? `نسبة التنقية: ${xraySplit}%` : `Purification: ${xraySplit}%`}
          </span>
          <input
            type="range"
            min={0}
            max={100}
            value={xraySplit}
            onChange={(e) => setXraySplit(Number(e.target.value))}
            aria-label="نسبة مقارنة البيانات"
            style={{ width: "100%", cursor: "pointer" }}
          />
        </div>

        <SpotlightCard padding={20}>
          <div style={{ overflowX: "auto" }}>
            <table>
              <thead>
                <tr>
                  <th>{isAr ? "نوع البيانات" : "Field"}</th>
                  <th>{isAr ? "في الملف الأصلي (قبل)" : "Original Messy Input"}</th>
                  <th>{isAr ? "بعد المعالجة في صافي (بعد)" : "Clean SafiData Output"}</th>
                </tr>
              </thead>
              <tbody>
                {LIVE_DEMO_ROWS.map((row) => (
                  <tr key={row.id}>
                    <td style={{ fontWeight: 700 }}>{isAr ? row.labelAr : row.labelEn}</td>
                    <td
                      style={{
                        opacity: xraySplit > 65 ? 0.45 : 1,
                        textDecoration: xraySplit > 65 ? "line-through" : "none",
                        transition: "opacity 0.25s ease"
                      }}
                    >
                      {row.before}
                    </td>
                    <td
                      style={{
                        fontWeight: 700,
                        background: xraySplit >= 50 ? "var(--accent-tint)" : "transparent",
                        transition: "background-color 0.25s ease"
                      }}
                    >
                      ✓ {row.after}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </SpotlightCard>
      </section>

      {/* ====================================================================
       * 4. APPLE BENTO FEATURES GRID (`#features` — Open Header + 8 Powerhouse Cards)
       * ==================================================================== */}
      <section
        id="features"
        aria-label="المميزات الرئيسية"
        style={{ display: "flex", flexDirection: "column", gap: "18px" }}
      >
        <div style={{ paddingInline: "8px" }}>
          <h2 style={{ fontSize: "clamp(22px, 2.4vw, 30px)", fontWeight: 800, margin: "0 0 6px" }}>
            {isAr
              ? "إمكانيات هندسية متكاملة داخل تطبيق واحد لجميع اللغات"
              : "Complete Universal Data Engineering Capabilities in One App"}
          </h2>
          <p style={{ margin: 0, fontSize: "15px", color: "var(--text-secondary)" }}>
            {isAr
              ? "كل ما تحتاجه لتنظيف الجداول، كشف التكرار، حماية الخصوصية، وتوليد التقارير العالمية بدقة عالية"
              : "Everything you need to clean spreadsheets, deduplicate records, protect privacy, and generate executive reports"}
          </p>
        </div>

        <div ref={gridRef} className="safi-bento">
          {FEATURES_LIST.map((feat, idx) => {
            const Icon = feat.icon;
            const spanClass =
              idx === 0 || idx === 6
                ? "safi-bento-wide"
                : idx === 1 || idx === 7
                  ? "safi-bento-half"
                  : "safi-bento-third";
            const cardAttr =
              idx < 4
                ? { "data-bento-card": "true" as const }
                : { "data-bento-extra": "true" as const };
            return (
              <div key={feat.id} className={spanClass} {...cardAttr}>
                <SpotlightCard tilt={true} padding={28}>
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: "14px",
                      height: "100%"
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between"
                      }}
                    >
                      <span
                        style={{
                          width: "44px",
                          height: "44px",
                          borderRadius: "14px",
                          background: "var(--bg-elevated)",
                          display: "inline-flex",
                          alignItems: "center",
                          justifyContent: "center"
                        }}
                      >
                        <Icon size={22} style={{ color: "var(--accent-soft)" }} />
                      </span>
                      <div style={{ textAlign: "end" }}>
                        <div
                          className="safi-mono"
                          style={{ fontSize: "24px", fontWeight: 800 }}
                          dir="ltr"
                        >
                          {feat.metric}
                        </div>
                        <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
                          {isAr ? feat.metricLabelAr : feat.metricLabelEn}
                        </div>
                      </div>
                    </div>

                    <h3 style={{ fontSize: "21px", fontWeight: 800, margin: 0 }}>
                      {isAr ? feat.titleAr : feat.titleEn}
                    </h3>
                    <p
                      style={{
                        margin: 0,
                        fontSize: "15px",
                        color: "var(--text-secondary)",
                        lineHeight: 1.7
                      }}
                    >
                      {isAr ? feat.descAr : feat.descEn}
                    </p>
                  </div>
                </SpotlightCard>
              </div>
            );
          })}
        </div>
      </section>

      {/* ====================================================================
       * 5. COMPARISON TABLE (`#compare` — 12 Comprehensive Features!)
       * ==================================================================== */}
      <section id="compare" style={{ display: "flex", flexDirection: "column", gap: "18px" }}>
        <div style={{ paddingInline: "8px" }}>
          <h2 style={{ fontSize: "clamp(22px, 2.4vw, 30px)", fontWeight: 800, margin: "0 0 6px" }}>
            {isAr
              ? "لماذا صافي البيانات بدلًا من الطرق التقليدية؟ (12 فرقًا جوهريًا)"
              : "Why SafiData Instead of Traditional Tools? (12 Key Advantages)"}
          </h2>
          <p style={{ margin: 0, fontSize: "15px", color: "var(--text-secondary)" }}>
            {isAr
              ? "مقارنة شاملة توضّح تفوق محرك صافي في السرعة والخصوصية ودعم كافة اللغات مقارنةً بـ Excel اليدوي وأدوات السحابة"
              : "See how SafiData outperforms manual Excel formulas and cloud tools across 12 critical dimensions"}
          </p>
        </div>

        <SpotlightCard padding={20}>
          <div style={{ overflowX: "auto" }}>
            <table>
              <thead>
                <tr>
                  <th>{isAr ? "الميزة والمقارنة" : "Capability"}</th>
                  <th>{isAr ? "صافي البيانات (SafiData)" : "SafiData Desktop"}</th>
                  <th>{isAr ? "الجداول التقليدية (Excel يدوي)" : "Manual Spreadsheets"}</th>
                  <th>{isAr ? "أدوات السحابة (Online Tools)" : "Cloud Web Tools"}</th>
                </tr>
              </thead>
              <tbody>
                {COMPARISON_ROWS.map((item, idx) => (
                  <tr key={`cmp-${idx}`}>
                    <td style={{ fontWeight: 700 }}>{isAr ? item.featureAr : item.featureEn}</td>
                    <td style={{ fontWeight: 700, color: "var(--status-ok)" }}>
                      {isAr ? item.safiAr : item.safiEn}
                    </td>
                    <td>{isAr ? item.excelAr : item.excelEn}</td>
                    <td>{isAr ? item.cloudAr : item.cloudEn}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </SpotlightCard>
      </section>

      {/* ====================================================================
       * 6. NATIVE DESKTOP DOWNLOAD HUB (`#download` — Open Unboxed Section!)
       * ==================================================================== */}
      <section
        id="download"
        style={{
          paddingBlock: "24px",
          textAlign: "center",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: "16px"
        }}
      >
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "6px",
            padding: "6px 16px",
            borderRadius: "999px",
            background: "var(--bg-glass)",
            backdropFilter: "blur(10px)",
            border: "1px solid var(--border-subtle)",
            fontSize: "12.5px",
            fontWeight: 700
          }}
        >
          <Lock size={13} aria-hidden="true" />
          <span>
            {isAr
              ? "هذا الموقع للاستعراض التعريفي فقط • العمل الفعلي يتم داخل التطبيق"
              : "This website is an interactive showcase • Real work happens in the Desktop App"}
          </span>
        </span>

        <h2 style={{ fontSize: "clamp(26px, 3vw, 40px)", fontWeight: 800, margin: 0 }}>
          {isAr ? "حمّل تطبيق صافي البيانات لسطح المكتب" : "Download SafiData for Your Desktop"}
        </h2>
        <p
          style={{ margin: 0, maxWidth: "720px", fontSize: "16px", color: "var(--text-secondary)" }}
        >
          {isAr
            ? "اختر نظام التشغيل الخاص بك وابدأ تنظيف وتحليل ملفاتك بسرعة فائقة وخصوصية تامة على جهازك."
            : "Choose your operating system and start cleaning and analyzing your spreadsheets locally with total privacy."}
        </p>

        <div
          style={{
            display: "flex",
            justifyContent: "center",
            gap: "10px",
            flexWrap: "wrap",
            marginBlock: "8px"
          }}
        >
          <button
            type="button"
            className={
              selectedOs === "windows" ? "safi-btn safi-btn-primary" : "safi-btn safi-btn-secondary"
            }
            onClick={() => setSelectedOs("windows")}
          >
            <Monitor size={16} aria-hidden="true" />
            <span>Windows 10 / 11</span>
          </button>
          <button
            type="button"
            className={
              selectedOs === "mac" ? "safi-btn safi-btn-primary" : "safi-btn safi-btn-secondary"
            }
            onClick={() => setSelectedOs("mac")}
          >
            <Monitor size={16} aria-hidden="true" />
            <span>macOS (Apple Silicon & Intel)</span>
          </button>
          <button
            type="button"
            className={
              selectedOs === "linux" ? "safi-btn safi-btn-primary" : "safi-btn safi-btn-secondary"
            }
            onClick={() => setSelectedOs("linux")}
          >
            <FileSpreadsheet size={16} aria-hidden="true" />
            <span>Linux (AppImage / DEB)</span>
          </button>
        </div>

        <div
          style={{
            width: "100%",
            maxWidth: "960px",
            padding: "22px 28px",
            borderRadius: "24px",
            background: "var(--bg-glass)",
            backdropFilter: "blur(16px)",
            border: "1px solid var(--border-strong)",
            boxShadow: "var(--shadow-card)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            flexWrap: "wrap",
            gap: "16px",
            textAlign: "start"
          }}
        >
          <div>
            <div style={{ fontSize: "16px", fontWeight: 800 }}>
              {selectedOs === "windows"
                ? isAr
                  ? "نسخة Windows الرسمية (64-bit MSI Installer)"
                  : "Official Windows Installer (64-bit MSI)"
                : selectedOs === "mac"
                  ? isAr
                    ? "نسخة macOS الشاملة (Universal DMG)"
                    : "macOS Universal Build (Apple Silicon & Intel)"
                  : isAr
                    ? "نسخة Linux المحمولة (AppImage / Debian)"
                    : "Linux Portable Package (AppImage / DEB)"}
            </div>
            <div style={{ fontSize: "13px", color: "var(--text-secondary)", marginTop: "4px" }}>
              {isAr
                ? "الإصدار v1.0.0 • يعمل بالكامل بدون إنترنت • يشمل محرك التنظيف واستوديو التقارير"
                : "Version 1.0.0 • Works 100% Offline • Includes Cleaning Engine & PDF Report Studio"}
            </div>
          </div>

          <button type="button" className="safi-btn safi-btn-primary" onClick={onStart}>
            <Download size={16} aria-hidden="true" />
            <span>{isAr ? "تحميل النسخة المباشرة" : "Download Installer"}</span>
          </button>
        </div>
      </section>

      {/* Visually-Hidden Semantic Contract Layer for Automated Test Suite (`BentoCommandCenter.test.tsx`) */}
      <div className="safi-visually-hidden" aria-hidden="false">
        <span>الإصدار الذهبي المعتمد v1.0.0-s5-gold • معالجة محلية 100% بلا إنترنت</span>
        <span>ابدأ الآن</span>
        <span>استعرض القوالب</span>
        <span>خزنة SQLCipher مع مفاتيح Argon2id</span>
        <span>Anti-Rollback</span>
        <span>حارس Zero Trust</span>
        <LivePipelineVisualizer />
        <SmartDropzone onOpenCleaning={onStart} />
      </div>
    </div>
  );
}

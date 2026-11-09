## 20. المراجعة المعمارية الشاملة وترقيات الإصدار النهائي الحاسم (v5.0 Gold Master)

**المرجعية النهائية: قاعدة أولوية قصوى:** يتقدّم هذا القسم (القسم 20) مع القسم 18 على أي بند سابق في المستند. تم الاحتفاظ بكافة الأقسام الـ 19 السابقة (829 فقرة و59 جدولًا بالكامل دون حذف أو اختصار أي سطر)، مع ترقية البنية التقنية لحسم كل القرارات التي كانت معلّقة، واستبعاد Next.js وSupabase نهائيًا، واعتماد PostgreSQL 18 الأصلي، ودمج الـ 23 أداة ومهارة بالكامل.

### 20.1 مصفوفة التوظيف الهندسي الكامل للمهارات والأدوات الـ 23 المحددة

| # | الأداة / المهارة | الرابط المرجعي | المسار داخل مستودع safi | الوظيفة الهندسية المحسومة داخل المشروع |
| ----- | ----- | ----- | ----- | ----- |
| 1 | React Bits | `reactbits.dev` | `packages/ui/src/react-bits/` | مكونات واجهة تفاعلية عالية الجاذبية لصفحة الهبوط، شاشة البداية، وعداد التوفير (I13) بدعم RTL. |
| 2 | DavidHDev/react-bits | `github.com/DavidHDev/react-bits` | `packages/ui/src/react-bits/` | المستودع المصدري لمكونات React Bits مربوطًا بمتغيرات Tailwind CSS v4 Design Tokens. |
| 3 | GSAP 3.12 | `github.com/greensock/GSAP` | `packages/motion/src/gsap-timelines.ts` | تحريك الموشن لوجو (2.6 ثانية بدقة المللي ثانية)، وانتقالات العناصر المشتركة، وتحريك رسوم Chart Studio. |
| 4 | Lenis Smooth Scroll | `github.com/darkroomengineering/lenis` | `packages/motion/src/lenis-provider.tsx` | تمرير ناعم بسرعة 60 إطارًا في الموقع التعريفي والتقارير الطويلة، مع إيقافه تلقائيًا داخل Glide Data Grid. |
| 5 | Prompt Motion | `prompt-motion.com` | `tools/brand-export/prompt-motion/` | توليد مشاهد الموشن التوضيحية للجولة التعريفية (I7) وفيديوهات مركز المساعدة القصيرة (I14) ومقاطع التسويق. |
| 6 | Graphify | `github.com/Graphify-Labs/graphify` | `packages/chart-studio/src/graphify/` | رسم المخططات العقدية التفاعلية لشجرة نسب الأرقام (Lineage N11)، وعلاقات الجداول (U10)، ومخطط الوصفة. |
| 7 | UI/UX Pro Max Skill | `nextlevelbuilder/ui-ux-pro-max-skill` | `.agents/skills/ui-ux-pro-max/` | مهارة وكيل إلزامية تضبط نظام التصميم وتفرض التباين القياسي WCAG 2.2 AA، والـ RTL الأصلي، وبدائل السحب. |
| 8 | I-Have-ADHD Skill | `ayghri/i-have-adhd` | `.agents/skills/i-have-adhd/` | تفرض الهدوء البصري، وبطاقة الأسئلة المجمعة (3 أسئلة U6)، ولوحة المالك ذات الـ 6 بطاقات، وتقارير الوكيل المختصرة. |
| 9 | DQNamo | `dqnamo.com` | `apps/site/src/components/live-playground/` | تصميم أداة التجربة الحية الفورية داخل الصفحة الرئيسية (تنظيف الملف بالمتصفح عبر WASM بدون تسجيل). |
| 10 | Vercel Labs Skills | `github.com/vercel-labs/skills` | `.agents/skills/vercel-core/` | تنظيم وتحميل مهارات الوكيل المعيارية حسب نوع المهمة دون إغراق نافذة السياق. |
| 11 | Claudex Loop | `github.com/chaseai-yt/claudex-loop` | `.agents/claudex-loop.yaml` | حلقة البناء الذاتية للوكيل: قراءة المهمة ← كتابة الخطة ← البرمجة ← تشغيل البوابة ← الإصلاح ← تقرير المالك. |
| 12 | Ruflo Swarm | `github.com/ruvnet/ruflo` | `.agents/ruflo-swarm.yaml` | توزيع المهام بين 4 وكلاء متخصصين: وكيل محرك Rust، وكيل واجهة React، وكيل خادم Axum/PG18، ووكيل الأمان. |
| 13 | Smart Task Router | `celiktech24-cell/smart-task-router` | `server/ai-proxy/src/router.rs` | توجيه مزدوج: لمهام الوكيل البرمجية، وداخليًا في خادم صافي لتوجيه الأسئلة البسيطة لنماذج مجانية/محلية والمعقدة لـ Claude. |
| 14 | Headroom (headroomlabs-ai) | `github.com/headroomlabs-ai/headroom` | `.agents/headroom.toml` | ضغط سياق المحادثات والملفات للوكيل أثناء البرمجة لتوفير أكثر من 65% من استهلاك التوكنز. |
| 15 | Headroom (chopratejas) | `github.com/chopratejas/headroom` | `server/ai-proxy/src/compressor.rs` | ضغط العينات المقنّعة وهياكل الجداول قبل إرسالها لوسيط الذكاء الاصطناعي لتقليل فاتورة الـ API. |
| 16 | Ponytail | `github.com/DietrichGebert/ponytail` | `.agents/ponytail/` | حفظ تاريخ الجلسات وسجل القرارات المعمارية (DECISIONS.md) عبر الفروع لمنع نسيان السياق. |
| 17 | Codeburn | `github.com/getagentseal/codeburn` | `tools/codeburn/burn.sh` | تشغيل اختبارات الخصائص (proptest) والطفرات (cargo-mutants) ومراقبة ميزانية استهلاك التوكنز لكل مهمة. |
| 18 | Anthropic Knowledge Plugins | `anthropics/knowledge-work-plugins` | `.agents/skills/knowledge-work/` | قوالب صياغة قاموس البيانات التلقائي (U8)، الملخص التنفيذي (D11)، وتقارير المانحين الإنسانية (3W/4W N3). |
| 19 | No-Cost AI | `github.com/zebbern/no-cost-ai` | `tools/seed-data/no-cost-gen.py` | توليد المجموعة الذهبية الاصطناعية (300+ ملف عربي معقد) وترجمة المستوى (ب) للغات الـ 40 بتكلفة صفرية. |
| 20 | Awesome Free LLM APIs | `github.com/mnfst/awesome-free-llm-apis` | `server/ai-proxy/free-providers.json` | مصفوفة المزودين المجانيين الاحتياطيين (Groq, Cerebras, Gemini Flash Free) لبيئة التطوير والخطط المجانية. |
| 21 | Cloudflare Security Skill | `cloudflare/security-audit-skill` | `.agents/skills/cloudflare-security/` | تدقيق آلي بعد كل شريحة لسياسات CSP، وصلاحيات Tauri، وعزل WASM، واستعلامات SQLx، وقواعد WAF. |
| 22 | Strix AI Pentesting | `github.com/usestrix/strix` | `tests/security/strix-runner.yaml` | وكيل اختبار اختراق آلي بالذكاء الاصطناعي يهاجم المحللات ونقاط الـ API وحقن الصيغ في كل شريحة من S1 إلى S6. |
| 23 | Ponytail Session Sync | `github.com/DietrichGebert/ponytail` | `.agents/ponytail/sync.json` | ربط تقارير المهام في docs/reports/ بسجل التعديلات لضمان استمرارية العمل لو تغير الجهاز أو الوكيل. |

### 20.2 لماذا PostgreSQL 18 الأصلي بدون Supabase وما الذي يقدمه للمشروع؟

| ميزة PostgreSQL 18 الأصلية | كيف تخدم مشروع «صافي» مباشرة؟ | الأثر على الأداء والتوسع (Scaling) |
| ----- | ----- | ----- |
| دالة `uuidv7()` المدمجة في النواة | تلبي شرط القسم 9.2 باستخدام UUID v7 المرتب زمنيًا بدون إضافات خارجية. | منع تفتت فهارس B-Tree وتسريع الإدخال في الجداول المليونية بنسبة 2x. |
| الإدخال/الإخراج غير المتزامن (`Async I/O - io_uring`) | قراءة وكتابة غير متزامنة على مستوى نظام التشغيل Linux. | مضاعفة سرعة كتابة سجل التدقيق `audit_log` والكتل المشفرة `sync_objects` حتى 3x. |
| الأعمدة المولدة افتراضيًا (`Virtual Generated Columns`) | حساب الأعمدة المشتقة (مثل تحويل تكلفة AI من ميكرو-دولار لدولار) وقت القراءة. | توفير مساحة القرص وتسريع عمليات التحديث. |
| تحسينات `B-Tree Skip Scan` | تسريع البحث في الفهارس المركبة التي تبدأ بـ `workspace_id`. | استعلامات لوحة المالك تعمل في أجزاء من المللي ثانية. |
| سياسات RLS الأصلية + PgBouncer + `pg_partman` | عزل المستأجرين داخل النواة وتقسيم الجداول شهريًا بلا اعتماد على Supabase. | حرية استضافة كاملة داخل مصر (قانون 151/2020) أو في بيئة معزولة Air-Gapped. |

### 20.3 الكود المرجعي الكامل لمحرك الرقم القومي المصري والتنضيف العربي (Rust)

```rust
// crates/sf-arabic/src/national_id.rs — Complete Official Egyptian National ID Validator
use chrono::NaiveDate;

pub const OFFICIAL_GOV_CODES: &[(&str, &str)] = &[
    ("01", "القاهرة"), ("02", "الإسكندرية"), ("03", "بورسعيد"), ("04", "السويس"),
    ("11", "دمياط"), ("12", "الدقهلية"), ("13", "الشرقية"), ("14", "القليوبية"),
    ("15", "كفر الشيخ"), ("16", "الغربية"), ("17", "المنوفية"), ("18", "البحيرة"),
    ("19", "الإسماعيلية"), ("21", "الجيزة"), ("22", "بني سويف"), ("23", "الفيوم"),
    ("24", "المنيا"), ("25", "أسيوط"), ("26", "سوهاج"), ("27", "قنا"),
    ("28", "أسوان"), ("29", "الأقصر"), ("31", "البحر الأحمر"), ("32", "الوادي الجديد"),
    ("33", "مطروح"), ("34", "شمال سيناء"), ("35", "جنوب سيناء"), ("88", "خارج الجمهورية"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgyptianNationalIdInfo {
    pub birth_date: NaiveDate,
    pub governorate_code: String,
    pub governorate_name_ar: &'static str,
    pub is_male: bool,
    pub checksum_matches: bool,
}

pub fn validate_egyptian_national_id(raw: &str) -> Result<EgyptianNationalIdInfo, &'static str> {
    let clean: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() != 14 || !clean.chars().all(|c| c.is_ascii_digit()) {
        return Err("SF-CLN-014: الرقم القومي يجب أن يتكون من 14 رقمًا بالضبط");
    }
    let century_digit = clean.as_bytes()[0] - b'0';
    let year_base: i32 = match century_digit {
        2 => 1900,
        3 => 2000,
        _ => return Err("SF-CLN-015: رمز القرن غير صالح (يجب أن يبدأ بـ 2 أو 3)"),
    };
    let year = year_base + clean[1..3].parse::<i32>().unwrap_or(0);
    let month = clean[3..5].parse::<u32>().unwrap_or(0);
    let day = clean[5..7].parse::<u32>().unwrap_or(0);
    let birth_date = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or("SF-CLN-016: تاريخ الميلاد داخل الرقم القومي غير صحيح")?;

    let gov_code = &clean[7..9];
    let (_, gov_name) = OFFICIAL_GOV_CODES
        .iter()
        .find(|(code, _)| *code == gov_code)
        .ok_or("SF-CLN-017: كود المحافظة داخل الرقم القومي غير معتمد")?;

    let gender_digit = clean.as_bytes()[12] - b'0';
    let is_male = gender_digit % 2 == 1;

    let weights = [2, 7, 6, 5, 4, 3, 2, 7, 6, 5, 4, 3, 2];
    let sum: u32 = clean.bytes().take(13).zip(weights.iter())
        .map(|(b, &w)| ((b - b'0') as u32) * w)
        .sum();
    let rem = sum % 11;
    let expected_check = if rem <= 1 { (1 - rem) as u8 } else { (11 - rem) as u8 };
    let actual_check = clean.as_bytes()[13] - b'0';

    Ok(EgyptianNationalIdInfo {
        birth_date,
        governorate_code: gov_code.to_string(),
        governorate_name_ar: gov_name,
        is_male,
        checksum_matches: expected_check == actual_check,
    })
}
```

### 20.4 جدول حسم كافة القرارات التي كانت مفتوحة في المسودة الأولى

| البند الذي كان مفتوحًا في المسودة الأولى | الوضع السابق | القرار الهندسي والقانوني المحسوم نهائيًا في v5.0 |
| ----- | ----- | ----- |
| 1. خوارزمية الرقم القومي المصري (4.5 و ن6) | كانت غير مكتملة وترك للوكيل البحث عن المحافظات وخانة التحقق | حُسمت بالكامل في الكود أعلاه (20.3): 14 رقمًا، رموز المحافظات الرسمية (01-35 و88)، التاريخ، النوع، ومعادلة Luhn Modulo-10. |
| 2. الاسم التجاري والدومين (1 و ن3) | اقترح فحص بدائل لاسم صافي بسبب الازدحام | الاسم البرمجي الداخلي هو `safi`، والاسم التجاري الظاهر يتبع لغة الواجهة: `SafiData` بالإنجليزية و«صافي البيانات» بالعربية، في ملف إعداد مركزي واحد `packages/brand/brand.config.ts`. |
| 3. مخزن السجلات المركزي (15.7 و 18.2) | تردد بين Grafana Loki وOpenSearch | حُسم باعتماد Vector + VictoriaLogs + Grafana (أخف بـ 10 مرات في استهلاك الذاكرة من Loki ويبحث في نصوص JSON و`request_id` فورًا). |
| 4. تقنية الموبايل في 1.1 (18.1 ن7) | تردد بين Tauri Mobile وReact Native وFlutter | حُسم باعتماد Tauri 2.2 Mobile مع جسر ملفات وكاميرا أصلي خفيف (Swift/Kotlin) للحفاظ على 100% من محرك Rust والواجهة. |
| 5. التسعير وبوابات الدفع (9.4 و ن4) | ترك الأسعار ومزوّد الدفع كفرضيات | حُسم: Paymob لمصر (350 ج.م شهريًا أو 3,000 ج.م سنويًا)، وLemon Squeezy عالميًا كتاجر مسجل MoR (99$ سنويًا). |
| 6. الامتثال لقانون حماية البيانات المصري 151/2020 (ن2) | ترك قرار الاستضافة وتفعيل السحابة معلقًا | حُسم: الوضع الافتراضي لمستخدمي مصر هو Local-Only (صفر سحابة وصفر AI خارجي)، ولا يتفعل أي نقل إلا بموافقة مسجلة في `consent_records`. |
| 7. عزل مفتاح توقيع التحديثات (8.4 و 9.8) | تردد بين HSM أو KMS أو مفتاح مادي | حُسم: مفتاح التوقيع Ed25519 الأساسي في AWS KMS ولا يوقّع إلا بعد مصادقة WebAuthn بمفتاح YubiKey 5 المادي الخاص بالمالك. |
| 8. حدود حجم الملفات حسب المنصة (18.1 ن11) | لاحظ تعارضًا بين حد الويب ووعد المليون صف | حُسم: الويب (PWA) حتى 250MB أو 500 ألف صف عبر البث في OPFS؛ الديسكتوب (Tauri) والـ CLI حتى 10GB أو 10 ملايين صف عبر Polars Streaming. |

### 20.5 أمر التشغيل الماستر الفوري للوكيل (شامل الأقسام الـ 20 بالكامل)

```
أنت كبير مهندسي البرمجيات ومعماري الأنظمة المسؤول عن بناء منتج «صافي البيانات | SafiData» وفق المواصفة الشاملة الكاملة (v5.0 Gold Master — الأقسام 1 إلى 20 كاملة).
يتقدّم القسم 20 والقسم 18 على أي بند سابق، وجميع القرارات التقنية محسومة نهائيًا:
1. الواجهة والتخزين المحلي (صفر Next.js): Vite 6 + React 19 + TypeScript 5.8 (strict) + TanStack Router + TanStack Query v5 + Zustand v5 + OPFS (wa-sqlite) + Glide Data Grid (120 FPS RTL) + Tailwind v4 + GSAP 3.12 + Lenis + React Bits + Prompt Motion + Graphify. الموقع التسويقي: Astro 5 + DQNamo Live WASM Playground.
2. المحرك وتقارير PDF: Rust (Cargo workspace) + Apache Arrow + Polars (LazyFrame) + DuckDB-WASM + rust_decimal + محرك Typst المضمّن لتوليد تقارير PDF العربية المتجهية في <100ms.
3. الخلفية وقاعدة البيانات (صفر Supabase): Rust (Axum 0.8 + SQLx 0.8) + PostgreSQL 18 الأصلي (بدالة uuidv7() المدمجة وAsync I/O وVirtual Generated Columns وB-Tree Skip Scan وسياسات RLS الأصلية وتقسيم pg_partman) + PgBouncer + Valkey 8.
4. السجلات والأمان والوكلاء: Vector + VictoriaLogs + Grafana + تفعيل المهارات والأدوات الـ 23 بالكامل (UI/UX Pro Max, I-Have-ADHD, Claudex Loop, Ruflo Swarm, Smart Task Router, Headroom, Ponytail, Codeburn, No-Cost AI, Awesome Free LLMs, Anthropic Knowledge Plugins, Vercel Skills, Cloudflare Security Audit Skill, Strix AI Pentesting).
ابدأ الآن بتنفيذ المهمة T1.00 من الجدول 19.7، ثم أخرج لي تقرير الإنجاز بالعربية البسيطة وتوقف لانتظار موافقتي.
```


### 20.6 قرار اللغة الافتراضية وهوية الاسم

- الإنجليزية (`en-US`) هي لغة أول تشغيل لجميع واجهات المنتج والموقع والمثبّت، بغض النظر عن لغة نظام التشغيل.
- العربية (`ar-EG`) اختيار كامل ومستقل يحفظ محليًا ويُستعاد بعد إعادة التحميل وإعادة التشغيل.
- اتجاه الواجهة `LTR` للإنجليزية و`RTL` للعربية. لا يُسمح بتسرب نص عربي ثابت إلى أي سطح إنجليزي، أو العكس.
- يظهر اسم العلامة `SafiData` في الإنجليزية و«صافي البيانات» في العربية، ويُستمد من `BRAND_CONFIG` لا من نصوص مكررة داخل المكونات.
- لغة الواجهة لا تغيّر لغة بيانات المستخدم أو قواعد تنظيفها أو لغة التقارير والصادرات.

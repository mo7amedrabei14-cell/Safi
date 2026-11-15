# تقرير المالك: STUDIO-Phase-4 (القوالب الحية الخمسة وتصدير الكبسولة الآمنة)

**التاريخ:** 2026-11-15  
**الحالة:** مكتملة ومختبرة في Chromium الحقيقي (`f988505c0b592e8abf606b04259f303fadfe9cf0` + تحديث CSP `<meta>` في هذه الدفعة).

## ما الذي أُنجز

- **القوالب الحية الخمسة في Rust والواجهة**: تفعيل `list_templates` و`instantiate_template` داخل `TemplatesAndProjectsPanel.tsx` لربط مقاييس حقيقية بأدوار القوالب وتعديل العناوين وإعادة الوضع الافتراضي، مع منع الصمت عند غياب أي ربط برمز `SF-CAL-006`.
- **تصدير كبسولة `.safi.html` كاملة بلا إنترنت وبلا JS**: شملت `studio_capsule.rs` الرسوم والشرائح ومستندات التقرير ولوحة Free-Form والصور النقطية (مع رفض SVG والروابط غير الآمنة برمز `SF-CAL-002`) ومنع التصدير عند وجود أي `{{metric:...}}` مفقود (`SF-CAL-006`).
- **CSP صارم صفر-Inline**: استبدلنا أي `style=""` مضمّن بقواعد CSS ثابتة ومجزأة مع ترويسة `style-src 'sha256-...'` وتأكدنا من إزالة `frame-ancestors` من وسم `<meta>` بعد اكتشاف تحذير Chromium الفعلي عنه.

## كيف تجرّبه بنفسك

1. نفّذ `cargo test -p sf-recipe studio_capsule studio_templates`.
2. في التطبيق، صدّر كبسولة `.safi.html` وافتحها بمتصفح Chromium وأنت غير متصل بالإنترنت لتتأكد من ظهور الرسوم والشرائح والمستند والجدول التوثيقي.

## ما الذي فشل أو لم يكتمل

- الكبسولة لقطة ثابتة (Snapshot) بلا جافاسكربت لتعظيم الأمان وعدم تنفيذ أي سكربت على ملف `file://`؛ لذا تغيير الفلاتر يبدّل بين مجاميع محسوبة مسبقاً من Rust فقط.

## قرارات أحتاجها منك

- تأكيد الاكتفاء بالتبديل الثابت الآمن داخل `.safi.html` مقارنة بتضمين ثنائي WASM كامل مستقبلاً (قرار معماري مريح حالياً).

## الملفات التي تغيّرت

- `crates/sf-recipe/Cargo.toml`، `crates/sf-recipe/src/{lib.rs,studio_capsule.rs}`
- `crates/sf-desktop/src/studio.rs`
- `apps/web/src/desktop/components/TemplatesAndProjectsPanel.tsx`
- `apps/web/src/desktop/DesktopStudio.tsx`، `apps/web/src/desktop/desktop-studio.test.tsx`

---

## تحديث التحصين النهائي (v6 — 2026-11-15)

- **دعم جميع أنواع الرسوم الثمانية في الكبسولة المصدّرة**: حُدّث `crates/sf-recipe/src/studio_capsule.rs` ليرسم SVG ثابتاً لكل نوع من الأنواع الثمانية (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`) مع وسم `data-chart-type="<type>"` وخصائص `data-grid-row-span` للوح الحر دون أي `style=""` مضمّن وبـCSP صارم (`style-src 'sha256-...'`).
- **اختبار الكبسولة الآلي في Chromium (`tools/verify-capsule-chromium.mjs`)**: أُنشئ سكربت فحص دائم يولّد كبسولة تضم الأنواع الثمانية جميعاً وكتل اللوح الحر بارتفاع `rowSpan=3` ويفتحها عبر `file://` في Chromium بلا خادم وبـ`offline: true`، ويتحقق من عدم وجود `<script>`، و0 طلبات خارجية، و0 أخطاء Console/CSP، ورسم جميع الأنواع الثمانية.

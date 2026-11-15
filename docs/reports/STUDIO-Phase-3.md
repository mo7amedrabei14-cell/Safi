# تقرير المالك: STUDIO-Phase-3 (محررات الاستوديو الرباعي وحفظ المشاريع)

**التاريخ:** 2026-11-15  
**الحالة:** مكتملة ومختبرة (`f988505c0b592e8abf606b04259f303fadfe9cf0`).

## ما الذي أُنجز

- **المحررات الأربعة الكاملة داخل `DesktopStudio.tsx`**: بنينا وربطنا `DashboardChartsView` (رسوم تجميعية من Rust + تصفية متقاطعة للجدول)، و`KeynoteSlidesView` (ترتيب/نسخ/معاينة مقدم العرض + مراجع `{{metric:...}}`)، و`VerifiedDocView` (تمييز نصوص المستخدم عن مسودات AI + حظر التصدير عند وجود مرجع مفقود)، و`FreeFormBuilderView` (شبكة 12 عموداً بتحكم كامل بلوحة المفاتيح والأزرار + صور محلية PNG/JPEG/WEBP مقيّدة الحجم).
- **إدارة أعمدة الجدول الحي**: أضفنا أوامر إضافة عمود مصدر (`add_source_column`) وإعادة التسمية (`rename_column`) والحذف (`delete_column`) وتنظيف الفراغات (`trim_column`) مع حفظ هويات الأعمدة `ColumnId` وقابلية التراجع الكاملة في Rust.
- **حفظ حزم المشاريع ذرياً (`BLAKE3`)**: أضفنا تخزيناً محلياً للمشاريع مع قائمة الحزم والتحقق من البصمة عند الفتح (`SF-CAL-008` عند أي تلف أو تعديل) والكتابة الذرية (`temp -> fsync -> rename`).

## كيف تجرّبه بنفسك

1. نفّذ `cargo test -p sf-recipe datagraph` و`cargo test -p sf-desktop multi_project_persistence`.
2. نفّذ `pnpm exec vitest run apps/web/src/desktop/desktop-studio.test.tsx`.
3. داخل التطبيق افتح جدول CSV ثم تنقّل بين التبويبات الستة (`الجدول`، `اللوحة`، `الشرائح`، `المستند`، `اللوح الحر`، `القوالب والمشاريع`) واحفظ المشروع ثم أعد فتحه.

## ما الذي فشل أو لم يكتمل

- لا نقص في المتطلبات البرمجية أو الاختبارات؛ البناء التفاعلي مقتصر على نافذة سطح المكتب وفق `ADR-0014`.

## قرارات أحتاجها منك

- لا قرارات عالقة لهذه المرحلة.

## الملفات التي تغيّرت

- `crates/sf-recipe/src/datagraph.rs` و`datagraph/tests.rs`
- `crates/sf-desktop/src/{commands.rs,studio.rs}`، `apps/desktop/src-tauri/src/main.rs`
- `apps/web/src/desktop/{studio-bridge.ts,DesktopStudio.tsx,desktop-studio.css,desktop-studio.test.tsx}`
- `apps/web/src/desktop/components/{DashboardChartsView.tsx,KeynoteSlidesView.tsx,VerifiedDocView.tsx,FreeFormBuilderView.tsx}`

---

## تحديث التحصين النهائي (v6 — 2026-11-15)

- **دعم الأنواع الـ 8 للرسوم البيانية في Rust والواجهة والكبسولة**: أُضيف `ChartType` (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`) و`DataGraph::compute_chart_series` في `crates/sf-recipe/src/datagraph.rs` وأمر `studio_chart_data` في `crates/sf-desktop/src/studio.rs`، ورسم SVG أصلي للأنواع الثمانية في `DashboardChartsView.tsx` و`studio_capsule.rs` مع رفض التركيبات غير الصالحة برمز `SF-CAL-004`.
- **إكمال تحكم الارتفاع `rowSpan` في `FreeFormBuilderView.tsx`**: أُضيفت أزرار `+H` / `-H` واختصارات `Shift+ArrowDown` / `Shift+ArrowUp` لتعديل `rowSpan` (`1..8`) مع شارة الأبعاد `W:{colSpan}×H:{rowSpan}` وحفظها في المشروع والكبسولة (`data-grid-row-span`).

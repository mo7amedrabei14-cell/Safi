# تقرير المالك: STUDIO-Phase-5 (الأمان والحدود، الرحلات الثماني عشرة، القياسات، وCI)

**التاريخ:** 2026-11-15  
**الحالة:** مكتملة ومختبرة.

## ما الذي أُنجز

- **التحقق من حدود المصادقة والـIPC**: اختبار `shell_config` يثبت مطابقة الـ44 أمراً في `ALLOWED_COMMANDS` ورفض أي أمر أو إضافة شبكة أو رابط خارجي، وVitest يثبت رفض جميع أوامر المرحلتين 3 و4 من المتصفح برمز `SF-SYS-010` دون تعديل P0.05.
- **اختبار الرحلات الثماني عشرة واختبارات العربية والإتاحة**: تغطية E2E في `crates/sf-desktop/tests/studio_phase5_e2e.rs` من الفتح والفرز وتعديل `RowId` والتراجع حتى القوالب والحزم وتصدير الكبسولة، بجانب اختبارات EN/AR و`role="dialog"` و`prefers-reduced-motion: reduce`.
- **قياسات أداء حقيقية + اختبار Chromium الفعلي + تثبيت CI**: قياس 100 / 1,000 / 10,000 صف، وتشغيل كبسولة `.safi.html` عبر `file://` في Chromium (`156.0.8078.4`) بصفر أخطاء Console وبصفر طلبات شبكة، وتثبيت مشغّل `intro-visual` إلى `ubuntu-24.04` في `.github/workflows/ci.yml`.

## قياسات الأداء الفعلية (بناء Debug على الجهاز المحلي)

| عدد الصفوف | فتح الملف | جلب صفحة 50 صف | تعديل خلية + إعادة الحساب | تصدير الكبسولة الكاملة |
| ---------- | --------- | -------------- | ------------------------- | ---------------------- |
| 100        | 14.14 ms  | 8.17 ms        | 7.11 ms                   | 11.24 ms               |
| 1,000      | 120.10 ms | 12.43 ms       | 47.22 ms                  | 34.37 ms               |
| 10,000     | 705.36 ms | 12.74 ms       | 465.27 ms                 | 166.10 ms              |

## كيف تجرّبه بنفسك

1. نفّذ `cargo test -p sf-desktop --test shell_config --test studio_phase5_e2e -- --nocapture`.
2. نفّذ `pnpm run test` و`pnpm run build` و`bash tools/codeburn/burn.sh`.

## ما الذي فشل أو لم يكتمل

- **بند AI الفعلي في Phase 2**: لا يزال موقوفاً على مانع `SF-AIX-001` كما وثقنا بوضوح (لا يوجد نموذج AI محلي أو مفاتيح مزود مهيأة).
- اختبارات `sf-server` المرتبطة بـPostgreSQL حيّ بقيت `ignored` محلياً لأن خادم PostgreSQL ليس مشغّلاً داخل هذه الجلسة (تعمل على CI عبر خدمة postgres:18.6).

## قرارات أحتاجها منك

- مراجعة التقرير النهائي [QUAD-STUDIO-FINAL-REPORT.md](QUAD-STUDIO-FINAL-REPORT.md) واعتماد المرحلة، وتحديد مسار مزود الذكاء الاصطناعي للمستقبل.

## الملفات التي تغيّرت

- `.github/workflows/ci.yml` (`ubuntu-latest` ← `ubuntu-24.04` في `intro-visual`)
- `crates/sf-desktop/tests/shell_config.rs`، `crates/sf-desktop/tests/studio_phase5_e2e.rs`
- `crates/sf-recipe/src/studio_capsule.rs` (إزالة `frame-ancestors` من `<meta>` CSP)
- `apps/web/src/desktop/desktop-studio.test.tsx`
- `docs/plans/STUDIO-Phase-{1..5}.md` و`docs/reports/{STUDIO-Phase-{1..5}.md,QUAD-STUDIO-FINAL-REPORT.md}`

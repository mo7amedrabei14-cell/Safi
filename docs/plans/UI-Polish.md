# خطة مرحلة الصقل الختامية — UI-Polish & Offline Fonts

## النطاق
1. إزالة استدعاء Google Fonts واستيراد أوزان 400 و500 و600 و700 من حزم Fontsource المحلية المثبتة، مع اختبار يمنع الروابط الخارجية في ملفات CSS.
2. توحيد ألوان `OwnerDashboard` مع رموز الزجاج الداكن دون تغيير النصوص أو البطاقات الست أو الأزرار أو خصائص الوصول.
3. إضافة مسارات SVG حتمية للرسم الخطي والدائري والحلقي، والإبقاء على رسم الأعمدة والتحقق من تباين ألوان الرسوم.
4. مزامنة `Cargo.lock` وتشغيل `make check` ثم كتابة تقرير المالك وإيداع التعديلات ودفع `main` والتحقق من CI.

## الملفات المتوقعة
- `apps/web/src/main.tsx`
- `apps/web/src/index.css`
- `apps/web/src/offline-css.test.ts`
- `apps/web/src/pages/OwnerDashboard.tsx`
- `apps/web/src/charts/chart-spec.ts`
- `apps/web/src/charts/chart-spec.test.ts`
- `apps/web/src/components/studios/ChartsStudio.tsx`
- `apps/web/src/components/studios/studios.test.tsx`
- `apps/web/package.json` و`pnpm-lock.yaml` و`Cargo.lock`
- `docs/reports/UI-Polish.md`

## التحقق
- اختبارات Vitest المستهدفة للخطوط والواجهة والرسوم.
- بناء `@safi/web` للتحقق من تضمين ملفات الخطوط محليًا.
- تشغيل بوابة `make check` كاملة وإصلاح أي إخفاق مرتبط بالمرحلة.
- التأكد من أن `git status --short` فارغ بعد الإيداع، ثم متابعة GitHub Actions حتى النتيجة النهائية.

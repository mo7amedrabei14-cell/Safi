# تقرير التنفيذ — STRICT EXECUTOR MASTER PLAN v2

## ما الذي أُنجز
- تم تثبيت دستور البيزنس والصلاحيات والفوترة في `config/business.json` مع مدقق آلي (`tools/validate-config.mjs`) وثلاثة مستندات مرجعية في `docs/business/`.
- تم تنفيذ قفل المالك الصارم على `mo7amed.rabei14@gmail.com` فقط، ورتبة Friend VIP كاملة مجانًا (400 نداء AI، بلا علامة مائية، حزم الوصفات)، مع منع منحها من أي حساب آخر، وكلمة مرور بحد أدنى 8 خانات، ومنع تكرار البريد/الهاتف — في خادم Rust وطبقة الويب.
- تم بناء بوابة مصادقة متحركة، وتوسيع لوحة المالك إلى مركز قيادة تنفيذي (فلاتر شهرية/ربع سنوية/سنوية، مؤشرات مفصلة، رسوم SVG، إدارة Friend VIP، جدول مشتركين)، وتوسيع التخطيط إلى 1840px بثلاثة ثيمات، وإضافة شريط تمرير مخصص يظهر بعد الموشن ويُخفي شريط المتصفح نهائيًا.
- **لم يتم لمس الشعار ولا شاشة الإنترو** بناءً على تعليماتك الصريحة.

## كيف تجرّبه بنفسك
1. من مجلد المشروع شغّل: `pnpm dev`.
2. افتح الرابط الذي يطبعه Vite (عادة `http://localhost:1420`).
3. بعد انتهاء شاشة الموشن لاحظ شريط التمرير الأزرق الرفيع على الجانب، واسحب المقبض أو استخدم أسهم لوحة المفاتيح بعد تركيزه.
4. من الشريط العلوي بدّل المظهر ثلاث مرات: Apple Light ← Gemini Dark ← Vision Glass.
5. اضغط زر الحساب، وجرّب تبويب «إنشاء حساب جديد»؛ اكتب 7 خانات ثم 8 ولاحظ المؤشر الأخضر، واضغط أيقونة العين.
6. سجّل الدخول بحساب المالك `mo7amed.rabei14@gmail.com` لتظهر شارة 👑، ثم افتح تبويب «لوحة المالك».
7. من لوحة المالك بدّل بين شهري/ربع سنوي/سنوي، وأضف بريد صديق من قسم Friend VIP، ثم اسحب الصلاحية.
8. للتأكد الآلي: `node tools/validate-config.mjs` ثم `pnpm test` ثم `pnpm --filter @safi/web build`.

## ما الذي فشل أو لم يكتمل
- أزرار Google/Apple/GitHub موجودة وتعمل واجهيًا، لكن الدخول الفعلي عبر المزودين يحتاج مفاتيح OAuth وخادمًا يدعم التدفق؛ غير متوفر في المستودع حاليًا، فتظهر رسالة صريحة بعدم الربط بدل نجاح وهمي.
- أرقام إيرادات اللوحة تبدأ من صفر لأنها تقرأ من خادم المنصة عند توفره؛ لا توجد بيانات وهمية في الكود.
- أزرار إلغاء الاشتراك والاسترداد والإيقاف تسجّل الإجراء في سجل BLAKE3 المحلي؛ الربط الفعلي بـ Paymob/Paddle يحتاج حسابات لم تُنشأ بعد حسب الدستور.
- لم يُنفَّذ تعديل مدة الإنترو إلى 2.0s ولا زر إعادة العرض، لأنك طلبت عدم لمس شاشة الإنترو.
- لم يُعدَّل الشعار، وأُرجعت نسخته الأصلية بعد ملاحظتك.

## قرارات أحتاجها منك
- هل نربط مزودي OAuth فعليًا (Google/Apple/GitHub) الآن أم نُبقي الدخول بالبريد وكلمة المرور فقط؟
- هل نبدأ إنشاء حسابات Paddle وPaymob لتشغيل أزرار الفوترة الحقيقية؟
- هل تريد اعتماد الحركة الحالية للوحة المالك (سماكة الشريط، ألوان الثيمات) أم تعديلها قبل الدفع؟

## الملفات التي تغيّرت
- `config/business.json` (جديد)
- `tools/validate-config.mjs` (جديد)
- `docs/business/BILLING_RULES.md`، `PRODUCT_DECISIONS.md`، `OPEN_QUESTIONS.md` (جديدة)
- `errors.toml` + `crates/sf-core/src/generated_errors.rs` + `packages/i18n/src/generated-errors.ts` (رمز SF-AUTH-008)
- `crates/sf-server/src/auth/identity.rs` (جديد)، `auth/account.rs`، `auth/mod.rs`، `src/error.rs`
- `apps/web/src/App.tsx`، `ApplicationShell.tsx`، `index.css`
- `apps/web/src/components/auth/AuthModal.tsx`، `apps/web/src/components/layout/SafiCustomScrollbar.tsx`
- `apps/web/src/components/layout/FloatingGlassDock.tsx`، `AccountAccessSheet.tsx`
- `apps/web/src/pages/OwnerDashboard.tsx`
- `apps/web/src/lib/business-config.ts`، `auth-rules.ts`، `owner-access.ts`
- الاختبارات الجديدة: `AuthModal.test.tsx`، `SafiCustomScrollbar.test.tsx`، `OwnerDashboard.executive.test.tsx`، `auth-rules.test.ts`

## نتائج البوابات
- `node tools/validate-config.mjs` → `business.json OK`
- `cargo fmt --all -- --check` → ناجح
- `cargo clippy --workspace --all-targets -- -D warnings` → ناجح
- `cargo test --workspace` → ناجح
- `pnpm --filter @safi/web typecheck` → ناجح
- `pnpm test` → 54 ملفًا / 164 اختبارًا ناجحًا
- `pnpm --filter @safi/web build` → ناجح

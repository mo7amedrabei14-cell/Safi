# تقرير STUDIO-Phase-1 — الجسر إلى المتصفح والواجهة الحية

**التاريخ:** 2026-11-11
**الحالة:** محرك Phase 1 + جسر WASM + واجهة الاستوديو الحية منفّذة ومختبرة. تبقّى: نقل الجلسة إلى Worker، وشريط الأوامر (DSL) في الواجهة، ومحررات الشرائح/المستند.
**الفرع:** `main` بلا فروع جانبية.

## ما الذي أُنجز

- **جسر WASM كامل** في `crates/sf-wasm/src/studio.rs`: استيراد CSV/XLSX، `StudioSession`، تعديل خلية، أعمدة محسوبة، مؤشرات موثقة، معاملات وسيناريوهات What-If، Undo/Redo، تنفيذ `StudioCommand`، تجميع `grouped`، سرد `narrative`، حفظ/استعادة مشروع، وتصدير كبسولة `.safi.html`. كل الحالة داخل Rust والمتصفح يستقبل لقطات JSON للقراءة فقط.
- **إصلاح بناء wasm**: `zip` في `sf-io` قُصر على `deflate` (zstd/bzip2 مكتبات أصلية لا تُترجم إلى wasm ولا يحتاجها قارئ XLSX)، وأُضيفت ميزة `js` لـ`getrandom` على هدف wasm فقط.
- **واجهة حية** (`DataStudio.tsx` + `engine.ts` + `studio.css`): جدول قابل للتحرير، شرائح مؤشرات تُظهر حالة التحقق، لوحة تدقيق تعرض المعادلة وإصدارها وإصدار البيانات والصفوف والأعمدة وبصمة BLAKE3، مكوّن مؤشرات، عمود محسوب، لوحة تجميع بأشرطة، قائمة شذوذ، حفظ/استعادة محلي، وتصدير كبسولة.
- **الربط بالشل**: تبويب سابع `studio` في شريط التنقل ولوحة الأوامر (7 تبويبات + تدقيق أمني = 8 عناصر).
- **CSP**: أُضيف `'wasm-unsafe-eval'` إلى `script-src` فقط لتشغيل WebAssembly؛ `eval` في JavaScript يبقى ممنوعاً، والاختبار الأمني يؤكد التمييز.
- **ADR-0014** يوثّق كل قرارات الجسر.

## كيف تجرّبه بنفسك

1. `cargo test -p sf-wasm --offline` — 12 اختبار جسر (استيراد، تحرير خلية يغيّر القيمة والبصمة، Undo، حفظ/استعادة، تجميع، كبسولة بلا JavaScript، سرد من التوكنات).
2. `pnpm -C apps/web exec tsc --noEmit` ثم `pnpm exec vitest run apps/web` (153 اختباراً) ثم `pnpm -C apps/web exec vite build`.
3. إعادة توليد الجسر إن غيّرت Rust:
   `cargo build -p sf-wasm --target wasm32-unknown-unknown --release`
   ثم `~/.cargo/bin/wasm-bindgen --target web --out-dir apps/web/src/studio/generated --out-name sf_wasm target/wasm32-unknown-unknown/release/sf_wasm.wasm`
4. في التطبيق: سجّل الدخول، افتح تبويب **الاستوديو الرباعي**، استورد CSV، حرّر خلية، اضغط شريحة المؤشر لرؤية بصمة التحقق، ثم صدّر الكبسولة.

## ما الذي فشل أو لم يكتمل

- **لم يُجرَّب في المتصفح بصرياً**: الشل يفرض بوابة المصادقة ولا أملك حساباً لتسجيل الدخول، فلم أفتح الجلسة ولم ألتقط صورة. التأكد البصري مطلوب منك.
- **الجلسة على الخيط الرئيسي**، لا Worker بعد: الملفات الكبيرة قد تجمّد الواجهة لحظياً. لا ادعاء 16ms ولا 500 ألف صف؛ لا قياس أداء منفّذ.
- **شريط الأوامر (DSL) في الواجهة** غير موصول: المحرك والجسر يدعمان `apply_command`، لكن لا حقل إدخال للمستخدم بعد.
- **محررا الشرائح والمستند** (Phase 3) و**Free-Form** لم يُبنيا؛ الكبسولة تصدير فقط.
- **لا AI/WebLLM** ولا `render_narrative` في الواجهة (المحرك والجسر جاهزان).
- **لم أدفع** إلى GitHub (Commits محلية على `main`).
- حدود معروفة: الجدول يعرض أول 50 صفاً فقط؛ Sed للـUndo يعيد البناء من الأصل؛ الأعمدة المحسوبة `Unknown` النوع.

## قرارات أحتاجها منك

1. **تصريح الدفع** إلى `main` (ثم النشر إن أردت).
2. **نقل الجلسة إلى Worker** الآن أم بعد محررات الشرائح.
3. **نطاق محررات Phase 3**: شرائح فعلية (Tiptap/ProseMirror؟) أم عرض من قوالب Rust فقط في هذه النسخة.
4. **مزود AI**: WebLLM محلي (بطيء أول مرة، بلا مفاتيح) أم مزود سحابي بمفتاح.

## الملفات التي تغيّرت

- `crates/sf-wasm/src/studio.rs` (جديد)، `lib.rs`، `Cargo.toml`
- `crates/sf-io/Cargo.toml` (قيود `zip`)
- `crates/sf-recipe/src/datagraph.rs` (`metric_specs`/`to_snapshot`/`to_json`/`from_json`)
- `apps/web/src/studio/{engine.ts,studio.css,generated/*}`، `apps/web/src/components/studios/DataStudio.tsx`
- `apps/web/src/components/layout/{FloatingGlassDock,CommandPalette}.tsx` + اختباراتهما، `apps/web/src/{ApplicationShell,App.shell.test}.tsx`
- `apps/web/public/_headers`، `apps/web/src/security.test.ts`
- `Cargo.lock`، `docs/DECISIONS.md`، خطة وتقرير STUDIO-Phase-1

## نتائج الفحص الفعلية

| الفحص | النتيجة |
| --- | --- |
| `cargo test --workspace --offline` | 34 حزمة/هدف «ok» بلا فشل |
| `sf-wasm` | 12 ناجحاً (7 للجسر) |
| Clippy `-D warnings` (sf-wasm, sf-io, sf-recipe) | ناجح |
| `cargo fmt --check` | ناجح |
| `gen-errors --check` | 75 كوداً متسقاً |
| `verify-i18n` | 17 مفتاحاً متسقاً |
| `tsc --noEmit` | ناجح |
| Vitest | 153/153 في 50 ملفاً |
| `vite build` | ناجح (wasm 1.66MB، gzip 608KB) |

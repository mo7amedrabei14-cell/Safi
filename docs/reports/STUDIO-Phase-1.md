> **2026-11-11 — قرار المالك:** الويب موقع عرض للمشروع فقط. لا تشغيل لمحرك الاستوديو داخل المتصفح؛ التنفيذ يكون داخل التطبيقات (سطح المكتب/CLI). أُزيل تبويب الاستوديو وجسر WASM من `apps/web` بـ`git revert`، والمحرك في Rust باقٍ كما هو.

# تقرير STUDIO-Phase-1

**التاريخ:** 2026-11-11
**الحالة:** محرك Phase 1 الحسابي منفّذ ومختبر. الواجهة والجسر إلى المتصفح لم يُبنيا بعد؛ لا انتقال إلى Phase 2.
**خط الأساس:** `119c361` على `main`، والعمل الحالي على `main` بلا فروع.
**Commits:** `2726c04` (صحة التنظيف) · `0653971` (محرك الاستوديو + تحصين XLSX) · `b124b07` (ADR-0013).

## ما الذي أُنجز

- **محرك `DataGraph` حقيقي في Rust** (`crates/sf-recipe/src/datagraph.rs`): أصل أصلي غير قابل للتعديل، سجل تحويلات قابل لإعادة التنفيذ، أعمدة محسوبة تُعاد اشتقاقها بالترتيب، مؤشرات بمعادلات محدودة (SUM/AVG/COUNT/MIN/MAX + حساب)، تحديث تزايدي عبر اعتماديات الأعمدة، سيناريوهات What-If، وUndo/Redo لا يلمس المصدر.
- **`VerifiedMetricToken`** (`crates/sf-core/src/datagraph.rs`): يربط القيمة بالمعادلة وإصدارها وإصدار البيانات وبصمة المصدر والصفوف والأعمدة المساهمة وسجل التحويلات ذات الصلة، بختم `BLAKE3` **لا يتضمن وقت الحساب**، مع `Verified/Stale/Unverified` و`verify_against`/`verify_strict` (`SF-CAL-008`).
- **أكواد أخطاء مجال الحساب**: أُضيفت `SF-CAL-001`..`SF-CAL-008` إلى `errors.toml` ووُلّد الكتالوج (Rust + TypeScript) عبر `tools/gen-errors.py` في وضع التحقق `--check`.
- **إكمال شريحة 1B**: التطبيع يحفظ النص الأصلي والنوع، ومفتاح التكرار بنيوي، وCSV/XLSX محدودان قبل التخصيص، مع حاجز إحداثيات الخلايا وحد أبعاد الورقة في preflight، ورفض المراجع المرمّزة في أجزاء الميتاداتا فقط (مراجع الدمج المرمّزة داخل ورقة العمل مدعومة ومختبرة).
- **إصلاح تعارض**: كان وكيل آخر يكتب على `crates/sf-io/src/reader_excel.rs` كل ~15 ثانية أثناء المراجعة (ملف لا يُترجم، 6 أخطاء). أُوقف العمل المتوازي، وأُصلحت أخطاء الترجمة والاختبارات، ثم بُني الأساس الأخضر.

## كيف تجرّبه بنفسك

1. `cargo test --locked --offline -p sf-core -p sf-recipe -p sf-io -p sf-clean -p sf-arabic`
2. `cargo clippy --offline -p sf-core -p sf-recipe -p sf-io -p sf-clean -p sf-arabic --all-targets -- -D warnings`
3. `cargo fmt --all -- --check`
4. `py -3 tools/gen-errors.py --check` ثم `py -3 tools/verify-i18n.py`
5. `pnpm exec vitest run apps/web/src/ApplicationShell.test.tsx apps/web/src/App.shell.test.tsx apps/web/src/components/auth/AuthModal.test.tsx`
6. لا تتوقع تغييرًا في الواجهة: البوابة ما زالت إلزامية ولم يضف أي مسار دخول.

## نتائج التشغيل الفعلية

| الفحص                    | النتيجة                           |
| ------------------------ | --------------------------------- |
| `sf-core`                | 17 ناجحًا (منها 5 لتوكنات التحقق) |
| `sf-recipe`              | 14 ناجحًا (11 منها قبول Phase 1)  |
| `sf-io`                  | 36 ناجحًا بعد إصلاح preflight     |
| `sf-clean` + `sf-arabic` | 10 + 4 ناجحة                      |
| Clippy للأهداف كلها      | ناجح مع `-D warnings`             |
| `cargo fmt --check`      | ناجح                              |
| `gen-errors --check`     | 75 كودًا متسقًا                   |
| `verify-i18n`            | 17 مفتاحًا متسقًا                 |
| Vitest بوابة المصادقة    | 13/13                             |

اختبارات قبول Phase 1 المغطاة: تعديل خلية يحدّث المؤشرات التابعة فقط ويترك غير التابعة ببصمة ووقت حساب متطابقين؛ الأربعة تقرأ نفس التوكن؛ إعادة الحساب حتمية و`replay() == working()`؛ تغيير المصدر يُبطل التحقق السابق؛ تغيير المعادلة يرفع `formula_version` ويُبطله؛ الصفوف المصدرية قابلة للوصول ومجموعها يطابق القيمة؛ رقم معروض محرَّف يُكشف؛ Undo/Redo لا يمسّ الأصل؛ السيناريوهات لا تغيّر البيانات؛ أخطاء `SF-CAL-001/002/003/004/005/008` تظهر بأكوادها.

## ما الذي فشل أو لم يكتمل

- **لا واجهة ولا جسر WASM**: لا `VerifiedNumberChip`، ولا جدول حي، ولا لوحة/شرائح/مستند. السبب: `target wasm32-unknown-unknown` و`wasm-bindgen-cli` غير مثبتين على الجهاز (AUDIT-05)، ولم يُطلب تصريح تثبيتهما.
- **لا تحقق في المتصفح** ولا Benchmark: لا ادعاء 16ms ولا 500 ألف صف.
- **لا AI ولا WebLLM** ولا DSL (Phase 2)، ولا قوالب/كبسولة/حفظ مشروع (Phases 3–4).
- **لا Push ولا CI**: `gh` غير متاح، والدفع إلى `main` يمكّن النشر؛ بُقيت الـCommits محلية حتى تصريحك.
- **حدود معروفة**: Undo/Redo يعيد البناء من الأصل (ليس فروقات فقط)؛ المتوسط مقرّب إلى 6 منازل؛ الأعمدة المحسوبة `ColumnType::Unknown`؛ لا فلاتر على المؤشرات بعد.

## قرارات أحتاجها منك

1. **تصريح تثبيت أدوات WASM** (`rustup target add wasm32-unknown-unknown` + `cargo install wasm-bindgen-cli --version 0.2.99`) لبناء الجسر وعرض `VerifiedNumberChip` فعليًا.
2. **تصريح الدفع إلى `main`** (وCI/`format-repair-once` قد يكتبان commits) بعد نجاح `make check`.
3. **استراتيجية الكبسولة** في Phase 4: تضمين محرك WASM داخل `.safi.html` (تفاعلية حقيقية، حجم كبير) أم Snapshot بلا إعادة حساب.

## الملفات التي تغيّرت

- `crates/sf-core/src/datagraph.rs` (جديد) و`value.rs` و`lib.rs`
- `crates/sf-recipe/src/datagraph.rs` و`datagraph/tests.rs` (جديد) و`lib.rs` و`Cargo.toml`
- `crates/sf-io/src/reader_excel.rs` و`reader_csv.rs` و`lib.rs` و`Cargo.toml`
- `crates/sf-clean/src/cleaner.rs` و`tests.rs`، `crates/sf-arabic/src/normalizer.rs`
- `errors.toml`، `crates/sf-core/src/generated_errors.rs`، `packages/i18n/src/generated-errors.ts`
- `Cargo.toml`، `Cargo.lock`، `docs/DECISIONS.md`
- وثائق المهام والخطط للمراحل الخمس، تقرير التدقيق، وهذا التقرير

## التنفيذ داخل تطبيق سطح المكتب (2026-11-11)

- **طبقة أوامر كاملة في Rust**: `crates/sf-desktop/src/studio.rs` تحفظ مشروعاً لكل معرّف وتنفّذ الاستيراد من القرص (حد 50 MiB، رفض الامتدادات غير المدعومة قبل الفتح)، تعديل خلية، عمود محسوب، مؤشر، أمر DSL، Undo/Redo، تجميع، سرد، حفظ/استعادة، وتصدير كبسولة بكتابة ذرّية (ملف مؤقت ← `fsync` ← إعادة تسمية) دون لمس الملف الأصلي.
- **13 أمر Tauri** في `apps/desktop/src-tauri/src/main.rs`، وكلها مضافة إلى `ALLOWED_COMMANDS` (17 أمراً إجمالاً). الويب بلا أي تنفيذ.
- **كودان جديدان**: `SF-SYS-015` (مشروع غير مفتوح) و`SF-SYS-016` (فشل كتابة الكبسولة)، مع تحديث الكتالوج (77 كوداً).
- **ADR-0014** يوثّق القرار: التنفيذ في التطبيق، والعرض في الموقع.

| الفحص                                        | النتيجة                      |
| -------------------------------------------- | ---------------------------- |
| `cargo test -p sf-desktop`                   | 56 ناجحاً (منها 8 للاستوديو) |
| Clippy `-D warnings` (sf-desktop, sf-recipe) | ناجح                         |
| `cargo test --workspace`                     | 34 هدفاً «ok» بلا فشل        |
| `gen-errors --check`                         | 77 كوداً متسقاً              |
| `verify-i18n`                                | 17 مفتاحاً                   |
| `check-no-network.sh`                        | 10 حزم محرك بلا شبكة         |

**ما زال ناقصاً**: نافذة الاستوديو داخل تطبيق سطح المكتب نفسها (الأوامر جاهزة لها)، ولا تحقق بصري بعد.

## شاشة الاستوديو داخل نافذة التطبيق (2026-11-11)

- **الشاشة**: `apps/web/src/desktop/DesktopStudio.tsx` تفتح الملف عبر حوار النظام، تعدّل الخلايا، تعرّف مؤشرات وأعمدة محسوبة، ترسم لوحة تجميع، تعرض الشذوذ، وتصدّر الكبسولة إلى مسار تختاره — كل حساب عبر أوامر Tauri.
- **البوابة**: `isDesktopRuntime()` يتحقق من جسر IPC؛ في المتصفح تظهر رسالة «الاستوديو يعمل داخل التطبيق» ولا يُنفَّذ شيء، وأي نداء خارج التطبيق يفشل بـ`SF-SYS-010` (مُختبَر).
- **الشريط**: `STUDIO_TAB` يُضاف إلى التبويبات فقط عند كشف سطح المكتب؛ الموقع يبقى 6 تبويبات كما تشهد اختباراته.
- **الاختبارات**: 3 اختبارات جديدة لسطح المكتب + تحديث مفاتيح لوحة الأوامر. المجموع 156/156 في 51 ملفاً.

**ما زال ناقصاً**: لم أُشغّل نافذة التطبيق نفسها (تحقق بصري)، والجدول يعرض أول 50 صفاً، وشريط أوامر DSL بلا حقل إدخال بعد.

---

## إكمال Phase 1 النهائي (2026-11-15 — `eb7874959d9bff34d4867da1a2cb09bec87b04bb`)

### ما الذي أُنجز

- **هويات صفوف وأعمدة مستقرة (`RowId` / `ColumnId`)**: صارت مراجع الصفوف والأعمدة في `crates/sf-recipe/src/datagraph.rs` و`crates/sf-core/src/datagraph.rs` مستقلة عن ترتيب العرض، وتصمد عبر الفرز والتصفية والحذف والإدراج والاستعادة، مع إبطال التوكن فورًا عند أي عبث أو نقص في السلسلة.
- **مكون `VerifiedNumberChip` وواجهة `WhatIfPanel`**: ربطنا شريحة القيم الموثقة بنافذة تدقيق تعرض الصيغة والإصدار وبصمة `BLAKE3` وتصفّح صفوف المصدر (25/صفحة) وتاريخ التحويلات، ومعها لوحة سيناريوهات What-If ومنزلقات الأرقام والمقارنة المباشرة عبر Rust فقط.
- **تحديث تفاعلي وتقسيم صفحات حقيقي (`studio_rows_page`)**: أي تعديل خلية يوسم القيم القديمة كـ`Stale` ثم يعيد حساب المؤشرات المعتمدة تلقائياً في Rust مع رفض الردود المتأخرة (`revision < current`) ونقل صفحة واحدة فقط بحد أقصى آمن (25–200 صف).

### كيف تجرّبه بنفسك

1. نفّذ `cargo test -p sf-core -p sf-recipe -p sf-desktop`.
2. نفّذ `pnpm exec vitest run apps/web/src/desktop/desktop-studio.test.tsx`.
3. داخل تطبيق سطح المكتب افتح ملف CSV، اضغط على أي شريحة رقم موثق لفتح نافذة التدقيق، وعدّل خلية أو بدّل سيناريو في لوحة What-If لمشاهدة التحديث التلقائي.

### ما الذي فشل أو لم يكتمل

- لا شيء ضمن نطاق Phase 1 في تطبيق سطح المكتب؛ التضمين داخل المتصفح العام يبقى ممنوعاً بقرار المالك `ADR-0014` (`SF-SYS-010`).

### قرارات أحتاجها منك

- مراجعة التجربة داخل نافذة Tauri عند الرغبة قبل الانتقال إلى المرحلة التالية.

### الملفات التي تغيّرت

- `crates/sf-core/src/datagraph.rs`، `crates/sf-recipe/src/datagraph.rs`، `crates/sf-desktop/src/studio.rs`
- `apps/desktop/src-tauri/src/main.rs`
- `apps/web/src/desktop/studio-bridge.ts`، `apps/web/src/desktop/DesktopStudio.tsx`، `apps/web/src/desktop/components/VerifiedNumberChip.tsx`، `apps/web/src/desktop/components/WhatIfPanel.tsx`، `apps/web/src/desktop/desktop-studio.test.tsx`

---

## تحديث التحصين النهائي (v6 — 2026-11-15)

- **حفظ واستعادة السيناريو النشط (`active_scenario`)**: حُدّث `ProjectSnapshot` و`DataGraph::from_json` في `crates/sf-recipe/src/datagraph.rs` لحفظ واستعادة السيناريو النشط وإعادة حساب جميع المؤشرات وفق المتغيرات النشطة عند فتح المشروع المحفوظ.
- **بوابة مصادقة Native في Tauri (`SF-AUTH-001` / `SF-AUTH-002`)**: رُبطت جميع أوامر Phase 1 في `StudioStore` بـ`gate_command` الذي يفرض `authorize_command` و`validate_ipc_payload_len` (`MAX_IPC_BYTES = 64 KiB`) والتحقق الفعلي من الجلسة النشطة وهوية الجهاز الوحيد قبل أي قراءة أو تعديل.
- **إثبات E2E في Chromium + Rust `StudioStore`**: جرى التحقق من رحلة Phase 1 كاملة عبر `tools/verify-studio-desktop-e2e.mjs` (تعديل الخلية `r1` من `100` إلى `250` وتحديث `VerifiedNumberChip` من `350 EGP` إلى `500 EGP` ثم تفعيل سيناريو `What-If` لتصبح `1000 EGP`).

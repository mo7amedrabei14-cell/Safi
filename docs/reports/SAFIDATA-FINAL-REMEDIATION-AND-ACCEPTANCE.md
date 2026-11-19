# صافي البيانات (SafiData) — تقرير القبول النهائي ومتابعة أمان وسلامة XLSB

> أُعد هذا التقرير باللغة العربية البسيطة وفق `AGENTS.md` ومهارة `i-have-adhd`، وبالتحقق الفعلي على فرع `main` المنشور حاليًا على GitHub عند `96207381e89923ccceca9569de83b67c30731b5e` (حيث `HEAD` المحلي يطابق `origin/main` تمامًا) **دون** تنفيذ `commit` أو `push` أو إنشاء فروع أو `P0.06`. التغييرات الموثّقة أدناه موجودة في شجرة العمل المحلية ولم تُرفع بعد، وبالتالي لم يُرصد تشغيل GitHub CI عليها بعد.

---

## 1. ما الذي أُنجز

- **P0 — منع استنزاف الذاكرة غير المحدود في قراءة XLSB (`crates/sf-io/src/xlsb_reader.rs` و`crates/sf-io/src/workbook_intelligence.rs`)**:
  - التحقق الصريح من إحداثيات الصف (`0..=1_048_575`) والعمود (`0..=16_383`) وإرجاع `SF-IMP-002` فورًا لأي إحداثي خارج حدود Excel.
  - التحقق بحساب آمن من الفائض (`checked_*`) لمساحة النطاق الكثيف (`(max_r - min_r + 1) * (max_c - min_c + 1)` و`(max_r + 1) * (max_c + 1) <= 2_000_000`، وكذلك `span_rows <= max_rows_limit + 64`) بشكلٍ مستقل لكلٍ من `value_cells` و`formula_cells` **قبل** أي استدعاء لـ `calamine::Range::from_sparse()` وقبل تخصيص الشبكة الكثيفة في `build_worksheet_intelligence`، مع إرجاع `SF-IMP-005` دون الاعتماد على `catch_unwind`، وتقييد مساحة الخلايا المدمجة `BrtMergeCell` بـ `100_000` خلية.
  - إضافة عيّنات ثنائية عدائية (`adversarial_sparse_corners.xlsb` لقيم `A1` + `XFD1048576`، و`adversarial_sparse_formula_corners.xlsb` لصيغ `A1` + `XFD1048576`) واختبارات انحدار تثبت الرفض الفوري بـ `SF-IMP-005`، بالإضافة إلى اختبار خلية منفردة عند `XFD1048576` وإحداثي خارج الحدود (`SF-IMP-002`).
- **P1 — رفض تيارات BIFF12 التالفة والمبتورة + الحفاظ على كشف الصيغ غير المدعومة**:
  - تحويل `Biff12Iter::next_record()` إلى `Result<Option<(u16, &[u8])>, SafiError>` للتمييز الصارم بين نهاية الملف النظيفة (`Ok(None)`) وبين التلف أو البتر (بتر معرّف السجل ثنائي البايت، بت استمرار خاطئ، طول `varint` مبتور أو لا ينتهي عند 4 بايت، أو بتر جسم السجل)، مع التحقق من إغلاق سجلات البداية/النهاية (`Book`/`Sheet`/`SST`/`Styles`/`Comments`) وتمرير الأخطاء بـ `?` دون أي نجاح جزئي صامت، وتغطية ذلك بعيّنة `malformed_truncated_sheet.xlsb` واختبارات بتر `workbook.bin` و`varint`.
  - منع إسقاط الصيغ غير المفكوكة في `0x0008..=0x000B` أو خفض نتائجها المخزنة مؤقتًا إلى `LiteralValue`: تُسجَّل الصيغة صراحةً بالعلامة `=(unsupported BIFF12 formula)` مع الاحتفاظ بالقيمة المخزنة في `cached_value` وتصنيفها كـ `FormulaVerificationStatus::UnsupportedFormula` وضمّها في عدد الصيغ المكتشفة والتحذيرات الشفافة (`outside local recalculation ... undecoded binary`)، مع اختبار انحدار على عيّنة `xlsb_unsupported_formula_cached.xlsb`.
- **P2 — توحيد إسقاط الرسم الجغرافي وحذف `tools/vendor/picomatch/` غير المستخدم**:
  - توحيد إحداثيات رسم `Geographic` بين لوحة سطح المكتب (`DashboardChartsView.tsx`) وكبسولة `.safi.html` (`crates/sf-recipe/src/studio_capsule.rs`) ورسوم SVG والعروض المصدّرة (`apps/web/src/utils/quad-studio-engine.ts`) عبر دالة `computeRegionalCoordinatePct` المطابقة حرفيًا لـ `geo_region_coords` في `crates/sf-recipe/src/datagraph.rs` مع الإقرار الصريح `"Regional coordinate scatter (no GIS boundaries implied)"` واختبار تطابق آلي في `quad-studio-engine.test.ts`.
  - التحقق من عدم وجود أي مرجع برمجي أو بنوي لـ `tools/vendor/picomatch/` وحذف المجلد بالكامل من شجرة العمل مع الإبقاء على الاعتماد الرسمي `"picomatch": "4.0.7"` في `package.json` و`pnpm-lock.yaml`.

---

## 2. كيف تجرّبه بنفسك

1. فحص حالة Git والتأكد من المطابقة مع `origin/main`:
   - `git branch --show-current` و`git rev-parse HEAD` و`git ls-remote origin refs/heads/main` و`git status --short`
2. فحص Rust وTauri (بما فيها اختبارات XLSB العدائية الجديدة في `sf-io`):
   - `cargo fmt --all -- --check`
   - `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
3. فحص الجافاسكريبت والحوكمة والأمان وعدم وجود شبكة في المحرك:
   - `pnpm install --frozen-lockfile --ignore-scripts`
   - `pnpm run format:check` و`pnpm run typecheck` و`pnpm run lint` و`pnpm run test`
   - `pnpm run check:contrast` و`pnpm audit --audit-level=moderate`
   - `py -3 -B tools/verify-i18n.py` و`py -3 -B tools/verify-security.py` و`py -3 -B tools/gen-errors.py --check` و`py -3 -B tools/verify-migrations.py`
   - `& "C:\Program Files\Git\bin\bash.exe" tools/check-no-network.sh --self-test` ثم `tools/check-no-network.sh`
4. فحص إعدادات الإطلاق والبناء والمتصفح الحقيقي والصادرات الـ 9 وسطح المكتب E2E:
   - `node tools/verify-desktop-build-config.mjs` ثم `pnpm run build`
   - `node tools/verify-intro-visual.mjs`
   - `node tools/verify-ux-e2e.mjs`
   - تعيين `PLAYWRIGHT_MODULE_URL` إلى Playwright المحلي ثم `node tools/verify-device-identity.mjs`
   - `node tools/verify-capsule-chromium.mjs`
   - `node tools/verify-exports-roundtrip.mjs`
   - `node tools/verify-studio-desktop-e2e.mjs`

---

## 3. نتائج التحقق النهائية (على آخر نسخة كود نهائية دون أي تعديل لاحق)

| الفحص / الأمر | النتيجة الفعلية المرصودة | أرقام وملاحظات موثقة من التشغيل الأخير |
| :--- | :--- | :--- |
| مطابقة `HEAD` مع `origin/main` | مطابق | `96207381e89923ccceca9569de83b67c30731b5e` على المحلي وعلى `refs/heads/main` في `origin` |
| تعارضات الدمج + `tools/vendor` + `git diff --check` | ناجح | `0` علامات تعارض، `0` مراجع لـ `vendor/picomatch` أو `tools/vendor` في الكود والاعتماديات، `git diff --check` خروج `0` |
| `cargo fmt --check` و`cargo clippy --all-targets -- -D warnings` (Workspace + `src-tauri`) | ناجح | `0` تحذيرات و`0` أخطاء |
| `cargo test --workspace` | ناجح (مع 3 متجاهلة) | **`294` ناجح، `0` فشل، `3` متجاهلة** (`#[ignore]` في `sf-server` تتطلب PostgreSQL 18 عبر `SF_TEST_DATABASE_URL`) — تشمل `42` اختبار وحدة و`10` اختبارات انحدار في `sf-io` |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | ناجح (مع 1 متجاهل) | **`2` ناجح، `0` فشل، `1` متجاهل** (`benchmark_native_session_verifier_performance` قياس أداء TLS يدوي) |
| `pnpm install --frozen-lockfile --ignore-scripts` | ناجح | `Lockfile is up to date` (`all 9 workspace projects`) |
| `pnpm run format:check` / `typecheck` / `lint` | ناجح | خروج `0` عبر 8 مشاريع في `pnpm -r` |
| `pnpm run test` (Vitest) | ناجح | **`66` ملف اختبار / `232` اختبارًا ناجحًا، `0` فشل، `0` تخطي** (يشمل اختبار تطابق إحداثيات `Geographic` مع `datagraph.rs`) |
| `pnpm run check:contrast` | ناجح | `46` زوج نص (`min 4.58:1 >= 4.5:1`) و`36` زوج واجهة/رسوم (`min 4.37:1 >= 3.0:1`) + نجاح `--self-test` |
| `pnpm audit --audit-level=moderate` | ناجح | `No known vulnerabilities found` (`picomatch@4.0.7` الرسمي في `devDependencies` فقط) |
| `verify-i18n.py` / `verify-security.py` / `gen-errors.py --check` / `verify-migrations.py` / `check-no-network.sh` | ناجح | `28` مفتاح i18n؛ `110` مهام موثقة؛ `errors.toml` مطابق؛ `6` أزواج هجرة DB؛ `0` اعتماديات شبكة في `10` حزم محرك (+ نجاح `--self-test`) |
| `node tools/verify-desktop-build-config.mjs` ثم `pnpm run build` | ناجح | رفض الأصول المفقودة/المحجوزة/غير HTTPS في الويب وTauri Release، ومطابقة `https://ci-verify.safidata.dev` بين الحزمة و`safi-desktop.exe`، ثم استعادة بناء الإنتاج القياسي |
| `node tools/verify-intro-visual.mjs` | ناجح | `1440x900` (3G + 4x CPU): دخول `377.3ms`، خروج `756ms`، `reducedMotionPassed: true`، `pageErrors: []` |
| `node tools/verify-ux-e2e.mjs` | ناجح | مصفوفة المقاسات الـ 7 (`tabAlignedDelta: 0` بما فيها `320x700` RTL)، تمرير نافذة الدخول، `authMountCount: 1`، `0` عناصر سطح مكتب في الموقع العام |
| `node tools/verify-device-identity.mjs` | ناجح | معرّف التثبيت ثابت عبر الصفحات في IndexedDB (`stableAcrossPages: true`, `localStorageUsed: false`) |
| `node tools/verify-capsule-chromium.mjs` | ناجح | `.safi.html` عبر `file://`: `scripts: 0`, `imagesLoaded: 1`, `externalRequests: 0`, `consoleErrors: 0`, `pageErrors: 0` |
| `node tools/verify-exports-roundtrip.mjs` | ناجح | التحقق المستقل عبر Python (`xlsx_sheets`: 4 أوراق، `png_colors`: 8، `pdf_pages`: 7 عند `xref=660742`) + `21` رسم SVG متميز (`svgWidth: 420`) + مشغل `.safi.html` (`slideNodes: 10`, `zeroSizedSvgs: 0`) |
| `node tools/verify-studio-desktop-e2e.mjs` | ناجح | كامل خطوات سطح المكتب ضد `sf-server` الحقيقي و`studio_ipc_harness` (بما فيها `messy_enterprise.xlsx` و`enterprise_binary.xlsb` والتصفية المتقاطعة والتصدير الذري) |
| الحفاظ على الموقع العام مقارنةً بـ `50d473581ab0e2318bf149cded2e0ee6061527f6` | ناجح | `0` ملفات معدّلة في `apps/site`، `packages/`، `apps/web/src/pages`، `home`، `layout`، `brand`، `App.tsx`، و`ApplicationShell.tsx` |

---

## 4. ما الذي لم يكتمل أو بقي مقيدًا بحدود البيئة أو التصميم (بصراحة تامة)

- **حالة GitHub CI**: لم تُلتزم التغييرات الحالية ولم تُرفع إلى `origin/main` احترامًا لتعليمات عدم تنفيذ `commit` أو `push`، ولذلك لم يُرصد بعد تشغيل GitHub Actions CI على هذه التغييرات المحلية.
- **اختبارات متجاهلة محليًا (4 اختبارات إجمالًا)**:
  - `3` اختبارات في `sf-server` (`#[ignore]`) تتطلب مثيل PostgreSQL 18 حيًا عبر `SF_TEST_DATABASE_URL` غير المتوفر في هذه البيئة.
  - `1` اختبار أداء يدوي في `apps/desktop/src-tauri` (`benchmark_native_session_verifier_performance`).
- **إطار اختبار سطح المكتب**: يشغّل `tools/verify-studio-desktop-e2e.mjs` خادم `sf-server` الحقيقي و`studio_ipc_harness` الحقيقي داخل Chromium (عبر جسر `__TAURI_INTERNALS__`) وليس عبر WebDriver خارجي لنافذة WebView2 الأصلية، مع تحقق إضافي من بناء Release الحقيقي `safi-desktop.exe` عبر `verify-desktop-build-config.mjs`.
- **قيود رسم `Geographic`**: الرسم هو مخطط تبعثر إحداثي للمناطق المصرية المعروفة وتوزيع حتمي للفئات غير المعرّفة (مطابق الآن عبر لوحة سطح المكتب وكبسولة `.safi.html` وSVG المصدّر) وليس محرك خرائط GIS حدوديًا، وهو ما يُعرض صراحةً في التسمية التوضيحية.
- **بيئة Windows لـ `check-no-network.sh`**: يتطلب التشغيل عبر Git Bash (`C:\Program Files\Git\bin\bash.exe`) لأن `bash` الافتراضي قد يفتح WSL دون `cargo.exe`.

---

## 5. قرارات أحتاجها منك

- مراجعة التغييرات المحلية غير المُلتزمة (إصلاحات P0/P1 لـ XLSB، توحيد إحداثيات `Geographic`، وحذف `tools/vendor/picomatch/`) واعتمادها قبل الإذن بعملية `commit` و`push` إلى `main` وتشغيل GitHub CI عليها.

---

## 6. الملفات التي تغيّرت أو أُضيفت أو حُذفت (من `git status --short` الحالي)

- **المرجع الأساسي على `main`**: `96207381e89923ccceca9569de83b67c30731b5e` (مطابق لـ `origin/main`).
- **محرك XLSB وذكاء المصنفات واختباراته**:
  - `crates/sf-io/src/xlsb_reader.rs` (معدّل)
  - `crates/sf-io/src/workbook_intelligence.rs` (معدّل)
  - `crates/sf-io/tests/real_workbooks_regression.rs` (معدّل)
  - `tests/fixtures/generate_workbooks.py` (معدّل)
  - `tests/fixtures/workbooks/adversarial_sparse_corners.xlsb` (مضاف)
  - `tests/fixtures/workbooks/adversarial_sparse_formula_corners.xlsb` (مضاف)
  - `tests/fixtures/workbooks/malformed_truncated_sheet.xlsb` (مضاف)
  - `tests/fixtures/workbooks/xlsb_unsupported_formula_cached.xlsb` (مضاف)
- **توحيد الرسم الجغرافي واختباره**:
  - `apps/web/src/utils/quad-studio-engine.ts` (معدّل)
  - `apps/web/src/utils/quad-studio-engine.test.ts` (معدّل)
- **إزالة الحزمة المورّدة القديمة غير المستخدمة**:
  - `tools/vendor/picomatch/` (محذوف بالكامل — 10 ملفات)
- **التقرير ومخرجات التحقق البصري المحدّثة**:
  - `docs/reports/SAFIDATA-FINAL-REMEDIATION-AND-ACCEPTANCE.md` (معدّل)
  - `artifacts/intro-visual/` (تحديثات التشغيل الأخير لـ `intro-auth-gate-handoff.png` و`intro-opening-motion.png` و`studio-desktop-e2e.png` و`visual-check.json` وملفات `studio-e2e/`)

# صافي البيانات (SafiData) — تقرير القبول النهائي ومراجعة استنفاد الموارد وسلامة تيارات XLSB

> أُعد هذا التقرير باللغة العربية البسيطة وفق `AGENTS.md` ومهارة `i-have-adhd`، وبالتحقق الفعلي على فرع `main` عند الالتزام المنشور حاليًا على GitHub (`0409edc79d9b3254d8d763bb123e88f770e2b1fa`). يتطابق `HEAD` المحلي مع `origin/main` تمامًا (`0409edc79d9b3254d8d763bb123e88f770e2b1fa`)، بينما تقع إصلاحات هذه الجولة (P0 وP1 وP2) كـ تعديلات محلية غير مُلتزمة على قمة `0409edc79d9b3254d8d763bb123e88f770e2b1fa` تنفيذًا لتعليمات عدم تنفيذ `commit` أو `push` أو إنشاء فروع أو بدء `P0.06`.

---

## 1. حالة المزامنة والتاريخ بين المحلي وGitHub (`origin/main`)

- **الفرع النشط**: `main`
- **`HEAD` المحلي**: `0409edc79d9b3254d8d763bb123e88f770e2b1fa`
- **`origin/main` البعيد (`git ls-remote origin refs/heads/main`)**: `0409edc79d9b3254d8d763bb123e88f770e2b1fa` (متطابقان تمامًا)
- **تسلسل الالتزامات المنشورة حتى الآن على `main`**:
  1. `2255a5b` — `style: apply repository formatters`
  2. `008fe42` — `Update`
  3. `fdc13b6` — `Update`
  4. `9620738` — `Update` (نشر إصلاحات الدمج والـ XLSB الأصلية و`picomatch@4.0.7`)
  5. `0409edc` (`0409edc79d9b3254d8d763bb123e88f770e2b1fa`) — `Update` (نشر إصلاحات الجولة السابقة: حد النطاق الكثيف لـ XLSB ورفض تيارات BIFF12 المبتورة وحفظ الصيغ غير المفكوكة وحذف `tools/vendor/picomatch/`)
- **حالة شجرة العمل الحالية**: تعديلات محلية غير مُلتزمة وغير مرفوعة فوق `0409edc79d9b3254d8d763bb123e88f770e2b1fa` لإصلاحات هذه الجولة فقط؛ لم يُنفَّذ أي `commit` أو `push` أو `deploy`.
- **حالة GitHub Actions CI**: **أدلة تشغيل GitHub CI غير متوفرة/غير مرصودة هنا** لا للالتزام `0409edc79d9b3254d8d763bb123e88f770e2b1fa` ولا للتعديلات المحلية غير المرفوعة، ولا يُدّعى في هذا التقرير نجاح GitHub CI دون رابط تشغيل فعلي ناجح للالتزام المطابق.

---

## 2. ما الذي أُنجز في هذه الجولة

- **P0 — تقييد عدد سجلات الخلايا ورفض التكرار على نفس الإحداثي (`crates/sf-io/src/xlsb_reader.rs` و`crates/sf-io/tests/real_workbooks_regression.rs`)**:
  - فرض حدود صريحة مستقلة لعدد السجلات (`MAX_VALUE_CELL_RECORDS = 2_000_000`، `MAX_FORMULA_CELL_RECORDS = 2_000_000`، `MAX_SHEET_CELL_RECORDS = 2_000_000`، `MAX_SHEET_ROW_HEADERS = 1_048_576`، `MAX_SHARED_STRINGS = 2_000_000`، `MAX_MERGE_RECORDS = 100_000`) داخل `ExtentTracker` (عبر `checked_add(1)` ورفض `SF-IMP-005` `XlsbCellRecordCountLimitExceeded` قبل تعديل الحالة أو الإدراج في `value_cells`/`formula_cells`، ومع إعادة التحقق داخل `validate()`).
  - منع تكرار سجلات الخلايا على نفس الإحداثي `(row, col)` عبر `seen_cell_coords: HashSet<(u32, u32)>` مقيّد الحجم قبل الإدراج: أي سجل خلية مكرر (`0x0001..=0x000B`) يُرفض فورًا بـ `SF-IMP-002` (`DuplicateXlsbCellCoordinate`) عند أول تكرار قبل نمو المتجهات أو استدعاء `calamine::Range::from_sparse()`.
  - الإبقاء على التحقق المستقل لنطاقي القيم والصيغ، وإضافة اختبارات انحدار عدائية: أرشيف XLSB غير مضغوط (`Stored`) يحوي `10,000` سجل قيمة مكرر عند `A1`، وآخر يحوي `10,000` سجل صيغة مكرر عند `A1` (يتجاوز نسبة ضغط ZIP لاختبار قارئ BIFF12 مباشرة)، بالإضافة إلى اختبارات وحدة لـ `ExtentTracker::with_max_records` و`parse_xlsb_package_with_record_limits` تثبت الرفض قبل نمو المتجهات أو `Range::from_sparse()`.
- **P1 — فرض حدود وترتيب تيارات BIFF12 بصرامة (`crates/sf-io/src/xlsb_reader.rs` و`crates/sf-io/tests/real_workbooks_regression.rs`)**:
  - استبدال الفحص الشرطي `if saw_begin && !saw_end` بآلة حالة إلزامية `StreamBoundaryGuard` (إضافة إلى `OptionalSectionGuard` لأقسام `BrtBeginSheetData`/`BrtEndSheetData` و`BrtBeginCellXFs`/`BrtEndCellXFs` وحارس تداخل التعليقات `0x027B`/`0x027C`) عبر تيارات: المصنف (`0x0083`/`0x0084`)، وأوراق العمل (`0x0081`/`0x0082`)، والأنماط (`0x0116`/`0x0117`)، والنصوص المشتركة (`0x009F`/`0x00A0`)، والتعليقات (`0x0274`/`0x0275`).
  - رفض جميع حالات الإخلال الست بـ `SF-IMP-002`: غياب علامتي البداية والنهاية معًا، ظهور النهاية قبل البداية، تكرار البداية، تكرار النهاية، ظهور أي سجل قبل البداية، وظهور أي سجل بعد النهاية، مع الاستمرار في التمييز الصارم بين نهاية الملف النظيفة (`Ok(None)`) والبيانات المبتورة (`Err(SF-IMP-002)`).
  - تغطية جميع الحالات الست باختبارات وحدة (`stream_boundary_guard_enforces_strict_biff12_framing_across_all_streams`) واختبارات تكامل شاملة على ملفات XLSB كاملة للمصنف وورقة العمل و`styles.bin` و`sharedStrings.bin` و`comments1.bin`.
- **P2 — تطابق تطبيع Unicode في الرسم الجغرافي (`apps/web/src/utils/quad-studio-engine.ts` و`apps/web/src/utils/quad-studio-engine.test.ts` و`crates/sf-recipe/src/datagraph/tests.rs`)**:
  - استبدال `label.trim().toLowerCase()` في TypeScript بدالة `toAsciiLowerCase(label.trim())` (`A-Z` -> `a-z` فقط) المطابقة تمامًا لـ `label.trim().to_ascii_lowercase()` في Rust (`geo_region_coords`)، مع توثيق القاعدة المشتركة (تطبيع ASCII فقط لمطابقة المناطق المعروفة، وتجزئة بايتات UTF-8 الأصلية غير المعدّلة للفئات غير المعروفة).
  - إضافة اختبارات انحدار متناظرة في Rust (`datagraph/tests.rs`) وTypeScript (`quad-studio-engine.test.ts`) لتسميات فئات غير معروفة تحتوي على أحرف Unicode كبيرة غير لاتينية (`"ÜBER_NORD_ÖST"` -> `(73, 21)`، `"İSTANBUL_SÜD"` -> `(52, 62)`، وعلامة كلفن `"DA\u212AAHL"` -> `(49, 27)` التي كان `toLowerCase()` يحوّلها خطأً إلى Delta `(48, 28)`).

---

## 3. كيف تجرّبه بنفسك

1. فحص حالة Git والمطابقة مع `origin/main`:
   - `git branch --show-current` و`git rev-parse HEAD` و`git ls-remote origin refs/heads/main` و`git status --short`
2. فحص Rust وTauri (بما فيها اختبارات حدود السجلات وتكرار الإحداثيات وحدود التيارات وتطابق Unicode الجغرافي):
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
4. فحص إعدادات الإطلاق والبناء والكبسولة والصادرات الـ 9 وتكامل سطح المكتب E2E:
   - `node tools/verify-desktop-build-config.mjs` ثم `pnpm run build`
   - `node tools/verify-capsule-chromium.mjs`
   - `node tools/verify-exports-roundtrip.mjs`
   - `node tools/verify-studio-desktop-e2e.mjs`
   - `node tools/verify-intro-visual.mjs` و`node tools/verify-ux-e2e.mjs` و`node tools/verify-device-identity.mjs`

---

## 4. نتائج التحقق النهائية (بعد آخر تعديل في الكود، مع رموز الخروج الدقيقة)

| الفحص / الأمر | رمز الخروج | النتيجة الفعلية والأرقام الموثقة |
| :--- | :---: | :--- |
| `cargo fmt --all -- --check` + `src-tauri` | `0` | مطابق بالكامل |
| `cargo clippy --workspace --all-targets -- -D warnings` + `src-tauri` | `0` | `0` تحذيرات و`0` أخطاء |
| `cargo test --workspace` | `0` | **`300` ناجح، `0` فشل، `3` متجاهلة** (`#[ignore]` في `sf-server` تتطلب PostgreSQL 18 عبر `SF_TEST_DATABASE_URL`) — تشمل `45` اختبار وحدة و`12` اختبار انحدار في `sf-io` + اختبار `geo_region_coords` الجديد في `sf-recipe` |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | `0` | **`2` ناجح، `0` فشل، `1` متجاهل** (`benchmark_native_session_verifier_performance` قياس أداء TLS يدوي) |
| `pnpm install --frozen-lockfile --ignore-scripts` | `0` | `Lockfile is up to date` (`all 9 workspace projects`) |
| `pnpm run format:check` / `typecheck` / `lint` | `0` | نجاح كامل عبر 8 مشاريع في `pnpm -r` |
| `pnpm run test` (Vitest) | `0` | **`66` ملف اختبار / `232` اختبارًا ناجحًا، `0` فشل، `0` تخطي** (يشمل اختبار تطابق التطبيع ASCII-only وUnicode في `quad-studio-engine.test.ts`) |
| `pnpm run check:contrast` | `0` | `46` زوج نص (`min 4.58:1 >= 4.5:1`) و`36` زوج واجهة/رسوم (`min 4.37:1 >= 3.0:1`) + نجاح `--self-test` |
| `pnpm audit --audit-level=moderate` | `0` | `No known vulnerabilities found` |
| `verify-i18n.py` / `verify-security.py` / `gen-errors.py --check` / `verify-migrations.py` | `0` | `28` مفتاح i18n؛ `110` مهام موثقة؛ `errors.toml` مطابق؛ `6` أزواج هجرة DB |
| `tools/check-no-network.sh --self-test` و`tools/check-no-network.sh` (Git Bash) | `0` | اجتياز الاختبار السلبي؛ `0` اعتماديات شبكة في `10` حزم محرك |
| `node tools/verify-desktop-build-config.mjs` ثم `pnpm run build` | `0` | رفض الأصول غير الصالحة في الويب وTauri Release، ومطابقة `https://ci-verify.safidata.dev` بين الحزمة و`safi-desktop.exe`، واستعادة بناء الإنتاج القياسي |
| `node tools/verify-capsule-chromium.mjs` | `0` | `.safi.html` عبر `file://`: `scripts: 0`, `imagesLoaded: 1`, `externalRequests: 0`, `consoleErrors: 0`, `pageErrors: 0` |
| `node tools/verify-exports-roundtrip.mjs` | `0` | تحقق Python المستقل (`xlsx_sheets`: 4 أوراق، `png_colors`: 8، `pdf_pages`: 7 عند `xref=660742`) + `21` رسم SVG متميز (`svgWidth: 420`) + مشغل `.safi.html` (`slideNodes: 10`, `zeroSizedSvgs: 0`) |
| `node tools/verify-studio-desktop-e2e.mjs` | `0` | كامل الخطوات ضد `sf-server` الحقيقي و`studio_ipc_harness` (`messy_enterprise.xlsx` و`enterprise_binary.xlsb` والتصفية المتقاطعة والتصدير الذري)؛ ملاحظة: تجاوز التشغيل الأول مهلة `page.goto` (30 ثانية) تحت ضغط بناء `sf-server` ثم اجتاز بالكامل عند إعادة التشغيل المنفصل بخروج `0` |
| `node tools/verify-intro-visual.mjs` / `verify-ux-e2e.mjs` / `verify-device-identity.mjs` | `0` | دخول `386.8ms`، خروج `798.6ms`، `reducedMotionPassed: true`، مصفوفة المقاسات الـ 7 (`tabAlignedDelta: 0`)، `authMountCount: 1`، معرّف IndexedDB ثابت (`localStorageUsed: false`)، و`0` عناصر سطح مكتب في الموقع العام |

---

## 5. ما الذي فشل أو لم يكتمل أو بقي مقيدًا (بصراحة تامة)

- **أدلة GitHub Actions CI غير متوفرة**: لم تُرفع تعديلات هذه الجولة احترامًا لعدم تنفيذ `commit` أو `push`، ولم يُرصد هنا أي تشغيل GitHub CI للالتزام `0409edc79d9b3254d8d763bb123e88f770e2b1fa` أو للنسخة المحلية المعدّلة.
- **4 اختبارات متجاهلة محليًا**:
  - `3` اختبارات في `sf-server` (`#[ignore]`) تتطلب مثيل PostgreSQL 18 حيًا عبر `SF_TEST_DATABASE_URL`.
  - `1` اختبار أداء يدوي في `apps/desktop/src-tauri` (`benchmark_native_session_verifier_performance`).
- **إطار اختبار سطح المكتب**: `verify-studio-desktop-e2e.mjs` يختبر واجهة Chromium مقابل خادم `sf-server` الحقيقي ومحرك `studio_ipc_harness` الحقيقي وليس عبر WebDriver خارجي لنافذة WebView2 الأصلية، مع تحقق مستقل من بناء Release الحقيقي `safi-desktop.exe`.

---

## 6. قرارات أحتاجها منك

- مراجعة الفروقات المحلية الحالية فوق `0409edc79d9b3254d8d763bb123e88f770e2b1fa` واعتمادها قبل الإذن بعملية `commit` و`push` إلى `main` للتحقق النهائي عبر GitHub Actions CI.

---

## 7. الملفات التي تغيّرت في شجرة العمل المحلية فوق `0409edc79d9b3254d8d763bb123e88f770e2b1fa`

- `crates/sf-io/src/xlsb_reader.rs` — حدود صريحة لعدد سجلات الخلايا والقيم والصيغ ورفض الإحداثيات المكررة قبل نمو المتجهات + `StreamBoundaryGuard` و`OptionalSectionGuard` لجميع تيارات BIFF12 + 3 اختبارات وحدة جديدة.
- `crates/sf-io/tests/real_workbooks_regression.rs` — اختبارات تكامل عدائية لـ `10,000` سجل قيمة وصيغة مكرر عند `A1` (بأرشيف `Stored`) وجميع حالات إخلال حدود تيارات BIFF12 الست (المصنف وورقة العمل والأنماط والنصوص المشتركة والتعليقات).
- `crates/sf-recipe/src/datagraph/tests.rs` — اختبار تطابق إحداثيات `geo_region_coords` لتسميات Unicode غير اللاتينية ذات الأحرف الكبيرة.
- `apps/web/src/utils/quad-studio-engine.ts` — دالة `toAsciiLowerCase` وتوثيق قاعدة التطبيع المشتركة مع Rust.
- `apps/web/src/utils/quad-studio-engine.test.ts` — اختبار تطابق `computeRegionalCoordinatePct` و`renderChartSvg("Geographic", ...)` مع Rust لنفس تسميات Unicode الكبيرة (`"ÜBER_NORD_ÖST"` و`"DA\u212AAHL"` و`"İSTANBUL_SÜD"`).
- `docs/reports/SAFIDATA-FINAL-REMEDIATION-AND-ACCEPTANCE.md` — تحديث التقرير بالكامل ليطابق `0409edc79d9b3254d8d763bb123e88f770e2b1fa` ويعكس الحالة الحقيقية والنتائج الدقيقة.
- `artifacts/intro-visual/` — مخرجات التشغيل الأخير لسكربتات التحقق البصري وDesktop E2E (`intro-auth-gate-handoff.png` و`intro-opening-motion.png` و`studio-desktop-e2e.png` و`visual-check.json` و`studio-e2e/`).

# صافي البيانات (SafiData) — تقرير القبول الفني لجولة إصلاحات XLSB وسجل التحقق الموحّد

> أُعد هذا التقرير باللغة العربية البسيطة وفق `AGENTS.md` ومهارة `i-have-adhd`، وهو يوثق **القبول الفني لجولة إصلاحات XLSB فقط** على فرع `main` بعد نشر الالتزام البرمجي `94551e4579aaf1bfd335e4ce486749602885a711` والتزام تحديث التقرير `785209a2c8e3e3f62a01dca979617bc88ac203b4` واكتمال نتائج GitHub Actions لكليهما، **وليس القبول النهائي للمشروع بالكامل أو إعلان جاهزيته التجارية للإطلاق**. لم يُجرَ أي تعديل برمجي أو منطقي بعد `94551e4579aaf1bfd335e4ce486749602885a711`.

---

## 1. سجل الالتزامات المنشورة وحالة المزامنة على `main`

- **الفرع**: `main`
- **حالة Git والمزامنة**:
  - `HEAD` المحلي و`origin/main` متطابقان عند `785209a` (`785209a2c8e3e3f62a01dca979617bc88ac203b4`) بعد الـ manual push، دون أي التزامات متقدمة أو متأخرة.
  - ملف التقرير وحده (`docs/reports/SAFIDATA-FINAL-REMEDIATION-AND-ACCEPTANCE.md`) معدّل محليًا في النسخة الجاري إعدادها.
  - لا توجد تعديلات برمجية محلية.
- **تسلسل الالتزامات المنشورة على `main` وأدوارها**:
  1. `2255a5b` — `style: apply repository formatters`
  2. `008fe42` — `Update`
  3. `fdc13b6` — `Update`
  4. `9620738` — `Update` (إصلاحات الدمج وقارئ MS-XLSB الأولي و`picomatch@4.0.7`)
  5. `0409edc` — `Update` (حد النطاق الكثيف لـ XLSB، رفض تيارات BIFF12 المبتورة، حفظ الصيغ غير المفكوكة، وحذف `tools/vendor/picomatch/`)
  6. `94551e4` (`94551e4579aaf1bfd335e4ce486749602885a711`) — `Update`: **الالتزام الذي نشر إصلاحات P0 وP1 وP2 البرمجية والاختبارات المقابلة** (12 ملفًا: 5 ملفات برمجية/اختبارات + 1 ملف تقرير + 6 مخرجات `artifacts`).
  7. `785209a` (`785209a2c8e3e3f62a01dca979617bc88ac203b4`) — `Update`: **الالتزام الذي نشر تحديث التقرير السابق** (عدّل `docs/reports/SAFIDATA-FINAL-REMEDIATION-AND-ACCEPTANCE.md` فقط بمقدار `+100 / -72`، دون أي تغيير في الكود).
- **نطاق هذا التنقيح التوثيقي**: تصحيح بيانات التقرير فقط لتعكس نشر `785209a` واكتمال نتائج GitHub Actions لكلٍ من `94551e4` و`785209a` دون أي تغيير برمجي. وأي التزام توثيقي لاحق ينشر هذه النسخة من التقرير يخضع لتشغيل GitHub Actions الخاص به عند النشر، ولا يُدّعى هنا نجاح CI لأي التزام لم يكتمل تشغيله.

---

## 2. ما الذي أُنجز ونُشر في الالتزام البرمجي `94551e4579aaf1bfd335e4ce486749602885a711`

جميع إصلاحات P0 وP1 وP2 منشورة بالكامل على `main` ضمن `94551e4579aaf1bfd335e4ce486749602885a711` ولم يطرأ عليها أي تعديل برمجي في `785209a` أو بعده:

- **P0 — تقييد عدد سجلات الخلايا ومنع التكرار على نفس الإحداثي (منشور في `94551e4`)**:
  - **الملف البرمجي**: `crates/sf-io/src/xlsb_reader.rs` — فرض حدود صريحة مستقلة (`MAX_VALUE_CELL_RECORDS = 2_000_000`، `MAX_FORMULA_CELL_RECORDS = 2_000_000`، `MAX_SHEET_CELL_RECORDS = 2_000_000`، `MAX_SHEET_ROW_HEADERS = 1_048_576`، `MAX_SHARED_STRINGS = 2_000_000`، `MAX_MERGE_RECORDS = 100_000`) عبر `ExtentTracker::with_max_records` (بـ `checked_add(1)` ورفض `SF-IMP-005` قبل الإدراج وفي `validate()`)، إضافةً إلى `seen_cell_coords: HashSet<(u32, u32)>` لرفض أي سجل خلية مكرر (`0x0001..=0x000B`) عند نفس `(row, col)` بـ `SF-IMP-002` (`DuplicateXlsbCellCoordinate`) قبل نمو `value_cells`/`formula_cells` أو استدعاء `Range::from_sparse()`.
  - **الاختبارات المقابلة**:
    - اختبارات الوحدة في `crates/sf-io/src/xlsb_reader.rs`: `extent_tracker_enforces_explicit_record_count_limit_even_for_tiny_bounding_box` و`parse_xlsb_package_enforces_independent_value_and_formula_record_caps`.
    - اختبار التكامل العدائي في `crates/sf-io/tests/real_workbooks_regression.rs`: `rejects_xlsb_repeated_cell_records_at_identical_coordinates_before_vector_growth` (`10,000` سجل قيمة مكرر عند `A1` و`10,000` سجل صيغة مكرر عند `A1` داخل أرشيف ZIP غير مضغوط `Stored`).
- **P1 — فرض حدود وترتيب تيارات BIFF12 بصرامة (منشور في `94551e4`)**:
  - **الملف البرمجي**: `crates/sf-io/src/xlsb_reader.rs` — آلة الحالة الإلزامية `StreamBoundaryGuard` (و`OptionalSectionGuard` لأقسام `BrtBeginSheetData`/`BrtEndSheetData` و`BrtBeginCellXFs`/`BrtEndCellXFs` وحارس تداخل التعليقات `0x027B`/`0x027C`) عبر تيارات المصنف (`0x0083`/`0x0084`) وأوراق العمل (`0x0081`/`0x0082`) والأنماط (`0x0116`/`0x0117`) والنصوص المشتركة (`0x009F`/`0x00A0`) والتعليقات (`0x0274`/`0x0275`)، لرفض غياب العلامتين معًا، والنهاية قبل البداية، وتكرار البداية، وتكرار النهاية، وأي سجل قبل البداية أو بعد النهاية بـ `SF-IMP-002` مع الإبقاء على التمييز الصارم بين نهاية الملف النظيفة (`Ok(None)`) والبيانات المبتورة.
  - **الاختبارات المقابلة**:
    - اختبار الوحدة في `crates/sf-io/src/xlsb_reader.rs`: `stream_boundary_guard_enforces_strict_biff12_framing_across_all_streams` — يغطي حالات حدود التيارات المختلفة (غياب العلامتين معًا، سجل قبل البداية، النهاية قبل البداية، تكرار البداية، تكرار النهاية، سجل بعد النهاية، غياب النهاية، والإطار الصالح) عبر أنواع التيارات الخمسة (`Workbook` و`Worksheet` و`Styles` و`SharedStrings` و`Comments`).
    - اختبار التكامل في `crates/sf-io/tests/real_workbooks_regression.rs`: `rejects_xlsb_stream_boundary_violations_across_workbook_worksheet_and_supporting_parts` — يغطي حالات تمثيلية على مستوى حزمة XLSB الكاملة (5 حالات للمصنف و5 حالات لورقة العمل + حالة تمثيلية لكلٍ من `styles.bin` و`sharedStrings.bin` و`comments1.bin`)، وليس كل الحالات الست داخل كل جزء على حدة.
- **P2 — تطابق تطبيع Unicode في الرسم الجغرافي (منشور في `94551e4`)**:
  - **الملف البرمجي**: `apps/web/src/utils/quad-studio-engine.ts` — إضافة `toAsciiLowerCase()` (`A-Z` -> `a-z` فقط) واستخدامها داخل `computeRegionalCoordinatePct` لتطابق `label.trim().to_ascii_lowercase()` في `geo_region_coords` (`crates/sf-recipe/src/datagraph.rs`)، مع توثيق القاعدة المشتركة (تطبيع ASCII للمناطق المعروفة + تجزئة بايتات UTF-8 الأصلية غير المعدّلة للفئات غير المعروفة).
  - **الاختبارات المقابلة**:
    - اختبار Rust في `crates/sf-recipe/src/datagraph/tests.rs`: `geo_region_coords_handles_non_ascii_uppercase_unknown_labels_deterministically`.
    - اختبار TypeScript في `apps/web/src/utils/quad-studio-engine.test.ts`: `matches Rust datagraph regional coordinate projection across Geographic SVG exports` (يثبت التطابق التام لتسميات `"ÜBER_NORD_ÖST"` -> `(73, 21)` و`"DA\u212AAHL"` -> `(49, 27)` و`"İSTANBUL_SÜD"` -> `(52, 62)`).

---

## 3. جرد الملفات الفعلي في الالتزامين المنشورين (من `git show --numstat`)

### أ. الالتزام البرمجي `94551e4579aaf1bfd335e4ce486749602885a711` (12 ملفًا: `+1,041` / `-162` + 3 صور PNG ثنائية)

- **الملفات البرمجية والاختبارات (5 ملفات: `+958` / `-78`)**:
  | الملف | الإضافات | الحذف | الدور |
  | :--- | :---: | :---: | :--- |
  | `crates/sf-io/src/xlsb_reader.rs` | `+553` | `-66` | حدود سجلات الخلايا ومنع تكرار الإحداثيات + `StreamBoundaryGuard` و`OptionalSectionGuard` + 3 اختبارات وحدة |
  | `crates/sf-io/tests/real_workbooks_regression.rs` | `+333` | `-0` | اختبارات انحدار التكرار العدائي (`10,000` سجل `Stored`) وحالات تمثيلية لحدود التيارات على مستوى حزمة XLSB الكاملة والأجزاء المساندة |
  | `crates/sf-recipe/src/datagraph/tests.rs` | `+48` | `-0` | اختبار تطابق `geo_region_coords` لتسميات Unicode غير اللاتينية ذات الأحرف الكبيرة |
  | `apps/web/src/utils/quad-studio-engine.ts` | `+16` | `-3` | دالة `toAsciiLowerCase` وتوثيق قاعدة التطبيع المشتركة مع Rust |
  | `apps/web/src/utils/quad-studio-engine.test.ts` | `+8` | `-9` | اختبار تطابق `computeRegionalCoordinatePct` و`renderChartSvg("Geographic", ...)` مع Rust |
- **التوثيق والتقارير (ملف واحد: `+71` / `-72`)**:
  - `docs/reports/SAFIDATA-FINAL-REMEDIATION-AND-ACCEPTANCE.md` (`+71` / `-72`)
- **مخرجات التحقق والـ `artifacts` (6 ملفات: `+12` / `-12` نصيًا + 3 صور PNG ثنائية)**:
  - `artifacts/intro-visual/visual-check.json` (`+10` / `-10`)
  - `artifacts/intro-visual/studio-e2e/bundles/studio_q4.safi.json` (`+1` / `-1`)
  - `artifacts/intro-visual/studio-e2e/exported-e2e.safi.html` (`+1` / `-1`)
  - `artifacts/intro-visual/intro-auth-gate-handoff.png` (ثنائي: `256,525 -> 256,751` بايت)
  - `artifacts/intro-visual/intro-opening-motion.png` (ثنائي: `259,338 -> 259,160` بايت)
  - `artifacts/intro-visual/studio-desktop-e2e.png` (ثنائي: `447,163 -> 445,922` بايت)

### ب. التزام تحديث التقرير `785209a2c8e3e3f62a01dca979617bc88ac203b4` (ملف واحد: `+100` / `-72`)
- `docs/reports/SAFIDATA-FINAL-REMEDIATION-AND-ACCEPTANCE.md` (`+100` / `-72`) — صفر ملفات برمجية أو `artifacts`.

---

## 4. نتائج GitHub Actions CI الفعلية لكل التزام منشور

### أ. الالتزام `785209a2c8e3e3f62a01dca979617bc88ac203b4` (تحديث التقرير — `HEAD` المنشور حاليًا)
- **SafiData CI Gate (v5.0 Gold Master)** (Run `#44516652367`): **`5/5` jobs `success`**.
- **One-time repository formatting repair** (Run `#44516652468`): **`1/1` job `success`**.
- **الإجمالي عبر التشغيلين**: **`6/6` jobs `success`** (دون أي وظيفة فاشلة أو متخطاة).

| سير العمل / المهمة | الحالة | النتيجة | التوقيت (UTC) | الرابط المباشر |
| :--- | :---: | :---: | :---: | :--- |
| **SafiData CI Gate (v5.0 Gold Master)** (Run `#44516652367` — `5/5` jobs) | `completed` | `success` | `11:38:37Z -> 11:52:40Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652367 |
| ↳ `Full Gate (ubuntu-latest)` | `completed` | `success` | `11:38:39Z -> 11:41:50Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652367/job/135534273586 |
| ↳ `Full Gate (windows-latest)` | `completed` | `success` | `11:38:40Z -> 11:43:04Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652367/job/135534274244 |
| ↳ `PostgreSQL 18.6 migration apply / rollback` (شامل اختبارات PostgreSQL 18.6 الـ 3) | `completed` | `success` | `11:38:39Z -> 11:40:20Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652367/job/135534273732 |
| ↳ `Intro visual regression (Chromium)` | `completed` | `success` | `11:38:39Z -> 11:44:05Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652367/job/135534273276 |
| ↳ `Desktop shell compile (windows-latest)` | `completed` | `success` | `11:38:39Z -> 11:52:39Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652367/job/135534273657 |
| **One-time repository formatting repair** (Run `#44516652468` — `1/1` job) | `completed` | `success` | `11:38:37Z -> 11:38:58Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652468 |
| ↳ `format` | `completed` | `success` | `11:38:39Z -> 11:38:58Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652468/job/135534273834 |

### ب. الالتزام `94551e4579aaf1bfd335e4ce486749602885a711` (إصلاحات P0/P1/P2 البرمجية)
- **SafiData CI Gate (v5.0 Gold Master)** (Run `#44513205873`): **`5/5` jobs `success`**.
- **One-time repository formatting repair** (Run `#44513205853`): **`1/1` job `success`**.
- **الإجمالي عبر التشغيلين**: **`6/6` jobs `success`** (دون أي وظيفة فاشلة أو متخطاة).

| سير العمل / المهمة | الحالة | النتيجة | التوقيت (UTC) | الرابط المباشر |
| :--- | :---: | :---: | :---: | :--- |
| **SafiData CI Gate (v5.0 Gold Master)** (Run `#44513205873` — `5/5` jobs) | `completed` | `success` | `11:15:40Z -> 11:29:32Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205873 |
| ↳ `Full Gate (ubuntu-latest)` | `completed` | `success` | `11:15:42Z -> 11:19:43Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205873/job/135522421310 |
| ↳ `Full Gate (windows-latest)` | `completed` | `success` | `11:15:42Z -> 11:22:03Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205873/job/135522421232 |
| ↳ `PostgreSQL 18.6 migration apply / rollback` (شامل اختبارات PostgreSQL 18.6 الـ 3) | `completed` | `success` | `11:15:42Z -> 11:17:26Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205873/job/135522421030 |
| ↳ `Intro visual regression (Chromium)` | `completed` | `success` | `11:15:43Z -> 11:27:12Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205873/job/135522421102 |
| ↳ `Desktop shell compile (windows-latest)` | `completed` | `success` | `11:15:42Z -> 11:29:32Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205873/job/135522420763 |
| **One-time repository formatting repair** (Run `#44513205853` — `1/1` job) | `completed` | `success` | `11:15:40Z -> 11:16:06Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205853 |
| ↳ `format` | `completed` | `success` | `11:15:42Z -> 11:16:06Z` | https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205853/job/135522420794 |

---

## 5. نتائج التحقق المحلي (مفصولة عن GitHub Actions، على نفس الكود المنشور في `94551e4` / `785209a`)

| الفحص / الأمر المحلي | رمز الخروج | النتيجة والأرقام المرصودة محليًا |
| :--- | :---: | :--- |
| `cargo fmt --all -- --check` + `src-tauri` | `0` | مطابق بالكامل |
| `cargo clippy --workspace --all-targets -- -D warnings` + `src-tauri` | `0` | `0` تحذيرات و`0` أخطاء |
| `cargo test --workspace` | `0` | **`300` ناجح، `0` فشل، `3` متجاهلة محليًا** (`#[ignore]` في `sf-server` لـ PostgreSQL 18.6 — نُفّذت ونجحت في مهمة CI في القسم 4 أعلاه) — تشمل `45` اختبار وحدة و`12` اختبار انحدار في `sf-io` + اختبار `geo_region_coords` في `sf-recipe` |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | `0` | **`2` ناجح، `0` فشل، `1` متجاهل** (`benchmark_native_session_verifier_performance` قياس أداء TLS يدوي) |
| `pnpm install --frozen-lockfile --ignore-scripts` | `0` | `Lockfile is up to date` (`all 9 workspace projects`) |
| `pnpm run format:check` / `typecheck` / `lint` | `0` | نجاح كامل عبر 8 مشاريع في `pnpm -r` |
| `pnpm run test` (Vitest) | `0` | **`66` ملف اختبار / `232` اختبارًا ناجحًا، `0` فشل، `0` تخطي** |
| `pnpm run check:contrast` و`pnpm audit --audit-level=moderate` | `0` | `46` زوج نص (`>= 4.58:1`) و`36` زوج واجهة/رسوم (`>= 4.37:1`) + `--self-test`؛ `No known vulnerabilities found` |
| `verify-i18n.py` / `verify-security.py` / `gen-errors.py --check` / `verify-migrations.py` | `0` | `28` مفتاح i18n؛ `110` مهام موثقة؛ `errors.toml` مطابق؛ `6` أزواج هجرة DB |
| `tools/check-no-network.sh` (+ `--self-test` عبر Git Bash) | `0` | `0` اعتماديات شبكة في `10` حزم محرك |
| `node tools/verify-desktop-build-config.mjs` ثم `pnpm run build` | `0` | رفض الأصول غير الصالحة ومطابقة `https://ci-verify.safidata.dev` بين الحزمة و`safi-desktop.exe`، ثم بناء الإنتاج القياسي |
| `node tools/verify-capsule-chromium.mjs` | `0` | `.safi.html` عبر `file://`: `scripts: 0`, `imagesLoaded: 1`, `externalRequests: 0`, `consoleErrors: 0`, `pageErrors: 0` |
| `node tools/verify-exports-roundtrip.mjs` | `0` | تحقق Python المستقل (`xlsx_sheets`: 4، `png_colors`: 8، `pdf_pages`: 7 عند `xref=660742`) + `21` SVG + مشغل `.safi.html` (`slideNodes: 10`, `zeroSizedSvgs: 0`) |
| `node tools/verify-studio-desktop-e2e.mjs` | `0` | كامل الخطوات ضد `sf-server` الحقيقي و`studio_ipc_harness` (`messy_enterprise.xlsx` و`enterprise_binary.xlsb` والتصفية المتقاطعة والتصدير الذري)؛ ملاحظة: تجاوزت المحاولة الأولى مهلة `page.goto` (30 ثانية) تحت ضغط بناء `sf-server` ثم نجحت بالكامل في التشغيل المنفصل |
| `node tools/verify-intro-visual.mjs` / `verify-ux-e2e.mjs` / `verify-device-identity.mjs` | `0` | دخول `386.8ms`، خروج `798.6ms`، `reducedMotionPassed: true`، مصفوفة المقاسات الـ 7 (`tabAlignedDelta: 0`)، `authMountCount: 1`، معرّف IndexedDB ثابت، و`0` عناصر سطح مكتب في الموقع العام |

---

## 6. كيف تجرّبه بنفسك

1. التحقق من الالتزامات ومطابقة `HEAD` مع `origin/main`:
   - `git rev-parse HEAD` و`git ls-remote origin refs/heads/main`
   - `git show --numstat 94551e4579aaf1bfd335e4ce486749602885a711`
   - `git show --numstat 785209a2c8e3e3f62a01dca979617bc88ac203b4`
2. مراجعة تشغيلات GitHub Actions المكتملة:
   - التزام `785209a`: https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652367 وhttps://github.com/mo7amedrabei14-cell/Safi/actions/runs/44516652468
   - التزام `94551e4`: https://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205873 وhttps://github.com/mo7amedrabei14-cell/Safi/actions/runs/44513205853
3. فحص الحوكمة وسلامة الفروقات محليًا:
   - `py -3 -B tools/verify-security.py` و`git diff --check` و`git status --short`

---

## 7. حدود التحقق المتبقية (بصراحة تامة)

- **اختبار الأداء اليدوي المتجاهل (`#[ignore]`)**: `benchmark_native_session_verifier_performance` في `apps/desktop/src-tauri` غير مشمول في البوابة الآلية لأنه قياس أداء TLS يدوي.
- **أتمتة نافذة WebView2 الأصلية**: لم تُشغَّل أتمتة WebDriver خارجية لنافذة WebView2 الأصلية؛ جرى التحقق من واجهة سطح المكتب E2E عبر Chromium ضد `sf-server` الحقيقي و`studio_ipc_harness` الحقيقي، إلى جانب ترجمة وبناء `safi-desktop.exe` (Release) وفحوصاته في CI (`Desktop shell compile (windows-latest)`) ومحليًا (`verify-desktop-build-config.mjs`).
- **نطاق اختبارات CI التفاعلية**: السكربتان `verify-studio-desktop-e2e.mjs` و`verify-exports-roundtrip.mjs` جرى تشغيلهما ونجاحهما محليًا (`EXIT=0`)، بينما شغّل GitHub Actions بوابات `make check` على Linux وWindows واختبارات PostgreSQL 18.6 الـ 3 و`Intro visual regression (Chromium)` وبناء `Desktop shell compile (windows-latest)`.
- **تنويه خاص بأي نشر لاحق لهذا الملف**: نتائج CI المذكورة في القسم 4 تخص الالتزامين المنشورين `94551e4` و`785209a` تحديدًا؛ وأي التزام جديد ينشر هذا التنقيح التوثيقي لاحقًا يُعتمد عليه تشغيل CI الخاص به بعد النشر.

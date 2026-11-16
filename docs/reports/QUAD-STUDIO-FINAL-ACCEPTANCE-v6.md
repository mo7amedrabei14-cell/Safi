# تقرير القبول والتحصين النهائي للاستوديو الرباعي (`QUAD-STUDIO-FINAL-ACCEPTANCE-v6` — مصحّح مرجعياً في `v7`)

**التاريخ:** 2026-11-15 (تصحيح الحقائق المرجعية في 2026-11-16 ضمن `v7`)
**الفرع:** `main` (عمل محلي مباشر دون فروع أو worktrees ودون تنفيذ `git push` تلقائي)
**نقطة الأساس المراجَعة:** `361d3a20f8d80d34c536ecd273bfcb16b7c14dd5` (`361d3a20`)

---

> **تنبيه تصحيحي (`v7`):** صُحّحت في هذا الملف بيانات دقيقة شابها عدم تطابق في مسودة `v6` الأصلية: عدد الأوامر المسجلة في `ALLOWED_COMMANDS` هو **47** أمراً (منها **43** أمراً حساساً محمياً بـ `gate_command`، و**2** أمرا ربط/مسح الجلسة، و**2** أمرا shell غير حساسين)، والحد الفعلي `MAX_IPC_BYTES = 1_048_576` بايت (`1 MiB`) برمزي `SF-SYS-010` / `SF-SYS-007`، ومفاتيح كشف الشذوذ هي الـ 5 الحقيقية من `studio_anomaly.rs` (`empty_cell`, `structural_row_key`, `type_consistency`, `calendar_validation`, `iqr_1_5`). أما معالجة ملاحظات مراجعة `v6` (المصادقة Native الحقيقية عبر TLS، والرسوم الثمانية هندسياً، وحصر AI في المحلي، واسم اختبار الـ Harness الصادق) فموثّقة بالكامل في تقرير `QUAD-STUDIO-FINAL-ACCEPTANCE-v7.md`.

---

## 1. ملخص تنفيذي باللغة العربية البسيطة

- **ما الذي أُنجز (3 أسطر)**:
  1. أغلقنا ملاحظات مراجعة الكود الأولي على `main` (`361d3a20`): أضفنا `gate_command` داخل `StudioStore` لحماية جميع الأوامر الـ 43 الحساسة وحدود `MAX_IPC_BYTES = 1_048_576` (`1 MiB`) وسياسة الجهاز الواحد (مع لاحقة `v7` التي استبدلت آلية الـ loopback بمسار التحقق الإنتاجي عبر TLS + `SAFI_API_BASE_URL` المدمج وقت البناء).
  2. وسّعنا الأنواع الـ 8 للرسوم البيانية في Rust (`ComputeChartSeries`: `bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`)، وأضفنا تحكم الارتفاع `rowSpan` (`+H` / `-H` و`Shift+ArrowUp/Down`) في `FreeFormBuilderView`، وأصلحنا ترجمة جميع الحالات الـ 5 في `AnomalyPanel`، وطابقنا إعلان خصوصية AI (والذي حُصر في `v7` بالخادم المحلي `http://127.0.0.1:11434` مع تأجيل السحابي لمرحلة مستقلة).
  3. بنينا وشغّلنا اختبار التكامل `Chromium UI + Real Rust StudioStore Subprocess Harness` (`tools/verify-studio-desktop-e2e.mjs` + `studio_ipc_harness.rs`) واختبار الكبسولة المستقلة في Chromium (`tools/verify-capsule-chromium.mjs`) وربطناهما في `.github/workflows/ci.yml`.
- **كيف تجرّبه بنفسك (خطوات مرقّمة بسيطة جداً)**:
  1. نفّذ `cargo test -p sf-recipe -p sf-desktop` للتحقق من اختبارات بوابة المصادقة والرسوم الثمانية والكبسولة والرحلات الـ 18.
  2. نفّذ `pnpm --filter @safi/web test` للتحقق من اختبارات واجهة الاستوديو المحدثة في `desktop-studio.test.tsx`.
  3. نفّذ `node tools/verify-capsule-chromium.mjs` ثم `node tools/verify-studio-desktop-e2e.mjs` لتشغيل اختبارات Chromium.
- **ما الذي فشل أو لم يكتمل (بصراحة)**:
  - بقيت مراجعة `v6` بحاجة لاستكمال 6 نقاط رُصدت لاحقاً: الاعتماد في `v6` على `SAFI_DESKTOP_AUTH_LOOPBACK_ADDR` في التشغيل الطبيعي، ومحاكاة `/v1/auth` داخل `verify-studio-desktop-e2e.mjs`، وتمثيل SVG غير المكتمل في `Pie`/`Donut`/`Histogram`، والسماح لـ `https:` في `validateAiEndpointUrl` رغم غيابها عن CSP، وعدّ بعض المعاملات غير النصية في IPC — وقد جرى معالجة ذلك كله في **`v7`**.
- **قرارات أحتاجها منك**:
  - راجع تقرير `QUAD-STUDIO-FINAL-ACCEPTANCE-v7.md` المحدّث للاعتماد النهائي قبل أي Commit أو Push.

---

## 2. حالة كل ملاحظة من الملاحظات الـ 6 بالتفصيل

### الملاحظة 1 — حماية أوامر Tauri بالجلسة وحدود `MAX_IPC_BYTES` (صُحّح وصفها في `v7`)
- **المشكلة السابقة**: كانت `ApplicationShell.tsx` تمنع عرض الواجهة قبل تسجيل الدخول، لكن أوامر Studio في `apps/desktop/src-tauri/src/main.rs` لم تكن تتحقق من الجلسة النشطة، ولم تكن دوال `authorize_command` و`validate_ipc_payload_len` مستدعاة في مسار التنفيذ.
- **الحل المنفّذ (والاستكمال في `v7`)**:
  - أنشأنا سمة `StudioSessionVerifier` ومطبّقها `StudioSessionAuthority` في `crates/sf-desktop/src/studio.rs`، وفئة `DesktopShellSessionVerifier` في `apps/desktop/src-tauri/src/main.rs` (في `v6` اعتمدت على `SAFI_DESKTOP_AUTH_LOOPBACK_ADDR` أو `StudioSessionAuthority`، ثم استُبدلت في `v7` بمسار TLS المباشر إلى `SAFI_API_BASE_URL` المدمج وقت البناء على `GET /v1/auth/sessions/current` بدون أي متغير بيئة يدوي).
  - أضفنا الدالة المركزية `StudioStore::gate_command(command, payload_len, caller_now_ms)` التي تستدعي `enforce_ipc_command` (`authorize_command` رمز `SF-SYS-010` + `validate_ipc_payload_len` بحد `MAX_IPC_BYTES = 1_048_576` بايت رمز `SF-SYS-007`) ثم تتحقق من صلاحية الجلسة ومطابقة `installation_id` للجهاز الوحيد.
  - أضفنا أمري إدارة الجلسة `studio_set_auth_session` و`studio_clear_auth_session` في Tauri وربطناهما في `ApplicationShell.tsx` (ومُنع التسرع في فتح الواجهة قبل اكتمال `await setStudioAuthSession(...)` في `v7`).

### الملاحظة 2 — اختبارات التكامل لـ UI + Harness والكبسولة (صُحّح وصفها في `v7`)
- **المشكلة السابقة**: كانت اختبارات `desktop-studio.test.tsx` تعتمد على محاكاة `window.__TAURI_INTERNALS__`، وكان فحص الكبسولة في Chromium يدوياً خارج المستودع.
- **الحل المنفّذ (والاستكمال في `v7`)**:
  - أنشأنا `crates/sf-desktop/examples/studio_ipc_harness.rs`: مشغّل JSON-RPC عبر `stdio` يشغّل `StudioStore` و`DataGraph` (`rust_decimal`) و`StudioSessionAuthority` الحقيقية في Rust.
  - أنشأنا `tools/verify-studio-desktop-e2e.mjs` (المسمّى بصدق **Chromium UI + Real Rust StudioStore Subprocess Harness Integration Test**): في `v6` كان يمرر طلبات `/v1/auth/*` عبر `page.route`، ثم استُبدل في `v7` بخادم `sf-server` HTTP حقيقي (`auth_harness_server`) دون أي Mock للمصادقة.
  - أنشأنا `tools/verify-capsule-chromium.mjs`: يولّد كبسولة `.safi.html` ويفتحها في Chromium عبر `file://` دون خادم ومع تعطيل الشبكة.
  - ربطنا السكربتين داخل `.github/workflows/ci.yml` في مهمة `intro-visual`.

### الملاحظة 3 — إكمال أنواع الرسوم البيانية الـ 8 في اللوحة والكبسولة (مكتملة ومختبرة)
- **المشكلة السابقة**: كانت اللوحة تدعم 3 أنواع فقط (`bar`, `line`, `kpi`) وترسم `line` كأشرطة أفقية.
- **الحل المنفّذ**:
  - أضفنا `ChartType` (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`) ودالة `DataGraph::compute_chart_series` في `crates/sf-recipe/src/datagraph.rs` لحساب الإحداثيات والنسب والتوزيعات التكرارية في Rust حصراً باستخدام `rust_decimal` مع رفض التركيبات غير الصالحة (مثل قيم سالبة في `pie`/`donut` أو أعمدة غير رقمية في `scatter`/`histogram`) برمز `SF-CAL-004`.
  - أضفنا أمر `studio_chart_data` في `crates/sf-desktop/src/studio.rs` و`apps/desktop/src-tauri/src/main.rs` و`studio-bridge.ts`.
  - حدّثنا `DashboardChartsView.tsx` و`crates/sf-recipe/src/studio_capsule.rs` لرسم التمثيل البصري الحقيقي لكل نوع من الأنواع الثمانية عبر SVG أصلي مع التصفية المتقاطعة للجدول وحفظ `chartType` في حزم المشاريع والكبسولة.

### الملاحظة 4 — إكمال تحكم `rowSpan` في اللوح الحر `Free-Form Builder` (مكتملة ومختبرة)
- **المشكلة السابقة**: كان `rowSpan` مخزناً في النموذج لكن دون أزرار أو اختصارات لتغيير الارتفاع في الواجهة.
- **الحل المنفّذ**:
  - أضفنا دالة `resizeRowSpan` في `FreeFormBuilderView.tsx` مع أزرار `+H` (`Increase height`) و`-H` (`Decrease height`) واختصارات لوحة المفاتيح `Shift+ArrowDown` و`Shift+ArrowUp` المقيدة في المدى `1..8`.
  - أضفنا عرض الأبعاد `W:{colSpan}×H:{rowSpan}` على كل بطاقة، وخصائص `data-grid-col-span` و`data-grid-row-span` في كل من الواجهة والكبسولة المصدّرة `.safi.html`.

### الملاحظة 5 — إصلاح ترجمة طرق كشف الشذوذ في `AnomalyPanel` (مكتملة ومختبرة)
- **المشكلة السابقة**: كانت `AnomalyPanel.tsx` تقارن `item.methodKey` بقيم غير مطابقة لما ينتجه `crates/sf-recipe/src/studio_anomaly.rs` فتظهر المفاتيح الخام للمستخدم.
- **الحل المنفّذ**:
  - أنشأنا جدول `STUDIO_ANOMALY_METHODS` في `AnomalyPanel.tsx` لترجمة المفاتيح الخمسة الحقيقية (`studio.method.empty_cell`, `studio.method.structural_row_key`, `studio.method.type_consistency`, `studio.method.calendar_validation`, `studio.method.iqr_1_5`) بالعربية والإنجليزية بدقة مع fallback آمن.

### الملاحظة 6 — الصدق في تكامل الذكاء الاصطناعي والخصوصية وCSP (حُسمت في `v7`)
- **المشكلة السابقة**: كان نص الموافقة يذكر إرسال أسماء الأعمدة ونوعها فقط بينما الكود يرسل أيضاً نص الطلب ومعرّفات المؤشرات وأسماء المتغيرات، ولم يكن متوافقاً مع `connect-src` الصارم في `tauri.conf.json`.
- **الحل المنفّذ**:
  - حدّثنا `ai-interpreter.ts` و`CommandBarPanel.tsx` لعرض معاينة JSON الحية (`data-testid="ai-outbound-payload-preview"`) وحقل مفتاح API اختياري في الذاكرة فقط، مع مطابقة `connect-src` في `tauri.conf.json` (`'self' ipc: http://ipc.localhost http://127.0.0.1:11434`). وفي **`v7`** حُصر `validateAiEndpointUrl` في `http://127.0.0.1:11434` / `http://localhost:11434` فقط مع تأجيل AI السحابي لمرحلة لاحقة.

---

## 3. قائمة جميع الملفات التي تغيرت

1. `.github/workflows/ci.yml`
2. `apps/desktop/src-tauri/Cargo.toml`
3. `apps/desktop/src-tauri/src/main.rs`
4. `apps/desktop/src-tauri/tauri.conf.json`
5. `apps/web/src/ApplicationShell.tsx`
6. `apps/web/src/components/layout/CommandPalette.tsx`
7. `apps/web/src/components/layout/FloatingGlassDock.tsx`
8. `apps/web/src/desktop/DesktopStudio.tsx`
9. `apps/web/src/desktop/ai-interpreter.ts`
10. `apps/web/src/desktop/components/AnomalyPanel.tsx`
11. `apps/web/src/desktop/components/CommandBarPanel.tsx`
12. `apps/web/src/desktop/components/DashboardChartsView.tsx`
13. `apps/web/src/desktop/components/FreeFormBuilderView.tsx`
14. `apps/web/src/desktop/desktop-studio.test.tsx`
15. `apps/web/src/desktop/studio-bridge.ts`
16. `crates/sf-desktop/examples/studio_ipc_harness.rs` (جديد)
17. `crates/sf-desktop/src/api_origin.rs`
18. `crates/sf-desktop/src/commands.rs`
19. `crates/sf-desktop/src/studio.rs`
20. `crates/sf-desktop/tests/shell_config.rs`
21. `crates/sf-desktop/tests/studio_phase5_e2e.rs`
22. `crates/sf-recipe/src/datagraph.rs`
23. `crates/sf-recipe/src/datagraph/tests.rs`
24. `crates/sf-recipe/src/lib.rs`
25. `crates/sf-recipe/src/studio_capsule.rs`
26. `docs/plans/STUDIO-Phase-1.md` .. `docs/plans/STUDIO-Phase-5.md`
27. `docs/reports/STUDIO-COMPLETION-RE-AUDIT.md`
28. `docs/reports/STUDIO-Phase-1.md` .. `docs/reports/STUDIO-Phase-5.md`
29. `docs/reports/QUAD-STUDIO-FINAL-REPORT.md`
30. `docs/reports/QUAD-STUDIO-FINAL-ACCEPTANCE-v6.md` (جديد)
31. `tools/verify-capsule-chromium.mjs` (جديد)
32. `tools/verify-studio-desktop-e2e.mjs` (جديد)

---

## 4. شرح بوابة المصادقة Native في Tauri وكيف تمنع التجاوز

1. **نقطة تفتيش إلزامية داخل `StudioStore` (`gate_command`)**:
   - كل دالة أمر حساس داخل `crates/sf-desktop/src/studio.rs` تبدأ باستدعاء `self.gate_command(command_name, payload_len, caller_now_ms)?` قبل لمس أي مشروع أو ملف أو ذاكرة.
2. **فحص القائمة البيضاء وحجم الحمولة (`enforce_ipc_command`)**:
   - تستدعي `gate_command` دالة `enforce_ipc_command` في `crates/sf-desktop/src/commands.rs` التي تتحقق من وجود الأمر في `ALLOWED_COMMANDS` (`SF-SYS-010` عند الرفض) ومن أن حجم مدخلات الأمر لا يتجاوز `MAX_IPC_BYTES = 1_048_576` بايت (`1 MiB`، رمز `SF-SYS-007` عند التجاوز).
3. **التحقق الفعلي من الجلسة والجهاز بنمط Fail-Closed**:
   - تقرأ `gate_command` سياق الجلسة النشط من الذاكرة المحمية (`Mutex<Option<ActiveStudioSession>>`). إذا لم توجد جلسة مسجلة، تُرفض العملية فوراً برمز `SF-AUTH-001`.
   - تمرر `gate_command` التوكن ومعرّف تثبيت الجهاز (`installation_id`) والوقت الفعلي بالمللي ثانية إلى `StudioSessionVerifier::verify_session`. (في `v7` أصبح `DesktopShellSessionVerifier` يستعلم فعلياً عبر TLS عن `GET /v1/auth/sessions/current` على `SAFI_API_BASE_URL` المدمج وقت البناء مع `X-Safi-Installation-Id`).
   - إذا انتهت صلاحية الجلسة يُرجع `SF-AUTH-002`، وعند الإلغاء أو استبدال الجهاز يُرجع `SF-AUTH-003` ويُصفّر سياق الجلسة المحلي فوراً.
4. **عدم تخزين التوكن على القرص أو في السجلات**:
   - يُحفظ سياق الجلسة في ذاكرة العملية (`RAM`) فقط؛ عند تسجيل الخروج (`studio_clear_auth_session`) أو إعادة تشغيل التطبيق، يبدأ `StudioStore` خالياً من أي جلسة ويرفض جميع الأوامر الحساسة حتى تتم إعادة المصادقة.

---

## 5. قائمة جميع أوامر IPC المسجلة (`ALLOWED_COMMANDS` — 47 أمراً)

### أوامر Shell غير الحساسة (2 — مقيدة بـ `ALLOWED_COMMANDS` و`MAX_IPC_BYTES`):
1. `app_info`
2. `take_pending_deep_link`

### أوامر إدارة بوابة الجلسة (2 — مقيدة بـ `ALLOWED_COMMANDS` و`MAX_IPC_BYTES`):
3. `studio_set_auth_session`
4. `studio_clear_auth_session`

### الأوامر الحساسة المحمية بـ `gate_command` (43 أمراً: 3 حوارات ملفات + 40 أمر Studio):
1. `pick_input_file`
2. `pick_output_file`
3. `pick_capsule_output_file`
4. `studio_open_file`
5. `studio_state`
6. `studio_rows_page`
7. `studio_set_cell`
8. `studio_set_cell_by_id`
9. `studio_sort_rows`
10. `studio_filter_rows`
11. `studio_delete_row`
12. `studio_insert_row`
13. `studio_add_source_column`
14. `studio_rename_column`
15. `studio_delete_column`
16. `studio_trim_column`
17. `studio_define_metric`
18. `studio_add_calculated_column`
19. `studio_define_parameter`
20. `studio_update_parameter`
21. `studio_add_scenario`
22. `studio_rename_scenario`
23. `studio_activate_scenario`
24. `studio_reset_scenario`
25. `studio_set_scenario_parameter`
26. `studio_compare_scenarios`
27. `studio_apply_command`
28. `studio_apply_anomaly_fix`
29. `studio_undo`
30. `studio_redo`
31. `studio_grouped`
32. `studio_chart_data`
33. `studio_narrative`
34. `studio_snapshot`
35. `studio_restore`
36. `studio_set_artifacts`
37. `studio_save_project`
38. `studio_list_projects`
39. `studio_load_project`
40. `studio_delete_project`
41. `studio_list_templates`
42. `studio_instantiate_template`
43. `studio_export_capsule`

*(المجموع الدقيق في `crates/sf-desktop/src/commands.rs`: `47` أمراً في `ALLOWED_COMMANDS` = `2` إدارة جلسة + `2` shell غير حساسين + `43` أمراً حساساً محمياً عبر `gate_command`.)*

---

## 6. نتائج اختبارات القبول لكل مهمة

| المهمة | اختبارات القبول المنفذة | النتيجة (في `v6` ← المصحّح في `v7`) |
| --- | --- | --- |
| **المهمة 1: بوابة المصادقة Native في Tauri** | رفض دون جلسة (`SF-AUTH-001`)، رفض بتوكن مزور (`SF-AUTH-001`)، رفض بجلسة منتهية (`SF-AUTH-002`)، رفض عند الإلغاء أو استبدال الجهاز (`SF-AUTH-003` في `v7`)، السماح بجلسة صالحة، الرفض الفوري بعد `clear_auth_session`، ورفض الحمولات الأكبر من `MAX_IPC_BYTES = 1_048_576` (`SF-SYS-007`) عبر جميع الـ 43 أمراً الحساساً | **استُبدل Loopback بـ TLS في `v7`** |
| **المهمة 2: اختبارات التكامل للـ UI + Harness والكبسولة** | تشغيل `tools/verify-studio-desktop-e2e.mjs` (`Chromium UI + Real sf-server Auth + Real Rust StudioStore Subprocess Harness`) و`tools/verify-capsule-chromium.mjs` (`file://`) وربطهما في `.github/workflows/ci.yml` | **أُزيل Mock المصادقة في `v7`** |
| **المهمة 3: 8 أنواع رسوم بيانية** | حساب ورسم `bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi` في Rust والواجهة والكبسولة، مع مسارات SVG حقيقية للقطاعات والأعمدة وبطاقة KPI مستقلة في `v7` | **أُكمل التمثيل الهندسي في `v7`** |
| **المهمة 4: تحكم `rowSpan` في اللوح الحر** | أزرار `+H` / `-H` و`Shift+ArrowDown/Up` (`1..8`)، شارة `W×H`، حفظ واستعادة `rowSpan`، وتصدير `data-grid-row-span` في `.safi.html` | **ناجح** |
| **المهمة 5: إصلاح ترجمة `AnomalyPanel`** | مطابقة مفاتيح `studio.method.empty_cell`, `studio.method.structural_row_key`, `studio.method.type_consistency`, `studio.method.calendar_validation`, `studio.method.iqr_1_5` بالعربية والإنجليزية | **ناجح** |
| **المهمة 6: خصوصية AI وCSP** | مطابقة نص الموافقة مع الحمولة الفعلية، صندوق معاينة JSON الحي، وحصر `validateAiEndpointUrl` في `http://127.0.0.1:11434` / `http://localhost:11434` متطابقاً مع `tauri.conf.json` (تأجيل السحابي لمرحلة مستقلة) | **تم التوحيد في `v7`** |

---

## 7. نتائج اختبارات Rust الفعلية (المحدثة في `v7`)

- `cargo test -p sf-recipe`: **34 اختباراً ناجحاً** (`34` اختبار وحدة، `0` doctests)، صفر فشل.
- `cargo test -p sf-desktop`: **74 اختباراً ناجحاً** (`64` اختبار وحدة في `sf-desktop` + `5` اختبارات في `shell_config` + `2` اختباران شاملان في `studio_phase5_e2e` يغطيان الـ 18 رحلة والـ 43 أمراً الحساساً المحمياً + `3` اختبارات تكامل أخرى)، صفر فشل.
- `cargo test -p sf-server`: **79 اختباراً ناجحاً** محلياً (`64` وحدة + `10` `auth_flow` + `4` `zero_trust_guard` + `1` `log_center`) مع بقاء **3** اختبارات PostgreSQL حية (`account_postgres` 1 + `accounts_postgres` 2) في وضع `ignored` محلياً وتعمل فعلياً داخل `.github/workflows/ci.yml` (`tools/test-migrations.sh` على `postgres:18.6-bookworm`).

---

## 8. نتائج اختبارات Vitest الفعلية

- `pnpm exec vitest run apps/web/src/desktop/desktop-studio.test.tsx`: **18 / 18 اختباراً ناجحاً** (بوابة `SF-SYS-010`، والتدفق الكامل، وجميع الرسوم الـ 8 بهندستها، و`rowSpan`، وترجمة `AnomalyPanel` الـ 5، ومطابقة AI CSP).
- `pnpm test` (مساحة العمل كاملة): **58 ملف اختبار ناجحاً (197 اختباراً)**، صفر فشل.

---

## 9. نتائج اختبارات Chromium الفعلية لـ Harness وCapsule

1. **اختبار الكبسولة (`node tools/verify-capsule-chromium.mjs`)**:
   - المتصفح: Headless Chromium عبر بروتوكول `file://` مباشرة دون خادم HTTP أو اتصال خارجي (`externalRequests: 0`, `scripts: 0`, `consoleErrors: 0`).
2. **اختبار التكامل (`Chromium UI + Real sf-server Auth + Real Rust StudioStore Subprocess Harness Integration Test`)**:
   - `node tools/verify-studio-desktop-e2e.mjs`: يفتح التطبيق في Chromium متصلاً بخادم `sf-server` HTTP حقيقي (`auth_harness_server`) وبعملية `studio_ipc_harness` عبر جسر `window.__TAURI_INTERNALS__` المحقون (وليس عبر نافذة WebView2/WebKitGTK GUI المجمّعة بـ Tauri WebDriver).

---

## 10. مراجعة أدوات التحقق والفحص

- جميع التفاصيل الكاملة لأدوات التحقق المحدثة في **`v7`** (بما في ذلك `apps/desktop/src-tauri` مع اختبارات TLS الـ 11، و`make check` و`clippy` و`fmt`) موثقة بالأرقام الفعلية في **`docs/reports/QUAD-STUDIO-FINAL-ACCEPTANCE-v7.md`**.

---

## 11. أي قيود تقنية متبقية بصدق كامل

1. **نموذج الذكاء الاصطناعي في `CommandBarPanel` (`SF-AIX-001`)**:
   - التطبيق لا يضم أوزان نموذج مدمجة ويحصر الاتصال المحلي الاختياري في `http://127.0.0.1:11434` فقط (Ollama loopback)، مع تأجيل أي مزود سحابي إلى مرحلة مستقلة.
2. **كبسولة `.safi.html` خالية من JavaScript**:
   - تلتزم بـ `script-src 'none'` وبث الأرقام والبصمات وأشكال SVG المحسوبة مسبقاً من Rust.
3. **بيئة اختبار الـ GUI في `verify-studio-desktop-e2e.mjs`**:
   - يختبر Chromium UI + خادم `sf-server` الحقيقي + محرك Rust `StudioStore` الحقيقي عبر جسر محقون بدلاً من Tauri WebDriver لعدم توفر محرك WebView2 GUI تفاعلي في الحاوية/CI.

---

## 12. خطوات التحقق اليدوي والاعتماد

- راجع التقرير الكامل **`docs/reports/QUAD-STUDIO-FINAL-ACCEPTANCE-v7.md`** واتبع خطوات التحقق الواردة فيه قبل الموافقة على `git commit` و`git push`.

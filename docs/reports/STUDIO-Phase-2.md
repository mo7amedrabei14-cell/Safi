# تقرير المالك: STUDIO-Phase-2 (شريط الأوامر، السرد المرجعي، وتصحيح الشذوذ)

**التاريخ:** 2026-11-15  
**الحالة:** منفذة جزئياً بصدق (`44db08e2e4eadba22c4fb388d706dd0e6e117afb`) — البنية التحتية الكاملة منفذة ومختبرة، وتكامل نموذج AI حقيقي محجوب صراحةً برمز `SF-AIX-001`.

## ما الذي أُنجز

- **توسيع DSL الموثق في Rust**: أضفنا أوامر `SetCellById` و`SortRows` و`FilterRows` و`DeleteRow` إلى `crates/sf-recipe/src/studio_dsl.rs` مع رفض أي حقول أو أوامر مجهولة وحد حجم 8,192 بايت، وتحديث السرد `studio_narrative` وإضافة تصحيح الشذوذ الذري القابل للتراجع `apply_anomaly_fix`.
- **واجهة الأوامر والاعتماد والتصحيح**: بنينا `CommandBarPanel` و`AnomalyPanel` (يعرض رقم الصف المستقر `RowId` ويصحح بنقرة واحدة مع تراجع كامل) و`NarrativeTextRenderer` (يدمج `{{metric:id}}` كشرائح موثقة مع وسم المفقود وزر فك الربط).
- **مانع الثقة وتسجيل الأخطاء**: سُجّل `SF-AIX-001` (عدم توفر مزود/نموذج AI مهيأ) و`SF-AIX-002` (رفض حمولة DSL غير موثوقة) في `errors.toml` مع مزامنة الكتالوج الكامل (79 كوداً).

## كيف تجرّبه بنفسك

1. نفّذ `cargo test -p sf-recipe studio_dsl` و`cargo test -p sf-desktop anomaly_detection`.
2. نفّذ `pnpm exec vitest run apps/web/src/desktop/desktop-studio.test.tsx`.
3. في شاشة `CommandBarPanel` اكتب طلباً؛ سيظهر تنبيه `SF-AIX-001` الصريح لعدم تهيئة نموذج محلي، ويمكنك اختبار قوالب DSL المنظّمة ومعاينة الإجراء ثم اعتماد التنفيذ في Rust يدوياً.

## ما الذي فشل أو لم يكتمل

- **عدم وجود نموذج AI محلي أو مزود معتمد (`SF-AIX-001`)**: الجهاز والغلاف الآمن (CSP بلا اتصال خارجي في Rust ولا أوزان WebLLM/نموذج محلي مهيأة ولا مفاتيح API) لا يملكان نموذج AI فعلياً، ولم نزيّف الذكاء الاصطناعي بقواعد ثابتة تنفيذاً صريحاً لقواعد الأمانة الهندسية.

## قرارات أحتاجها منك

1. اختيار مسار AI المعتمد مستقبلاً: تشغيل نموذج محلي (مثل Ollama محلي على `127.0.0.1` أو حزمة أوزان WebLLM محلية) أم مزود سحابي بموافقة صريحة من المستخدم ومفتاح يُدخله بنفسه.

## الملفات التي تغيّرت

- `errors.toml`، `crates/sf-core/src/generated_errors.rs`، `packages/i18n/src/generated-errors.ts`
- `crates/sf-recipe/src/{studio_dsl.rs,studio_anomaly.rs,datagraph.rs}`
- `crates/sf-desktop/src/{commands.rs,studio.rs}`، `apps/desktop/src-tauri/src/main.rs`
- `apps/web/src/desktop/{ai-interpreter.ts,studio-bridge.ts,DesktopStudio.tsx,desktop-studio.css,desktop-studio.test.tsx}`
- `apps/web/src/desktop/components/{CommandBarPanel.tsx,AnomalyPanel.tsx,NarrativeTextRenderer.tsx}`

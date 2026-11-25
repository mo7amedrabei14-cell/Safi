# سجل القرارات المعمارية (Architecture Decision Records — `docs/DECISIONS.md`)

يُدار هذا السجل كمصدر وحيد للحقيقة للقرارات المعمارية عبر **Ponytail** (`.agents/ponytail/config.toml`) وفق القاعدة الصارمة رقم 3 في `AGENTS.md` والقسمين 18 و20 من المواصفة المعتمدة **SafiData v5.0 Gold Master**.

---

## ADR-0001: التوفيق بين قاعدة تأجيل المجلدات في القسم 19.4 وتصنيف الأدوات الـ 23 في القسم 20.1 ومنع تجاوز نطاق `T1.00`
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-02)
- **المهمة**: `T1.00`
- **السياق**:
  - ينص القسم 19.4 صراحةً على: «ملاحظة: المجلدات `server/` و`apps/desktop` و`apps/site` و`apps/admin` و`crates` الباقية تُنشأ في شرائحها (S4 وS5 وS6) لا قبلها».
  - تنص القاعدة الصارمة رقم 1 في `AGENTS.md` على: «Never build features outside the current task».
  - يعرض الجدول 20.1 مسارات التوظيف النهائية للأدوات والمهارات الـ 23، ومنها أدوات تقع داخل المجلدات المؤجلة للشرائح `S4` و`S6` أو داخل مهام لاحقة من الشريحة `S1` و`S3` (`T1.13`, `T1.14`, `T1.20`, `T3.14`).
- **القرار**:
  - الالتزام الحرفي بقاعدة القسم 19.4 بعدم إنشاء المجلدات المؤجلة (`server/`, `apps/desktop`, `apps/site`, `apps/admin`) أو حزم المحرك غير الداخلة في S1 (`sf-calc`, `sf-detect`, `sf-pii`, `sf-ffi`) خلال `T1.00`.
  - الاكتفاء بنقاط دخول تأسيسية نظيفة (`CRATE_ID` و`PACKAGE_ID`) في `crates/sf-*` و`packages/*` و`apps/web` مع إبقاء `packages/brand/brand.config.ts` و`errors.toml` و`recipe.schema.json`.
  - توصيف حالة كل أداة من الأدوات الـ 23 بصدق في `.agents/tools-manifest.json` بين ما هو إعداد مفعّل في `T1.00` (`configured_in_t1_00`) وما هو مؤجّل لمهمته أو شريحته المختصة (`deferred_to_task_or_slice`).
- **الأثر**:
  - تحقيق التوافق التام بين الأقسام 19.4 و19.7 و20.1 دون أي تجاوز لنطاق `T1.00` (`Zero Scope Creep`).

---

## ADR-0002: حظر مكتبات الشبكة كليًا داخل محرك Rust (`crates/sf-*`)
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-02)
- **المهمة**: `T1.00`
- **السياق**:
  - تنص القاعدة الصارمة رقم 7 في `AGENTS.md` والقسم 3 على أن حزم المحرك (`crates/sf-*`) لا تمتلك أي وصول للشبكة، وأن فحص CI يجب أن يفشل فورًا إن ظهرت أي مكتبة شبكة في شجرة اعتماديات المحرك.
- **القرار**:
  - بناء السكربت `tools/check-no-network.sh` الذي يفحص كلاً من استخدامات `std::net` و`core::net` وملفات `Cargo.toml` وشجرة الاعتماديات الفعلية المستخرجة عبر `cargo metadata --format-version 1` لجميع حزم `crates/sf-*` ضد قائمة الحظر الشاملة.
- **الأثر**:
  - ضمان عمل المحرك محليًا 100% واستحالة تسريب أي بيانات عبر المحرك سواء بطريق مباشر أو عبر اعتمادية فرعية (`transitive dependency`).

---

## ADR-0003: تثبيت الإصدارات الصارم وتعطيل سكربتات التثبيت (`postinstall`)
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-02)
- **المهمة**: `T1.00`
- **السياق**:
  - تنص القاعدة الصارمة رقم 6 في `AGENTS.md` على تثبيت كل اعتمادية بإصدار دقيق (`exact version`) مع ملف قفل (`lockfile`) وتعطيل سكربتات `postinstall`.
- **القرار**:
  - ضبط `.npmrc` بالقيم `save-exact=true` و`ignore-scripts=true` و`engine-strict=true`.
  - منع استخدام بادئات التراخي (`^` أو `~`) في جميع ملفات `package.json` و`Cargo.toml`, والتحقق من ذلك آليًا ضمن بوابة الأمان في `make check`.
- **الأثر**:
  - حماية سلسلة التوريد البرمجية (`Supply Chain Security`) وضمان حتمية البناء (`Deterministic Builds`) عبر جميع البيئات.

---

## ADR-0004: ميزانية حجم مخرج WebAssembly لمتصفح الويب (`crates/sf-wasm`)
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-04)
- **المهمة**: `T1.11`

---

## ADR-0006: اعتماد معيار WebCrypto (AES-256-GCM + PBKDF2) للتخزين المحلي المشفّر في OPFS
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-05)
- **المهمة**: `T2.02`
- **السياق**:
  - يتطلب القسم 8.3 في المواصفة وتطبيق الأمان حماية وصفات المستخدم وجلساته محلياً عبر OPFS باستخدام تشفير معتمد وقوي بعبارة سر اختيارية دون أي خوارزميات صورية أو تجريبية.
- **القرار**:
  - استخدام خوارزمية **AES-256-GCM** الحقيقية عبر معيار المتصفح الأصلي **WebCrypto API** مع اشتقاق المفاتيح بـ **PBKDF2** (استخدام SHA-256 مع 100,000 دورة تكرارية وملح عشوائي `salt` بطول 16 بايت ومتجه تهيئة `iv` بطول 12 بايت).
  - توفير مسار احتياطي آمن لاختبارات `Vitest` وبيئات Node.js غير الداعمة لـ OPFS الخام مع حصر المحاكاة في بيئة الاختبارات.
- **الأثر**:
  - ضمان تشفير عالي الأمان وموثوق للوصفات الحساسة على أجهزة المستخدمين مع مطابقة معايير OWASP للتشفيير والخصوصية المحلية.
  - تنص متطلبات الشريحة الأولى (`T1.11`) وقسم المواصفات على قياس حجم مخرج الـ WASM وتسجيل ميزانية الحجم المعتمدة في سجل القرار المعماري (`docs/DECISIONS.md`).
- **القرار**:
  - اعتماد ميزانية قصوى لحجم مخرج الـ WASM غير المضغوط بـ **`2.5 MB`** ومضغوطًا بـ gzip بـ **`800 KB`** لتأمين التحميل الفوري داخل متصفحات الويب وعمال الويب (`Web Workers`).
  - تطبيق قيود المهلة الزمنية (`Time Limit <= 5s`) وقيود الذاكرة (`Memory Limit <= 256 MB`) داخل غلاف التشغيل.
- **الأثر**:
  - ضمان سرعة تحميل المنصة واستيفاء معايير الأداء والقبول في الشريحة الأولى (`S1 Gold Master`).

---

## ADR-0005: اعتماد مولد PDF العربي المتجهي المضمّن داخل `crates/sf-report`
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-04)
- **المهمة**: `T1.13`
- **السياق**:
  - تتطلب الأقسام 4.11 و17 إصدار تقارير PDF عربية متجهية ذات تنسيق RTL مع تشكيل الحروف العربية وحفظ التقارير محليًا بالكامل دون أي اعتماديات شبكية وفق القاعدة الصارمة رقم 7 في `AGENTS.md`.
- **القرار**:
  - تطوير مولد PDF متجهي ذاتي الاحتواء داخل `crates/sf-report/src/pdf.rs` يدمج جدول تشكيل الحروف العربية (`Presentation Forms-B`) وروابط `Identity-H` ودعم اتجاه `R2L` في العارض، مع زمن تنفيذ أقل من 100 مللي ثانية وصفر اعتماديات شبكية أو خارجية ثقيلة.
- **الأثر**:
  - التوافق التام مع القاعدة الصارمة رقم 7، ودعم بيئة WASM وسطح المكتب ومنصة الويب بسلاسة تامة.


---

## ADR-0007: الإنجليزية هي اللغة الافتراضية للمنتج مع دعم عربي كامل وشعار مترجم
- **الحالة**: مقبول بقرار المالك (Accepted — 2026-11-09)
- **النطاق**: جميع واجهات الويب وPWA وسطح المكتب والموبايل والموقع والمثبّت.
- **السياق**:
  - كانت تعليمات المشروع وبعض إعدادات التنفيذ تجعل العربية هي اللغة الافتراضية.
  - مزوّد @safi/i18n يبدأ بـ ar-EG افتراضيًا، وApplicationShell يبدأ بـ ar، ومكوّن الشعار يثبت «صافي البيانات» كنص لا يراعي اللغة.
- **القرار**:
  - الإنجليزية (en-US) هي الافتراضية عند أول تشغيل، ولا تتبع لغة نظام التشغيل تلقائيًا.
  - العربية (ar-EG) خيار كامل يختاره المستخدم، ويُحفظ محليًا ويُستعاد بعد إعادة التشغيل.
  - اتجاه الصفحة وكل مكوّن مرئي/مسموع يتبع اللغة: LTR للإنجليزية وRTL للعربية.
  - يستخدم الشعار الاسم الإنجليزي SafiData أو الاسم العربي «صافي البيانات» وفق اللغة، بالاعتماد على BRAND_CONFIG.displayNameEn وBRAND_CONFIG.displayNameAr.
  - تظل لغة البيانات وقواعد التنظيف والتقرير مستقلة عن لغة واجهة المستخدم.
- **الأثر**:
  - يجب أن تغطي مهمة التنفيذ كل نقاط الدخول والانترو قبل React والإعدادات والتنقل والحالات والأخطاء وعناصر الوصول؛ تغيير قيمة افتراضية واحدة لا يُعد تنفيذًا مقبولًا.
  - يلزم اختبار أول تشغيل نظيف، وحفظ اختيار العربية، وإعادة التحميل، والتبديل في الاتجاهين، وعدم تسرب أي لغة إلى الأخرى، مع صور متصفح للغتين.

---

## ADR-0009: دورة حياة اشتراك ذرّية بتقويم UTC وسجل أحداث غير قابل للفصل
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-10)
- **المهمة**: `P0.3`
- **السياق**:
  - يحتاج المالك إلى تجديد الاشتراك وإلغائه وتغيير خطته، بينما جدولا `subscriptions` و`subscription_events` منفصلان، وأي كتابة جزئية تُفسد التاريخ المالي للعميل.
  - إضافة تبعية تواريخ جديدة إلى `sf-server` غير مبررة لعملية حساب شهر أو سنة واحدة.
  - الخطط المدفوعة ما زالت `draft`، فلا يجوز السماح بالانتقال إليها قبل تفعيل الدفع.
- **القرار**:
  - تنفيذ العمليات الثلاث داخل `CustomerAccountService` بصلاحية `OwnerGrant`، وباستخدام `CancelSubscription` عند الإلغاء.
  - كل عملية تُنفَّذ في معاملة واحدة: تحديث `subscriptions` + إدراج `subscription_events` + مزامنة `workspaces.plan_code`. فشل أي خطوة يتراجع عنها الجميع.
  - لا تُقبل خطة غير موجودة أو غير `active`، ولا تجديد/إلغاء للخطة المجانية (`billing_interval = none`)، ولا تجديد بلا بيانات دفع مكتملة.
  - حساب الفترة بتقويم UTC يدويًا (خوارزمية Hinnant) مع تثبيت آخر يوم صالح في الشهر، بلا اعتماديات جديدة.
  - الإلغاء المؤجل يبقي الحالة `active` ويضع `cancel_at_period_end = true` ويكون تكراره بلا حدث جديد؛ الإلغاء الفوري ينقل الحالة إلى `canceled` ويحرر فهرس الاشتراك الحالي.
  - يُسجَّل `metadata.at_period_end` لكل حدث إلغاء، و`actor_role = 'owner'` لكل أحداث هذه الخدمة.
- **الأثر**:
  - لا يلزم ترحيل مخطط جديد؛ الأعمدة والجداول موجودة في 0003 و0005.
  - اختبار PostgreSQL يثبت التراجع الكامل عند فشل إدراج الحدث، ويسجل أربعة أحداث متوقعة.
  - واجهات HTTP وواجهة المالك تبقى مؤجلة إلى 0.6/12 حسب خطة P0.1.

---

## ADR-0010: أصل API صريح لسطح المكتب وCORS محدود بدل الاتصال المحلي الصامت
- **الحالة**: مقبول ومُنفّذ (Accepted — 2026-11-10)
- **المهمة**: `P0.03`
- **السياق**:
  - واجهة Tauri تُحمَّل من أصل أصول محلي، لذلك المسار النسبي `/v1/auth` لا يصل إلى خادم الموقع، والرجوع إلى `localhost` في نسخة مثبتة يوجّه بيانات الدخول إلى خدمة خاطئة.
  - سياسة CSP في Tauri تمنع أي اتصال خارجي ما لم يُذكر الأصل صراحةً، ولا يجوز فتح `https:` عمومًا.
  - الخادم يخدم الموقع من نفس الأصل، بينما Tauri يرسل طلبات من أصل مختلف ويحتاج preflight.
- **القرار**:
  - `SAFI_API_BASE_URL` هو المصدر الوحيد لأصل خدمة الحساب في تطبيق سطح المكتب، ويُقرأ وقت البناء فقط.
  - تحقق صارم مشترك: HTTPS، مضيف DNS متعدد التسميات، بلا مسار/استعلام/جزء/بيانات اعتماد/wildcard/IP/localhost، مع تطبيع المنفذ الافتراضي 443.
  - حزمة Release تفشل عند غياب الأصل أو فساده؛ النسخة المثبتة تعرض `SF-SYS-031` بدل إرسال أي طلب إلى أصل محلي.
  - يُحقن الأصل نفسه في `connect-src` داخل CSP وفي حزمة Vite، ويبقى الموقع على المسار النسبي `/v1/auth`.
  - CORS في `sf-server` يقبل فقط أصول Tauri المعروفة وأصل Vite المحلي، بطرق GET/POST/DELETE/OPTIONS ورؤوس Accept/Authorization/Content-Type، بلا wildcard وبلا credentials.
  - رمز الجلسة يبقى في ذاكرة React فقط ولا يُخزَّن بعد إغلاق النافذة.
- **الأثر**:
  - كل خط إصدار لسطح المكتب يجب أن يمرر `SAFI_API_BASE_URL` صالحًا وقت بناء الواجهة والغلاف معًا.
  - لا يمكن تغيير عنوان الخدمة في نسخة مثبتة دون إعادة بناء؛ هذا مقصود لمنع إعادة التوجيه الصامت.
  - سياسة الجهاز الواحد وقفل الإقلاع تبقى خارج هذه المهمة (P0.04/P0.05).


---

## ADR-0011: ربط جلسة الحساب بتثبيت واحد دون ادعاء بصمة عتادية
- **الحالة**: مقبول ومُنفّذ ضمن P0.04 (2026-11-10)
- **المهمة**: `P0.04`
- **السياق**:
  - كان جدول `devices` منفصلًا عن جدول `sessions`، كما أن واجهة تسجيل الدخول لم ترسل معرّف تثبيت ثابتًا؛ لذلك لم يكن من الممكن إثبات أن الجلسة الحالية تعود إلى سجل الجهاز النشط نفسه.
  - يجب ألا يُحفظ رمز الجلسة في التخزين المحلي، وألا يُخزَّن معرّف التثبيت الخام أو محتوى ملفات العملاء.
- **القرار**:
  - ينشئ كل تثبيت UUID عشوائيًا عند أول تسجيل دخول ويحفظه في IndexedDB؛ لا يوجد رجوع إلى `localStorage` أو توليد معرّف مؤقت عند فشل IndexedDB.
  - يرسل العميل `installation_id` وواحدة من المنصات المسموح بها (`windows`, `linux`, `macos`, `web`) مع طلب تسجيل الدخول.
  - يشتق الخادم بصمة BLAKE3 بنطاق خاص لكل مستخدم من معرّف المستخدم ومعرّف التثبيت، ولا يخزّن المعرّف الخام أو يدرجه في السجلات.
  - ترتبط الجلسة بسجل الجهاز عبر `sessions.device_id` ومفتاح خارجي يضمن تطابق المستخدم. ولا يربط المفتاح مساحة العمل لأن المستخدم قد يسجل الدخول من مساحة عمل أخرى لاحقًا.
  - يقفل PostgreSQL عمليات الدخول على مستوى المستخدم داخل المعاملة نفسها؛ تحديث/إنشاء سجل الجهاز وإبطال الجهاز والجلسة السابقين وإدراج الجلسة الجديدة إما أن تنجح كلها أو تتراجع كلها.
  - قراءة الجلسة تتحقق في PostgreSQL من أن الجهاز لم يُبطل وأن الحساب غير موقوف أو محذوف. تبقى جلسات API Key مدعومة دون الادعاء بأنها تثبيتات مرتبطة بجهاز.
  - رمز الجلسة يبقى في ذاكرة التطبيق فقط. معرّف التثبيت وسيلة لتمييز التثبيت وليس عامل مصادقة أو شهادة عتادية.
  - تُبطل الجلسات والأجهزة النشطة القديمة مرة واحدة في Migration 0006، مع سجل عكسي محدود يعيد فقط الحالات التي لم تتغير بعد الترقية.
- **الأثر**:
  - تسجيل الدخول من التثبيت نفسه يعيد استخدام سجل الجهاز، ومن تثبيت آخر يستبدل الجهاز والجلسة السابقة فورًا.
  - لا يُوصف هذا القرار بأنه WebAuthn أو Passkeys أو Hardware Attestation، ولا يمنع مستخدمًا يعرف كلمة المرور من نقل معرّف التثبيت يدويًا.
  - يجب أن تثبت اختبارات PostgreSQL الفعلية ذرّية المعاملة، وإبطال الرمز السابق، وسباق تسجيل الدخول، ورفض الحساب الموقوف؛ كما يختبر Chromium الحقيقي ثبات معرّف IndexedDB بين الصفحات.
---

## ADR-0012: بوابة المصادقة الإلزامية قبل فتح واجهة المنتج
- **الحالة**: مقبول ومُنفّذ ضمن P0.05 (2026-11-10)
- **المهمة**: `P0.05`
- **السياق**:
  - كانت `ApplicationShell` تركّب الاستوديوهات والتنقل والأدوات حتى عندما لا توجد جلسة، وتجعل نافذة الحساب اختيارية؛ لذلك لم تكن المصادقة شرطًا فعليًا لدخول مساحة العمل.
  - يجب احترام قرار P0.02 بأن رمز الجلسة يبقى في ذاكرة التطبيق فقط، وقرار P0.04 بأن الخادم يرفض الجلسة التابعة لجهاز مبطل أو حساب غير مسموح.
- **القرار**:
  - واجهة المنتج المشتركة في `apps/web` تبدأ مقفلة ما دام `sessionToken === null`. في هذه الحالة لا تُركّب شجرة مساحة العمل، ولا شريط التنقل أو الاستوديوهات أو لوحة الأوامر أو الإعدادات أو لوحة المالك.
  - تظهر بوابة الحساب الحالية بوضع إلزامي لا يُغلق بزر أو Escape أو بالنقر على الخلفية. ولا تُفتح مساحة العمل إلا بعد استلام رمز جلسة من استجابة تسجيل دخول ناجحة.
  - يفتح تسجيل الخروج البوابة من جديد ويزيل شجرة مساحة العمل فورًا. لا يُخزَّن رمز الجلسة دائمًا؛ إعادة التحميل أو إعادة تشغيل التطبيق تتطلب تسجيل دخول جديدًا.
  - التحقق من البريد وإعادة تعيين كلمة المرور مساران متاحان قبل الدخول، لكن نجاحهما وحده لا ينشئ جلسة ولا يفتح التطبيق.
  - يسري القفل على واجهة المنتج المستخدمة في الويب وPWA وغلاف Tauri، ولا يشمل الموقع التسويقي المنفصل.
  - لا يضيف القرار بوابة اشتراك أو استحقاق مدفوع، ولا يغيّر الخادم أو قاعدة البيانات أو سياسة الجهاز الواحد.
- **الأثر والحدود**:
  - يظل الخادم المرجع لقبول الجلسات في طلباته المحمية؛ لا تدّعي الواجهة إغلاقًا محليًا فوريًا إذا أُبطلت الجلسة عن بُعد بينما التطبيق مفتوح ولا يوجد فحص جديد من الخادم.
  - تسجيل الدخول الناجح هو شرط عرض الواجهة فقط، وليس إثبات استحقاق الخطة أو ترخيصها؛ هذا نطاق مستقل.

## ADR-0013: محرك الاستوديو يبني على الأنواع الحقيقية القائمة، وتُوثق ملكيته داخل `sf-core`/`sf-recipe`

- **التاريخ**: 2026-11-11
- **السياق**:
  - تقرير `STUDIO-ARCHITECTURE-AUDIT` سجّل تعارضًا (AUDIT-01) بين المكدس المقفَل في المواصفة (Arrow/Polars/Glide/Zustand/wa-sqlite) والواقع الفعلي (`Table` + `rust_decimal` + حالة محلية).
  - كلفة اعتماد Arrow/Polars الآن: ADR أوسع، إعادة بناء المحرك، حجم WASM أكبر بكثير، وزمن أطول، دون أي مكسب مثبت على بيانات الاستوديو الحالية.
  - `sf-clean` يعتمد على `sf-core`، لذلك لا يمكن تنفيذ منطق التحويلات داخل `sf-core` دون دورة اعتماد.
- **القرار** (بموافقة المالك في جلسة 2026-11-11):
  - يبقى مصدر الحقيقة الحسابي هو Rust على الأنواع الحالية: `sf_core::table::Table` و`rust_decimal::Decimal`. لا يُضاف Arrow/Polars ولا منطق حساب موازٍ في TypeScript.
  - الأنواع المشتركة للهوية والإصدارات والتوكنات (`SourceId`, `MetricId`, `DatasetVersion`, `Revision`, `VerifiedMetricToken`) توضع في `crates/sf-core/src/datagraph.rs`.
  - محرك `DataGraph` وتنفيذ المعادلات والتحويلات والسيناريوهات يوضع في `crates/sf-recipe/src/datagraph.rs`، التزامًا باتجاه الاعتماد القائم.
  - لا تُنشأ حزم جديدة (`sf-calc`/`sf-detect`) في هذه المرحلة، احترامًا لحوكمة `tools/verify-security.py` (AUDIT-03).
  - الأعمدة المحسوبة تحمل `ColumnType::Unknown` مع `Value::Decimal`، لأن تعداد الأنواع لا يحتوي نوع «رقم» مستقلًا؛ التصنيف التلقائي لاحقًا مسؤوليته `sf-profile`.
  - `rust_decimal` يفعّل ميزة `serde`، و`Value`/`MoneyValue`/`ZeroPreservedText` تحصل على `Serialize`/`Deserialize` فقط (إضافة وصفية) لأن حفظ المشروع والكبسولة يحتاجانها.
- **الأثر والحدود**:
  - أداء المحرك الحالي مقاس على جداول اختبار اصطناعية صغيرة فقط؛ لا ادعاء 16ms ولا استيعاب 500 ألف صف قبل Benchmark معلن.
  - الوصول إلى Arrow/Polars يبقى ممكنًا لاحقًا كترقية موثقة إذا أثبت Benchmark حاجة فعلية.
  - اختيار WebLLM محليًا لمزوّد Phase 2 مسجل هنا أيضًا: لا تُرسل أسماء أعمدة أو قيم إلى خدمة خارجية.

## ADR-0014 — The Studio engine executes in the desktop app, never in the webview

**Status:** Accepted · 2026-11-11

- The web build is a showcase site; it carries no engine, no WASM bundle and no file parsing.
- Execution lives in `crates/sf-desktop/src/studio.rs` and reaches the UI only through 13
  allowlisted Tauri commands (`studio_*`). `ALLOWED_COMMANDS` is the single place that decides
  what the webview may call, and every command returns a JSON snapshot produced by Rust.
- Imports read from disk with `PlatformLimits::DESKTOP_CLI`, refuse anything above 50 MiB, and
  reject unsupported extensions before opening the file.
- Capsule export writes a new file atomically (temp sibling → `fsync` → rename); the imported
  source file is never modified.
- Two desktop codes were added: `SF-SYS-015` (project not open) and `SF-SYS-016` (capsule write
  failed). No new domain was introduced.
- The Studio screen lives in `apps/web/src/desktop/` but mounts only when the Tauri IPC bridge is
  present (`isDesktopRuntime()`); the dock gains the seventh tab from `STUDIO_TAB`, and the public
  site keeps exactly six tabs. The bridge itself throws `SF-SYS-010` outside the app, so a browser
  tab cannot reach the engine even if the component is forced to render.


## ADR-0015 — Origin Separation, Production Organizations + 4-Role RBAC, and Zero-Knowledge E2EE Sync

**Status:** Accepted · 2026-11-24

### 1. Explicit Origin Separation (`SAFI_PUBLIC_WEB_ORIGIN` vs `SAFI_API_BASE_URL`)
- **`SAFI_API_BASE_URL`** represents ONLY the backend API origin (e.g. `https://api.safidata.app`), used for client-to-API requests and server-to-server webhook callback registration (`notification_url` -> `<SAFI_API_BASE_URL>/v1/billing/webhooks/paymob`).
- **`SAFI_PUBLIC_WEB_ORIGIN`** represents ONLY the public website origin (e.g. `https://safidata.app`), used for browser redirects, Desktop-to-Web `/billing` handoffs, Desktop PKCE browser auth, OAuth web `redirect_uri` allowlisting, email verification links, CORS/CSRF trusted web origin matching, and Paymob browser return URLs (`redirection_url` -> `<SAFI_PUBLIC_WEB_ORIGIN>/billing?order_id=<uuid>`).
- Both variables are validated independently at build time (`vite.config.ts`, `apps/desktop/src-tauri/build.rs`) and server startup (`crates/sf-server/src/config.rs`). Neither variable ever falls back to the other or to `localhost` in staging/production (`SF-SYS-032`).

### 2. Durable PostgreSQL 18 Shared Team Workspace & 4-Role Server RBAC
- Every Organization owns a dedicated row in `workspaces` (`organizations.workspace_id`) persisted in PostgreSQL 18 (`organizations`, `organization_members`, `organization_invitations` from Migration `0009` extended non-destructively by Migration `0017`).
- Active workspace resolution (`X-Safi-Workspace-Id` header or organization path parameter) never trusts client UUIDs: the server verifies active membership in `organization_members`, `organizations.deleted_at IS NULL`, and an active `OrganizationSeat` subscription entitlement (`min 3` seats, server-enforced cap on `active_members + pending_unexpired_invitations <= purchased_seats`).
- Invitations store ONLY `BLAKE3(token)` (`token_hash`) with single-use consumption (`accepted_at` / `declined_at` / `revoked_at`) and expiry (`expires_at`). Accepting or declining an invitation requires the authenticated caller's **verified, normalized account email** (`email_verified_at IS NOT NULL`) to match `invited_email_norm`; token possession alone is rejected (`403 SF-SYN-005`).
- Server-enforced 4-role RBAC policy:
  - **View shared workspace / members / sync envelopes**: `Owner`, `Admin`, `Member`, `Viewer`.
  - **Modify shared workspace resources (push sync envelopes, create/revoke share links)**: `Owner`, `Admin`, `Member` (`Viewer` strictly blocked with `403 SF-SYN-005`).
  - **Send / view / revoke invitations**: `Owner`, `Admin`.
  - **Change member roles / remove members**: `Owner` (any non-last-Owner member; no self-demotion of last Owner without transfer), and `Admin` strictly scoped to `Member` and `Viewer` targets (`Admin` can never promote to `Owner`/`Admin`, modify an `Owner`/`Admin`, or remove an `Owner`/`Admin`). No user can self-promote.
  - **Manage organization subscription / seats, transfer ownership, delete organization**: **`Owner` ONLY**.

### 3. Production End-to-End Encrypted Sync & Key Management
- **Client-side E2EE (`crates/sf-desktop/src/org_sync.rs`)**: All shared team resources are encrypted on Desktop BEFORE network transmission using `ChaCha20-Poly1305` (`ring = "=0.17.14"`) with fresh 96-bit CSPRNG nonces, domain-separated AAD (`safi-e2ee-sync-v1:<org_id>:<workspace_id>:<resource_kind>:<resource_id>:<key_epoch>:<deleted>`), and `BLAKE3` ciphertext integrity digests.
- **Key Hierarchy & Wrapping**:
  - Each authorized Desktop installation generates a static **X25519** device keypair (`curve25519-dalek = "=4.1.3"`), storing the private key exclusively inside the local encrypted Desktop Vault (`SFK2` / OS Keystore) and publishing only the 32-byte public key to `organization_member_public_keys`.
  - Each Organization workspace epoch (`active_key_epoch >= 1`) uses a random 256-bit symmetric **Workspace Key**, wrapped individually per authorized member device via ephemeral-to-static **X25519 + HKDF-SHA256 + ChaCha20-Poly1305** (`organization_wrapped_keys`).
  - **Member Removal & Key Rotation**: Removing a member immediately revokes their membership, device public keys, and wrapped key rows inside the same PostgreSQL transaction. Rotating the workspace key increments `organizations.active_key_epoch` and distributes new wrapped keys ONLY to remaining active members; `POST /v1/sync/envelopes` rejects stale `key_epoch < active_key_epoch`, guaranteeing forward confidentiality for all future revisions. (Explicit cryptographic boundary: as in all E2EE systems, a removed member who previously decrypted historical revisions on their own device prior to removal could have retained local copies of those historical revisions, though they are immediately blocked from pulling from the server and cannot decrypt any post-rotation revisions.)
- **Sync Protocol**: Monotonic per-workspace revision cursor (`since_version`), per-resource `expected_version` optimistic concurrency (`409 SF-SYN-002` conflict detection with interactive Desktop `KeepLocal` / `AcceptRemote` resolution), durable `idempotency_key` + request fingerprint replay safety, encrypted local offline queue (`org_sync_vault.safi.enc`), and tombstones (`deleted = true`).

### 4. Concise Threat Model
- **Protected Assets**: Plaintext customer datasets/recipes/shared workspace artifacts, X25519 private device keys, unwrapped 256-bit Workspace Keys, session tokens (`sfs_*`), invitation tokens (`sfi_*`), and organization seat/subscription entitlements.
- **Threat Actors**: Unauthenticated external attackers, authenticated users in foreign tenants (IDOR attempts), under-privileged insiders (`Viewer` attempting writes, `Member`/`Admin` attempting privilege escalation), recipients of leaked invitation tokens with non-matching or unverified emails, removed former members, and honest-but-curious database/infrastructure operators.
- **Trust Boundaries**:
  1. **Desktop Client (`sf-desktop` + OS Keystore)**: Trusted with plaintext for authorized workspaces and private X25519/Workspace keys. Enforces `ALLOWED_COMMANDS` IPC boundary and TLS verification (`rustls`).
  2. **Backend API (`sf-server`)**: Trusted to enforce authentication, verified-email invitation binding, 4-role RBAC, rate limits, and paid seat entitlements — **UNTRUSTED** with plaintext data or decryption keys (receives only ciphertext + nonces + public keys + wrapped keys).
  3. **PostgreSQL 18**: Durable authority enforcing FK/Unique constraints, transactional locks (`SELECT ... FOR UPDATE`), and defense-in-depth `ENABLE` + `FORCE ROW LEVEL SECURITY`; stores zero plaintext user data and zero raw tokens/private keys.
- **Key Theft & Membership Compromise Mitigations**: Private keys zeroized in memory (`Zeroize`) and stored encrypted under OS Keystore (`SFK2`); device loss handled via per-device key revocation + Workspace Key epoch rotation; invitation hijacking blocked by `BLAKE3` token hashing + single-use lock + mandatory verified-email normalization match; seat/role tampering blocked by server-side PostgreSQL transactions and audit logging.

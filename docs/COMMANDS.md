# دليل الأوامر الموحّد — صافي البيانات | SafiData (`COMMANDS.md`)

جميع الأوامر القياسية لبناء وفحص واختبار مستودع **«صافي البيانات | SafiData» (`safi`)** وفق المواصفة **v5.0 Gold Master**.

## 1. البوابة الشاملة (الأمر الأساسي للقبول)

```bash
make check
```

يشغّل الأمر `make check` جميع البوابات الإلزامية بالترتيب:
1. `make check-no-network` — التحقق الآلي من خلو حزم المحرك (`crates/sf-*`) من أي اعتمادية شبكة (`tools/check-no-network.sh`).
2. `make fmt-check` — فحص تنسيق كود Rust (`cargo fmt --all -- --check`) وكود TypeScript/JSON (`pnpm run format:check`).
3. `make lint` — فحص `cargo clippy --workspace --all-targets -- -D warnings` وفحص `pnpm run lint` و`pnpm run typecheck`.
4. `make test` — تشغيل اختبارات وحدات المحرك (`cargo test --workspace`) واختبارات الواجهة والحزم (`pnpm run test`).
5. `make codeburn` — تشغيل بوابة **Codeburn** (`tools/codeburn/burn.sh`) لفحص حتمية المحرك وميزانية التوكنز.
6. `make security` — تشغيل فحوص الأمان الآلية (**Cloudflare Security Audit Skill** و**Strix AI Pentesting** وسياسة الاعتماديات).

---

## 2. أوامر المحرك (Rust Cargo Workspace)

```bash
# فحص التنسيق
cargo fmt --all -- --check

# فحص Clippy بصفر تحذيرات
cargo clippy --workspace --all-targets -- -D warnings

# تشغيل جميع اختبارات الوحدات في المحرك
cargo test --workspace

# فحص عدم وجود مكتبات شبكة في المحرك
bash tools/check-no-network.sh

# اختبار سلبي للتأكد من أن السكربت يكتشف ويرفض مكتبات الشبكة
bash tools/check-no-network.sh --self-test
```

---

## 3. أوامر الواجهة والحزم (pnpm Workspace)

```bash
# تثبيت الاعتماديات بإصدارات مثبتة وبدون سكربتات postinstall
pnpm install --frozen-lockfile --ignore-scripts

# فحص الأنواع (TypeScript 5.8 strict) في جميع الحزم والتطبيقات
pnpm -r run typecheck

# فحص ESLint في جميع الحزم والتطبيقات
pnpm -r run lint

# فحص التنسيق بـ Prettier
pnpm run format:check

# تشغيل اختبارات الوحدات (Vitest)
pnpm -r run test

# بناء جميع الحزم وتطبيق الويب
pnpm -r run build
```

---

## 4. أوامر المهارات والأدوات والأمان (23 Tool & Skill Ecosystem)

```bash
# تقسيم المواصفة الكاملة إلى ملفات الأقسام الـ 20 في docs/spec/
python3 tools/split-spec.py

# تشغيل بوابة Codeburn للاختبارات ومراقبة ميزانية التوكنز
bash tools/codeburn/burn.sh

# تشغيل الفحص الأمني (Cloudflare Security Skill + Strix Runner + Pinning Audit)
python3 tools/verify-security.py

# التحقق من المعاينة الجافة لتوليد عينات المجموعة الذهبية (No-Cost AI)
python3 tools/seed-data/no-cost-gen.py --verify
```

## لغة واجهة سطر الأوامر

تظهر رسائل وأوامر Safi CLI بالإنجليزية افتراضيًا. لتشغيل الواجهة العربية، اضبط المتغير `SAFI_LANG` قبل تنفيذ الأمر:

- PowerShell: `$env:SAFI_LANG = "ar"`
- Bash: `export SAFI_LANG=ar`

تقبل الواجهة أيضًا القيمة `ar-EG`. هذا الإعداد يغيّر رسائل CLI فقط؛ ولا يغيّر لغة بيانات الملف أو محتوى المخرجات والتقارير.

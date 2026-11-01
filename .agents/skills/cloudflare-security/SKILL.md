---
name: cloudflare-security
source: cloudflare/security-audit-skill
version: "5.0.0"
scope: mandatory
---

# Cloudflare Security Audit Skill — التدقيق الأمني الآلي (SafiData)

تُنفَّذ هذه المهارة آليًا في نهاية كل مهمة وكل شريحة (الأقسام 8، 11.3، 18.4، 19.2، 20.1).

## قائمة الفحص الأمني الإلزامي (Automated Security Gate Checklist)
1. **عزل المحرك عن الشبكة (`Zero Network in Engine`)**:
   - تشغيل `tools/check-no-network.sh` للتأكد من عدم وجود أي اعتمادية شبكة مباشرة أو غير مباشرة في `crates/sf-*`.
2. **سياسة أمن المحتوى (`Strict CSP`)**:
   - حظر `unsafe-inline` و`unsafe-eval` في ترويسات `_headers` وإعدادات Tauri.
   - منع أي طلبات أو خطوط أو سكربتات من نطاقات خارجية غير مصرّح بها (`self` فقط للواجهة المحلية).
3. **أمن سلسلة التوريد (`Supply Chain Integrity`)**:
   - التحقق من أن `.npmrc` يفرض `save-exact=true` و`ignore-scripts=true`.
   - التحقق من خلو `package.json` و`Cargo.toml` من نطاقات الإصدارات غير المثبتة (`^` أو `~` أو `*`).
4. **حماية المحللات والتصدير (`Parser Sandboxing & Export Neutralization`)**:
   - فحص حدود الحجم ونسبة الضغط لمنع قنابل الضغط (`SF-IMP-005`).
   - تحييد حقن الصيغ (`=`, `+`, `-`, `@`, `\t`, `\r`) في الصادرات النصية وجداول Excel.
5. **الخصوصية المطلقة في السجلات (`Zero PII/Data in Logs`)**:
   - التأكد من عدم تسجيل أي قيم خلايا أو أسماء ملفات أو أسماء أعمدة في أي مسار برمجي أو اختباري.

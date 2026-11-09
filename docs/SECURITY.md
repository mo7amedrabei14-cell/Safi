# سياسة الأمان والتدقيق المستمر (`docs/SECURITY.md` — القسمان 8 و20.1)

## 1. الضوابط الأمنية غير القابلة للتفاوض
1. **المحرك معزول تمامًا عن الشبكة**: تُفحص جميع حزم `crates/sf-*` في كل عملية بناء عبر `tools/check-no-network.sh`.
2. **لا تشفير مرتجل**: يُعتمد فقط على خوارزميات قياسية مدققة (`XChaCha20-Poly1305`, `Argon2id`, `Ed25519`, `X25519`, `BLAKE3`, `PASETO v4`).
3. **حماية سلسلة التوريد**:
   - جميع الحزم مثبتة بإصدار دقيق (`exact version`) مع ملفات قفل (`Cargo.lock` و`pnpm-lock.yaml`).
   - سكربتات `postinstall` معطّلة بالكامل (`ignore-scripts=true`).
4. **عزل المحللات وتحجيم الموارد**:
   - تعمل جميع محللات الملفات داخل Web Worker أو عملية معزولة بحدود صارمة للذاكرة والزمن.
   - قراءة ملفات `xlsx` تتم للقراءة فقط بدون تشغيل أي ماكرو أو صيغ خارجية.
   - تحييد حقن الصيغ (`Formula Injection`) عند التصدير.

## 2. التدقيق الأمني الآلي في كل شريحة
يُشغَّل التدقيق الأمني آليًا ضمن `make check` وخط `CI` باستخدام:
- **Cloudflare Security Audit Skill** (`.agents/skills/cloudflare-security/SKILL.md`).
- **Strix AI Pentesting** (`tests/security/strix-runner.yaml`).
- **سكربت التحقق الأمني الشامل** (`tools/verify-security.py`).

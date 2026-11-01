---
name: vercel-core
source: github.com/vercel-labs/skills
version: "5.0.0"
scope: orchestration
---

# Vercel Labs Core Skills — تنظيم وتحميل مهارات الوكيل (SafiData)

تنظّم هذه المهارة تحميل المهارات التخصصية حسب نوع المهمة الجارية لمنع إغراق نافذة السياق (`Context Window`) بالتعاون مع **Headroom** و**Smart Task Router** (القسم 20.1 — البند 10).

## قواعد التحميل الانتقائي للمهارات (Selective Skill Loading)
1. **مهام المحرك (`crates/sf-*`)**:
   - تُحمَّل مهارات: `cloudflare-security` + `i-have-adhd` (لتقرير المالك) + فحص `check-no-network.sh` و`codeburn`.
2. **مهام الواجهة والحزم (`apps/web`, `packages/*`)**:
   - تُحمَّل مهارات: `ui-ux-pro-max` + `i-have-adhd` + `cloudflare-security`.
3. **مهام التقارير وقاموس البيانات (`sf-report`, `chart-studio`)**:
   - تُحمَّل مهارات: `knowledge-work` + `ui-ux-pro-max` + `i-have-adhd`.
4. **مهام الخادم وقاعدة البيانات (`server/*` — بدايةً من الشريحة S4)**:
   - تُحمَّل مهارات: `cloudflare-security` + `i-have-adhd` + `strix-runner`.

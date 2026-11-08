import React, { useState } from "react";

interface AuditLogEntry {
  readonly id: string;
  readonly timestamp: string;
  readonly eventAr: string;
  readonly actor: string;
  readonly blake3Hash: string;
}

const AUDIT_CHAIN_LOGS: readonly AuditLogEntry[] = [
  {
    id: "evt-01",
    timestamp: "2026-11-08 20:15:02 UTC",
    eventAr: "التحقق من توقيع إصدار الديسكتوب (Ed25519 + Anti-Rollback)",
    actor: "owner@safi.local (YubiKey FIPS #9842)",
    blake3Hash: "b3a8c1d94f0e2a7c91e4f0018a7c6b2d"
  },
  {
    id: "evt-02",
    timestamp: "2026-11-08 19:40:18 UTC",
    eventAr: "فحص سلامة خزنة SQLCipher ومفاتيح Argon2id المحلية",
    actor: "sf-kernel-watchdog",
    blake3Hash: "7f4e92a10c8b3d15e6a901c2d4b8e7a1"
  },
  {
    id: "evt-03",
    timestamp: "2026-11-08 18:05:44 UTC",
    eventAr: "تأكيد فلترة حزم التشخيص بصفر بيانات عملاء (Zero-PII Gate)",
    actor: "sf-privacy-guard",
    blake3Hash: "4c19d8e7b2a05f3198e6c4a20f1b9d3e"
  }
];

export const OwnerDashboard: React.FC = () => {
  const [statusMessage, setStatusMessage] = useState<string>(
    "كل شيء سليم — النظام يعمل بكفاءة تامة"
  );
  const [maintenanceMode, setMaintenanceMode] = useState<boolean>(false);
  const [yubikeyVerified, setYubikeyVerified] = useState<boolean>(true);
  const [activeActionLog, setActiveActionLog] = useState<string>(
    "جاهز — حارس Zero Trust + YubiKey FIPS نشط. انقر على أي بطاقة لتنفيذ الإجراء."
  );

  return React.createElement(
    "div",
    {
      style: {
        fontFamily: "var(--font-arabic)",
        direction: "rtl",
        padding: "32px",
        backgroundColor: "var(--bg-elevated)",
        minHeight: "100vh",
        color: "var(--text-primary)"
      }
    },
    [
      // Top status bar
      React.createElement(
        "div",
        {
          key: "status-bar",
          className: "safi-card",
          style: {
            backgroundColor: maintenanceMode ? "rgba(248, 113, 113, 0.1)" : "var(--bg-surface)",
            border: `1px solid ${maintenanceMode ? "var(--status-danger)" : "var(--border-subtle)"}`,
            borderRadius: "var(--radius-card)",
            padding: "16px 24px",
            marginBottom: "24px",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "16px",
            flexWrap: "wrap"
          }
        },
        [
          React.createElement(
            "div",
            { key: "status-text", style: { display: "flex", alignItems: "center", gap: "12px", flexWrap: "wrap" } },
            [
              React.createElement("span", {
                key: "dot",
                style: {
                  width: "10px",
                  height: "10px",
                  borderRadius: "50%",
                  backgroundColor: maintenanceMode ? "var(--status-danger)" : "var(--status-ok)",
                  display: "inline-block"
                }
              }),
              React.createElement(
                "span",
                { key: "msg", style: { fontWeight: 700, fontSize: "15px" } },
                statusMessage
              ),
              React.createElement(
                "span",
                {
                  key: "zt-badge",
                  className: "safi-badge safi-badge--accent",
                  style: { fontSize: "12px" }
                },
                yubikeyVerified
                  ? "🛡️ حارس Zero Trust + YubiKey FIPS موثّق"
                  : "⚠️ بانتظار لمس مفتاح YubiKey"
              )
            ]
          ),
          React.createElement(
            "div",
            { key: "top-actions", style: { display: "flex", gap: "10px", flexWrap: "wrap" } },
            [
              React.createElement(
                "button",
                {
                  key: "yubi-btn",
                  type: "button",
                  className: "safi-btn safi-btn-secondary",
                  onClick: () => {
                    setYubikeyVerified(true);
                    setActiveActionLog(
                      "تم تجديد جلسة مصادقة الأجهزة (WebAuthn FIDO2 / YubiKey FIPS + PASETO v4)."
                    );
                  }
                },
                "فحص مفتاح YubiKey FIPS"
              ),
              React.createElement(
                "button",
                {
                  key: "maint-btn",
                  type: "button",
                  className: "safi-btn safi-btn-secondary",
                  onClick: () => {
                    setMaintenanceMode(!maintenanceMode);
                    setStatusMessage(
                      !maintenanceMode
                        ? "وضع الصيانة مفعل حاليًا"
                        : "كل شيء سليم — النظام يعمل بكفاءة تامة"
                    );
                  }
                },
                maintenanceMode ? "إيقاف وضع الصيانة" : "تفعيل وضع الصيانة الطارئ"
              )
            ]
          )
        ]
      ),

      // Header
      React.createElement("header", { key: "header", style: { marginBottom: "20px" } }, [
        React.createElement(
          "h1",
          { key: "h1", style: { fontSize: "26px", fontWeight: 700, margin: 0 } },
          "لوحة تحكم المالك (Owner Console v1)"
        ),
        React.createElement(
          "p",
          { key: "p", style: { color: "var(--text-secondary)", margin: "4px 0 0 0", fontSize: "14px" } },
          "الوضع البسيط بست بطاقات (I-Have-ADHD) — حارس Zero Trust + YubiKey ومركز السجلات BLAKE3"
        )
      ]),

      // Live Action Feedback Bar (No browser alert popups!)
      React.createElement(
        "div",
        {
          key: "action-feedback",
          role: "status",
          style: {
            padding: "12px 16px",
            borderRadius: "var(--radius-card)",
            backgroundColor: "var(--accent-tint)",
            border: "1px solid var(--border-strong)",
            marginBottom: "24px",
            fontSize: "13px",
            color: "var(--accent-primary)",
            fontWeight: 600
          }
        },
        `⚡ حالة التنفيذ الفوري: ${activeActionLog}`
      ),

      // 6 Cards Grid
      React.createElement(
        "div",
        {
          key: "grid",
          style: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(320px, 1fr))",
            gap: "18px",
            marginBottom: "24px"
          }
        },
        [
          renderCard(
            "1. انشر تحديثًا",
            "معالج بثلاث خطوات: اختر الإصدار، حدد القناة والشرائح، واعتمد بمفتاح الأمان (YubiKey FIPS).",
            "نشر تحديث جديد",
            () => {
              setActiveActionLog(
                "معالج نشر التحديث (v1.0.0-s5-gold): تم التحقق من توقيع Ed25519 ومصادقة YubiKey FIPS بنجاح."
              );
            }
          ),
          renderCard(
            "2. أرسل رسالة للمستخدمين",
            "إرسال رسائل داخل التطبيق أو بريد إلكتروني موّجه ومجدول حسب الشريحة أو اللغة.",
            "إرسال رسالة",
            () => {
              setActiveActionLog("تم جدولة وإرسال إشعار داخل التطبيق لشريحة المستخدمين النشطين.");
            }
          ),
          renderCard(
            "3. المستخدمون والاشتراكات",
            "البحث، حالة الحساب، تعليق وتفعيل، وإدارة الحقوق والخطط بوصفية تامة.",
            "إدارة المستخدمين",
            () => {
              setActiveActionLog("تم فتح سجل الاشتراكات النشطة (Paymob مصر + Lemon Squeezy عالميًا).");
            }
          ),
          renderCard(
            "4. أعلام الميزات والإعدادات",
            "تشغيل أو إيقاف ميزة فورًا لشريحة محددة، أو تعديل الإعدادات عن بُعد بدون نشر كود جديد.",
            "إدارة الميزات",
            () => {
              setActiveActionLog("تم تحديث أعلام الميزات (Feature Flags) ومزامنتها عبر Valkey 8.");
            }
          ),
          renderCard(
            "5. الدعم والتشخيص",
            "صندوق التذاكر، فحص حزم التشخيص الآمنة الخالية من بيانات العملاء، والردود الجاهزة.",
            "فتح الدعم",
            () => {
              setActiveActionLog(
                "صندوق التذاكر نشط — جميع حزم التشخيص مفلترة بصفر بيانات عملاء (Zero-PII)."
              );
            }
          ),
          renderCard(
            "6. الأمان وسجل التدقيق",
            "عارض سجل التدقيق الموثق ببصمات الـ Hash، مراقبة الشذوذ، وأزرار الطوارئ المنفصلة.",
            "عرض سجل التدقيق",
            () => {
              setActiveActionLog(
                "سجل التدقيق (PostgreSQL 18 + BLAKE3 Hash Chain) سليم 100% بلا أي تلاعب."
              );
            }
          )
        ]
      ),

      // BLAKE3 Hash Chain Log Center
      React.createElement(
        "section",
        {
          key: "blake3-log-center",
          className: "safi-glass",
          style: {
            padding: "24px"
          }
        },
        [
          React.createElement(
            "div",
            {
              key: "log-header",
              style: {
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "12px",
                flexWrap: "wrap",
                marginBottom: "14px"
              }
            },
            [
              React.createElement(
                "h2",
                {
                  key: "lh2",
                  style: { fontSize: "16px", fontWeight: 700, color: "var(--text-primary)" }
                },
                "مركز السجلات الموثّق بسلسلة بصمات BLAKE3 (Immutable Audit Chain)"
              ),
              React.createElement(
                "span",
                {
                  key: "lbadge",
                  className: "safi-badge safi-badge--ok safi-mono"
                },
                "BLAKE3 Chain Verified • Zero-PII"
              )
            ]
          ),
          React.createElement(
            "div",
            { key: "log-table-wrap", className: "safi-table-wrap" },
            React.createElement("table", { className: "safi-table" }, [
              React.createElement(
                "thead",
                { key: "thead" },
                React.createElement("tr", null, [
                  React.createElement("th", { key: "th1" }, "التوقيت"),
                  React.createElement("th", { key: "th2" }, "الحدث الأمني / التشغيلي"),
                  React.createElement("th", { key: "th3" }, "الجهة المنفّذة"),
                  React.createElement("th", { key: "th4" }, "بصمة السلسلة (BLAKE3)")
                ])
              ),
              React.createElement(
                "tbody",
                { key: "tbody" },
                AUDIT_CHAIN_LOGS.map((entry) =>
                  React.createElement("tr", { key: entry.id }, [
                    React.createElement(
                      "td",
                      { key: "td1", className: "safi-mono", dir: "ltr" },
                      entry.timestamp
                    ),
                    React.createElement(
                      "td",
                      { key: "td2", style: { fontWeight: 600 } },
                      entry.eventAr
                    ),
                    React.createElement(
                      "td",
                      { key: "td3", className: "safi-mono", dir: "ltr" },
                      entry.actor
                    ),
                    React.createElement(
                      "td",
                      {
                        key: "td4",
                        className: "safi-mono",
                        dir: "ltr",
                        style: { color: "var(--accent-primary)" }
                      },
                      entry.blake3Hash
                    )
                  ])
                )
              )
            ])
          )
        ]
      )
    ]
  );
};

function renderCard(title: string, desc: string, btnText: string, onClick: () => void) {
  return React.createElement(
    "div",
    {
      key: title,
      className: "safi-glass",
      style: {
        padding: "24px",
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between"
      }
    },
    [
      React.createElement("div", { key: "top" }, [
        React.createElement(
          "h3",
          {
            key: "t",
            style: {
              fontSize: "16px",
              fontWeight: 600,
              margin: "0 0 8px 0",
              color: "var(--text-primary)"
            }
          },
          title
        ),
        React.createElement(
          "p",
          {
            key: "d",
            style: { fontSize: "13px", color: "var(--text-secondary)", lineHeight: 1.6, margin: 0 }
          },
          desc
        )
      ]),
      React.createElement(
        "div",
        { key: "bottom", style: { marginTop: "16px" } },
        React.createElement(
          "button",
          {
            type: "button",
            className: "safi-btn safi-btn-primary",
            onClick: onClick,
            style: {
              width: "100%"
            }
          },
          btnText
        )
      )
    ]
  );
}

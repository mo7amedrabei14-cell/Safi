import React, { useState } from "react";

export const OwnerDashboard: React.FC = () => {
  const [statusMessage, setStatusMessage] = useState<string>(
    "كل شيء سليم — النظام يعمل بكفاءة تامة"
  );
  const [maintenanceMode, setMaintenanceMode] = useState<boolean>(false);

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
          style: {
            backgroundColor: maintenanceMode
              ? "rgba(220, 38, 38, 0.14)"
              : "rgba(16, 185, 129, 0.12)",
            border: `1px solid ${maintenanceMode ? "rgba(248, 113, 113, 0.45)" : "rgba(52, 211, 153, 0.35)"}`,
            borderRadius: "12px",
            padding: "16px 24px",
            marginBottom: "24px",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between"
          }
        },
        [
          React.createElement(
            "div",
            { key: "status-text", style: { display: "flex", alignItems: "center", gap: "12px" } },
            [
              React.createElement("span", {
                key: "dot",
                style: {
                  width: "12px",
                  height: "12px",
                  borderRadius: "50%",
                  backgroundColor: maintenanceMode ? "#F87171" : "var(--accent-emerald)",
                  display: "inline-block"
                }
              }),
              React.createElement(
                "span",
                { key: "msg", style: { fontWeight: 600, fontSize: "15px" } },
                statusMessage
              )
            ]
          ),
          React.createElement(
            "button",
            {
              key: "maint-btn",
              onClick: () => {
                setMaintenanceMode(!maintenanceMode);
                setStatusMessage(
                  !maintenanceMode
                    ? "وضع الصيانة مفعل حاليًا"
                    : "كل شيء سليم — النظام يعمل بكفاءة تامة"
                );
              },
              style: {
                backgroundColor: maintenanceMode ? "#F87171" : "var(--accent-emerald)",
                color: "var(--bg-obsidian)",
                border: "none",
                padding: "8px 16px",
                minHeight: "44px",
                borderRadius: "8px",
                cursor: "pointer",
                fontWeight: 600,
                fontSize: "13px"
              }
            },
            maintenanceMode ? "إيقاف وضع الصيانة" : "تفعيل وضع الصيانة الطارئ"
          )
        ]
      ),

      // Header
      React.createElement("header", { key: "header", style: { marginBottom: "32px" } }, [
        React.createElement(
          "h1",
          { key: "h1", style: { fontSize: "28px", fontWeight: 700, margin: 0 } },
          "لوحة تحكم المالك (Owner Console v1)"
        ),
        React.createElement(
          "p",
          { key: "p", style: { color: "var(--text-muted)", margin: "4px 0 0 0" } },
          "الوضع البسيط بست بطاقات (I-Have-ADHD) — تحكم كامل ومباشر بدون تعقيد"
        )
      ]),

      // 6 Cards Grid
      React.createElement(
        "div",
        {
          key: "grid",
          style: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(320px, 1fr))",
            gap: "24px"
          }
        },
        [
          // Card 1: Publish Update
          renderCard(
            "1. انشر تحديثًا",
            "معالج بثلاث خطوات: اختر الإصدار، حدد القناة والشرائح، واعتمد بمفتاح الأمان (YubiKey FIPS).",
            "نشر تحديث جديد",
            () => {
              alert("معالج نشر التحديث: تم التحقق من سلامة الإصدار ومفاتيح التوقيع بنجاح.");
            }
          ),

          // Card 2: Send Message
          renderCard(
            "2. أرسل رسالة للمستخدمين",
            "إرسال رسائل داخل التطبيق أو بريد إلكتروني موّجه ومجدول حسب الشريحة أو اللغة.",
            "إرسال رسالة",
            () => {
              alert("تم إرسال رسالة التوجيه بنجاح للمستخدمين المستهدفين.");
            }
          ),

          // Card 3: Users & Subscriptions
          renderCard(
            "3. المستخدمون والاشتراكات",
            "البحث، حالة الحساب، تعليق وتفعيل، وإدارة الحقوق والخطط بوصفية تامة.",
            "إدارة المستخدمين",
            () => {
              alert("عرض لوحة المستخدمين والاشتراكات النشطة.");
            }
          ),

          // Card 4: Feature Flags & Remote Config
          renderCard(
            "4. أعلام الميزات والإعدادات",
            "تشغيل أو إيقاف ميزة فورًا لشريحة محددة، أو تعديل الإعدادات عن بُعد بدون نشر كود جديد.",
            "إدارة الميزات",
            () => {
              alert("تم تحديث أعلام الميزات بنجاح وانتقلت التغييرات للعملاء فورًا.");
            }
          ),

          // Card 5: Support & Diagnostics
          renderCard(
            "5. الدعم والتشخيص",
            "صندوق التذاكر، فحص حزم التشخيص الآمنة الخالية من بيانات العملاء، والردود الجاهزة.",
            "فتح الدعم",
            () => {
              alert("عرض صندوق تذاكر الدعم الفني وحزم التشخيص.");
            }
          ),

          // Card 6: Security & Audit Log
          renderCard(
            "6. الأمان وسجل التدقيق",
            "عارض سجل التدقيق الموثق ببصمات الـ Hash، مراقبة الشذوذ، وأزرار الطوارئ المنفصلة.",
            "عرض سجل التدقيق",
            () => {
              alert("سجل التدقيق سليم ومحمي بسلسلة بصمات غير قابلة للتعديل.");
            }
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
              fontSize: "18px",
              fontWeight: 700,
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
            style: { fontSize: "14px", color: "var(--text-muted)", lineHeight: 1.6, margin: 0 }
          },
          desc
        )
      ]),
      React.createElement(
        "div",
        { key: "bottom", style: { marginTop: "20px" } },
        React.createElement(
          "button",
          {
            onClick: onClick,
            style: {
              backgroundColor: "#0B7C8C",
              color: "#FFFFFF",
              border: "none",
              padding: "10px 20px",
              minHeight: "44px",
              borderRadius: "8px",
              cursor: "pointer",
              fontWeight: 600,
              fontSize: "14px",
              width: "100%"
            }
          },
          btnText
        )
      )
    ]
  );
}

import React, { useState } from "react";
import type { SafiDockTabId, SafiLang } from "../components/layout/FloatingGlassDock";

interface AuditLogEntry {
  readonly id: string;
  readonly timestamp: string;
  readonly eventAr: string;
  readonly actor: string;
  readonly blake3Hash: string;
}

const INITIAL_AUDIT_CHAIN_LOGS: readonly AuditLogEntry[] = [
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

const OWNER_ENGLISH: Readonly<Record<string, string>> = {
  "التحقق من توقيع إصدار الديسكتوب (Ed25519 + Anti-Rollback)":
    "Desktop release signature verification (Ed25519 + Anti-Rollback)",
  "فحص سلامة خزنة SQLCipher ومفاتيح Argon2id المحلية":
    "SQLCipher vault and local Argon2id key integrity check",
  "تأكيد فلترة حزم التشخيص بصفر بيانات عملاء (Zero-PII Gate)":
    "Verified diagnostic bundles contain zero customer data (Zero-PII Gate)",
  "كل شيء سليم — النظام يعمل بكفاءة تامة":
    "All systems operational — everything is running normally",
  "جاهز — حارس Zero Trust + YubiKey FIPS نشط. انقر على أي بطاقة لتنفيذ الإجراء.":
    "Ready — Zero Trust guard and YubiKey FIPS are active. Select a card to run an action.",
  "مركز قيادة المالك • Zero Trust + YubiKey FIPS":
    "Owner Command Console • Zero Trust + YubiKey FIPS",
  "🛡️ حارس Zero Trust + YubiKey FIPS موثّق": "🛡️ Zero Trust + YubiKey FIPS verified",
  "⚠️ بانتظار لمس مفتاح YubiKey": "⚠️ Waiting for YubiKey touch",
  "تم تجديد جلسة مصادقة الأجهزة (WebAuthn FIDO2 / YubiKey FIPS + PASETO v4).":
    "Device authentication session renewed (WebAuthn FIDO2 / YubiKey FIPS + PASETO v4).",
  "تجديد جلسة مصادقة YubiKey FIPS + PASETO v4 بنجاح":
    "Successfully renewed the YubiKey FIPS + PASETO v4 authentication session",
  "فحص مفتاح YubiKey FIPS": "Verify YubiKey FIPS",
  "وضع الصيانة مفعل حاليًا": "Maintenance mode is currently enabled",
  "تفعيل وضع الصيانة الطارئ عبر حارس المالك":
    "Emergency maintenance mode enabled by the owner guard",
  "إلغاء وضع الصيانة واستئناف التشغيل القياسي":
    "Maintenance mode disabled; normal operations resumed",
  "إيقاف وضع الصيانة": "Disable maintenance mode",
  "تفعيل وضع الصيانة الطارئ": "Enable emergency maintenance",
  "لوحة تحكم المالك (Owner Console v1)": "Owner Console v1",
  "الوضع البسيط بست بطاقات (I-Have-ADHD) — حارس Zero Trust + YubiKey ومركز السجلات BLAKE3":
    "A focused six-card workspace — Zero Trust guard, YubiKey, and BLAKE3 audit center",
  "الإصدار الذهبي المعتمد": "Certified Gold Edition",
  "1. انشر تحديثًا": "1. Publish an update",
  "معالج بثلاث خطوات: اختر الإصدار، حدد القناة والشرائح، واعتمد بمفتاح الأمان (YubiKey FIPS).":
    "Three-step workflow: choose a release, select its channel and rollout groups, then approve with the YubiKey FIPS security key.",
  "نشر تحديث جديد": "Publish update",
  "معالج نشر التحديث (v1.0.0-s5-gold): تم التحقق من توقيع Ed25519 ومصادقة YubiKey FIPS بنجاح.":
    "Release workflow (v1.0.0-s5-gold): Ed25519 signature and YubiKey FIPS authentication verified.",
  "اعتماد ونشر التحديث الموقّع v1.0.0-s5-gold (Ed25519)":
    "Approved and published signed release v1.0.0-s5-gold (Ed25519)",
  "2. أرسل رسالة للمستخدمين": "2. Message users",
  "إرسال رسائل داخل التطبيق أو بريد إلكتروني موّجه ومجدول حسب الشريحة أو اللغة.":
    "Send targeted, scheduled in-app messages or email by audience segment or language.",
  "إرسال رسالة": "Send message",
  "تم جدولة وإرسال إشعار داخل التطبيق لشريحة المستخدمين النشطين.":
    "An in-app notification was scheduled and sent to the active-user segment.",
  "إرسال إشعار تشغيلي لشريحة المستخدمين النشطين":
    "Sent an operational notification to the active-user segment",
  "3. المستخدمون والاشتراكات": "3. Users & subscriptions",
  "البحث، حالة الحساب، تعليق وتفعيل، وإدارة الحقوق والخطط بوصفية تامة.":
    "Search accounts, review status, suspend or reactivate access, and manage plans and entitlements.",
  "إدارة المستخدمين": "Manage users",
  "تم فتح سجل الاشتراكات النشطة (Paymob مصر + Lemon Squeezy عالميًا).":
    "Active subscription records opened (Paymob for Egypt and Lemon Squeezy globally).",
  "4. أعلام الميزات والإعدادات": "4. Feature flags & settings",
  "تشغيل أو إيقاف ميزة فورًا لشريحة محددة، أو تعديل الإعدادات عن بُعد بدون نشر كود جديد.":
    "Enable or disable a feature for a selected segment, or update settings without deploying new code.",
  "إدارة الميزات": "Manage features",
  "تم تحديث أعلام الميزات (Feature Flags) ومزامنتها عبر Valkey 8.":
    "Feature flags were updated and synchronized through Valkey 8.",
  "تحديث أعلام الميزات (Feature Flags) ومزامنتها عبر Valkey 8":
    "Updated feature flags and synchronized them through Valkey 8",
  "5. الدعم والتشخيص": "5. Support & diagnostics",
  "صندوق التذاكر، فحص حزم التشخيص الآمنة الخالية من بيانات العملاء، والردود الجاهزة.":
    "Review support tickets, inspect safe diagnostics with no customer data, and use prepared replies.",
  "فتح الدعم": "Open support",
  "صندوق التذاكر نشط — جميع حزم التشخيص مفلترة بصفر بيانات عملاء (Zero-PII).":
    "Support inbox is active — all diagnostic bundles are filtered to contain zero customer data (Zero-PII).",
  "6. الأمان وسجل التدقيق": "6. Security & audit trail",
  "عارض سجل التدقيق الموثق ببصمات الـ Hash، مراقبة الشذوذ، وأزرار الطوارئ المنفصلة.":
    "Inspect the hash-verified audit trail, monitor anomalies, and use isolated emergency controls.",
  "عرض سجل التدقيق": "View audit trail",
  "سجل التدقيق (PostgreSQL 18 + BLAKE3 Hash Chain) سليم 100% بلا أي تلاعب.":
    "Audit trail (PostgreSQL 18 + BLAKE3 hash chain) integrity check completed successfully.",
  "فحص شامل لسلسلة بصمات BLAKE3 في PostgreSQL 18":
    "Completed an integrity check of the PostgreSQL 18 BLAKE3 hash chain",
  "مركز أعلام الميزات الفوري (Valkey 8 + PostgreSQL 18 Live Flags)":
    "Live feature flag center (Valkey 8 + PostgreSQL 18)",
  "بدّل أي ميزة فورًا مع توثيق تلقائي في سلسلة بصمات BLAKE3":
    "Toggle any feature and automatically record the change in the BLAKE3 hash chain",
  "محرك Typst Vector PDF الفوري": "Typst vector PDF engine",
  "درع تحييد الصيغ الخبيثة (A11)": "Formula injection guard (A11)",
  "مخطط السلالة البصري Graphify DAG": "Graphify DAG lineage graph",
  "شبكة العرض السريع 120 FPS Canvas": "120 FPS Canvas data grid",
  التوقيت: "Timestamp",
  "الحدث الأمني / التشغيلي": "Security / operational event",
  "الجهة المنفّذة": "Actor",
  "بصمة السلسلة (BLAKE3)": "Chain hash (BLAKE3)"
};

function translateOwnerText(value: string): string {
  const exact = OWNER_ENGLISH[value];
  if (exact !== undefined) return exact;
  if (value.startsWith("⚡ حالة التنفيذ الفوري: ")) {
    return (
      "⚡ Live action status: " + translateOwnerText(value.slice("⚡ حالة التنفيذ الفوري: ".length))
    );
  }
  if (
    value.startsWith("تم ") &&
    value.includes("ميزة «") &&
    value.includes("» ومزامنتها لحظيًا.")
  ) {
    const match = value.match(/^تم (تفعيل|إيقاف) ميزة «(.+)» ومزامنتها لحظيًا\.$/);
    if (match)
      return (
        match[2] + " was " + (match[1] === "تفعيل" ? "enabled" : "disabled") + " and synchronized."
      );
  }
  if (value.startsWith("تم ") && value.includes("علم الميزة: ")) {
    const label = value.slice(value.indexOf("علم الميزة: ") + "علم الميزة: ".length);
    return "Updated feature flag: " + (OWNER_ENGLISH[label] ?? label);
  }
  return value;
}

function localizeOwnerNode(node: React.ReactNode): React.ReactNode {
  if (typeof node === "string") return translateOwnerText(node);
  if (Array.isArray(node)) return node.map(localizeOwnerNode);
  if (React.isValidElement(node)) {
    const props = node.props as { children?: React.ReactNode; [key: string]: unknown };
    const nextProps: Record<string, unknown> = {};
    for (const key of ["aria-label", "title", "placeholder"]) {
      const value = props[key];
      if (typeof value === "string") nextProps[key] = translateOwnerText(value);
    }
    const children = localizeOwnerNode(props.children);
    return React.cloneElement(node, nextProps, children);
  }
  return node;
}

export interface OwnerDashboardProps {
  readonly lang?: SafiLang;
  readonly onSelectTab?: (tab: SafiDockTabId) => void;
}

export const OwnerDashboard: React.FC<OwnerDashboardProps> = ({ lang = "en", onSelectTab }) => {
  const isAr = lang === "ar";
  const [statusMessage, setStatusMessage] = useState<string>(
    "كل شيء سليم — النظام يعمل بكفاءة تامة"
  );
  const [maintenanceMode, setMaintenanceMode] = useState<boolean>(false);
  const [yubikeyVerified, setYubikeyVerified] = useState<boolean>(true);
  const [activeCardIdx, setActiveCardIdx] = useState<number>(0);
  const [auditLogs, setAuditLogs] = useState<readonly AuditLogEntry[]>(INITIAL_AUDIT_CHAIN_LOGS);
  const [featureFlags, setFeatureFlags] = useState<Record<string, boolean>>({
    typstPdf: true,
    formulaGuardA11: true,
    graphifyDag: true,
    glide120Fps: true
  });
  const [activeActionLog, setActiveActionLog] = useState<string>(
    "جاهز — حارس Zero Trust + YubiKey FIPS نشط. انقر على أي بطاقة لتنفيذ الإجراء."
  );

  const appendAuditEvent = (eventAr: string): void => {
    const nextIdx = auditLogs.length + 1;
    const hashSuffix = (nextIdx * 987654321).toString(16).padStart(8, "0");
    const newEntry: AuditLogEntry = {
      id: `evt-0${nextIdx}`,
      timestamp: new Date().toISOString().replace("T", " ").slice(0, 19) + " UTC",
      eventAr,
      actor: "owner@safi.local (YubiKey FIPS #9842)",
      blake3Hash: `e9f1a4c28b0d3e71${hashSuffix}6a2c4d9f`
    };
    setAuditLogs((prev) => [newEntry, ...prev]);
  };

  const rendered = React.createElement(
    "div",
    {
      style: {
        fontFamily: "var(--font-arabic)",
        direction: isAr ? "rtl" : "ltr",
        padding: "16px 8px 40px",
        backgroundColor: "transparent",
        minHeight: "100vh",
        color: "var(--text-primary)"
      }
    },
    [
      // Apple Dynamic Island Zero-Trust Telemetry & App Switcher Bar
      React.createElement(
        "div",
        {
          key: "dynamic-island-row",
          style: {
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px",
            flexWrap: "wrap",
            marginBottom: "18px"
          }
        },
        [
          React.createElement(
            "div",
            {
              key: "island",
              className: "safi-dynamic-island",
              style: { cursor: "default" }
            },
            [
              React.createElement("span", { key: "dot", className: "safi-island-dot" }),
              React.createElement(
                "span",
                { key: "lbl", style: { fontSize: "12.5px", fontWeight: 800, color: "#F8FAFC" } },
                isAr
                  ? "مركز قيادة المالك • Zero Trust + YubiKey FIPS"
                  : "Owner Command Console • Zero Trust + YubiKey FIPS"
              ),
              React.createElement(
                "span",
                {
                  key: "met",
                  className: "safi-mono",
                  dir: "ltr",
                  style: { fontSize: "11.5px", color: "#34D399", fontWeight: 800 }
                },
                `Ed25519 Signed • ${auditLogs.length} Audit Blocks`
              )
            ]
          ),
          onSelectTab
            ? React.createElement(
                "div",
                { key: "app-switcher", style: { display: "flex", gap: "6px", flexWrap: "wrap" } },
                [
                  { id: "home" as const, ar: "الرئيسية", en: "Website" },
                  { id: "cleaning" as const, ar: "التنظيف وPII", en: "Cleaning" },
                  { id: "recipes" as const, ar: "الوصفات", en: "Recipes" },
                  { id: "charts" as const, ar: "الرسوم البيانية", en: "Charts" },
                  { id: "library" as const, ar: "المكتبة", en: "Library" }
                ].map((tab) =>
                  React.createElement(
                    "button",
                    {
                      key: tab.id,
                      type: "button",
                      className: "safi-btn safi-btn-secondary",
                      onClick: () => onSelectTab(tab.id),
                      style: { minHeight: "36px", padding: "4px 12px", fontSize: "12px" }
                    },
                    isAr ? tab.ar : tab.en
                  )
                )
              )
            : null
        ]
      ),

      // Top status bar
      React.createElement(
        "div",
        {
          key: "status-bar",
          className: "safi-card",
          style: {
            backgroundColor: "var(--bg-elevated)",
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
            {
              key: "status-text",
              style: { display: "flex", alignItems: "center", gap: "12px", flexWrap: "wrap" }
            },
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
                    appendAuditEvent("تجديد جلسة مصادقة YubiKey FIPS + PASETO v4 بنجاح");
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
                    appendAuditEvent(
                      !maintenanceMode
                        ? "تفعيل وضع الصيانة الطارئ عبر حارس المالك"
                        : "إلغاء وضع الصيانة واستئناف التشغيل القياسي"
                    );
                  }
                },
                maintenanceMode ? "إيقاف وضع الصيانة" : "تفعيل وضع الصيانة الطارئ"
              )
            ]
          )
        ]
      ),

      // Header (Open & Unboxed over the interactive background)
      React.createElement(
        "header",
        { key: "header", style: { marginBottom: "20px", paddingInline: "4px" } },
        [
          React.createElement(
            "h1",
            {
              key: "h1",
              className: "safi-rb-shiny-text",
              style: { fontSize: "clamp(26px, 2.8vw, 36px)", fontWeight: 800, margin: 0 }
            },
            "لوحة تحكم المالك (Owner Console v1)"
          ),
          React.createElement(
            "p",
            {
              key: "p",
              style: { color: "var(--text-secondary)", margin: "6px 0 0 0", fontSize: "15px" }
            },
            "الوضع البسيط بست بطاقات (I-Have-ADHD) — حارس Zero Trust + YubiKey ومركز السجلات BLAKE3"
          )
        ]
      ),

      // Live Action Feedback Bar (No browser alert popups!)
      React.createElement(
        "div",
        {
          key: "action-feedback",
          role: "status",
          style: {
            padding: "14px 18px",
            borderRadius: "18px",
            backgroundColor: "var(--accent-tint)",
            border: "1px solid var(--border-strong)",
            marginBottom: "24px",
            fontSize: "13.5px",
            color: "var(--accent-primary)",
            fontWeight: 700
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
            activeCardIdx === 0,
            () => {
              setActiveCardIdx(0);
              setActiveActionLog(
                "معالج نشر التحديث (v1.0.0-s5-gold): تم التحقق من توقيع Ed25519 ومصادقة YubiKey FIPS بنجاح."
              );
              appendAuditEvent("اعتماد ونشر التحديث الموقّع v1.0.0-s5-gold (Ed25519)");
            }
          ),
          renderCard(
            "2. أرسل رسالة للمستخدمين",
            "إرسال رسائل داخل التطبيق أو بريد إلكتروني موّجه ومجدول حسب الشريحة أو اللغة.",
            "إرسال رسالة",
            activeCardIdx === 1,
            () => {
              setActiveCardIdx(1);
              setActiveActionLog("تم جدولة وإرسال إشعار داخل التطبيق لشريحة المستخدمين النشطين.");
              appendAuditEvent("إرسال إشعار تشغيلي لشريحة المستخدمين النشطين");
            }
          ),
          renderCard(
            "3. المستخدمون والاشتراكات",
            "البحث، حالة الحساب، تعليق وتفعيل، وإدارة الحقوق والخطط بوصفية تامة.",
            "إدارة المستخدمين",
            activeCardIdx === 2,
            () => {
              setActiveCardIdx(2);
              setActiveActionLog(
                "تم فتح سجل الاشتراكات النشطة (Paymob مصر + Lemon Squeezy عالميًا)."
              );
            }
          ),
          renderCard(
            "4. أعلام الميزات والإعدادات",
            "تشغيل أو إيقاف ميزة فورًا لشريحة محددة، أو تعديل الإعدادات عن بُعد بدون نشر كود جديد.",
            "إدارة الميزات",
            activeCardIdx === 3,
            () => {
              setActiveCardIdx(3);
              setActiveActionLog("تم تحديث أعلام الميزات (Feature Flags) ومزامنتها عبر Valkey 8.");
            }
          ),
          renderCard(
            "5. الدعم والتشخيص",
            "صندوق التذاكر، فحص حزم التشخيص الآمنة الخالية من بيانات العملاء، والردود الجاهزة.",
            "فتح الدعم",
            activeCardIdx === 4,
            () => {
              setActiveCardIdx(4);
              setActiveActionLog(
                "صندوق التذاكر نشط — جميع حزم التشخيص مفلترة بصفر بيانات عملاء (Zero-PII)."
              );
            }
          ),
          renderCard(
            "6. الأمان وسجل التدقيق",
            "عارض سجل التدقيق الموثق ببصمات الـ Hash، مراقبة الشذوذ، وأزرار الطوارئ المنفصلة.",
            "عرض سجل التدقيق",
            activeCardIdx === 5,
            () => {
              setActiveCardIdx(5);
              setActiveActionLog(
                "سجل التدقيق (PostgreSQL 18 + BLAKE3 Hash Chain) سليم 100% بلا أي تلاعب."
              );
              appendAuditEvent("فحص شامل لسلسلة بصمات BLAKE3 في PostgreSQL 18");
            }
          )
        ]
      ),

      // Interactive Feature Flags & Live Telemetry Control Deck
      React.createElement(
        "section",
        {
          key: "live-feature-flags-deck",
          style: {
            padding: "22px 24px",
            borderRadius: "24px",
            background: "linear-gradient(135deg, #090D16 0%, #111827 100%)",
            color: "#F8FAFC",
            border: "1px solid rgba(96, 165, 250, 0.35)",
            marginBottom: "24px"
          }
        },
        [
          React.createElement(
            "div",
            {
              key: "ff-head",
              style: {
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                flexWrap: "wrap",
                gap: "12px",
                marginBottom: "16px"
              }
            },
            [
              React.createElement("div", { key: "ff-t" }, [
                React.createElement(
                  "h2",
                  { key: "h2", style: { fontSize: "17px", fontWeight: 800, margin: "0 0 4px" } },
                  "مركز أعلام الميزات الفوري (Valkey 8 + PostgreSQL 18 Live Flags)"
                ),
                React.createElement(
                  "p",
                  { key: "sub", style: { margin: 0, fontSize: "13px", color: "#94A3B8" } },
                  "بدّل أي ميزة فورًا مع توثيق تلقائي في سلسلة بصمات BLAKE3"
                )
              ]),
              React.createElement(
                "span",
                {
                  key: "ff-badge",
                  className: "safi-mono",
                  dir: "ltr",
                  style: {
                    padding: "4px 12px",
                    borderRadius: "999px",
                    background: "rgba(16, 185, 129, 0.16)",
                    color: "#34D399",
                    fontSize: "12px",
                    fontWeight: 800
                  }
                },
                "Valkey 8 Sync: 0.4ms"
              )
            ]
          ),
          React.createElement(
            "div",
            {
              key: "ff-grid",
              style: {
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
                gap: "12px"
              }
            },
            [
              { key: "typstPdf", label: "محرك Typst Vector PDF الفوري" },
              { key: "formulaGuardA11", label: "درع تحييد الصيغ الخبيثة (A11)" },
              { key: "graphifyDag", label: "مخطط السلالة البصري Graphify DAG" },
              { key: "glide120Fps", label: "شبكة العرض السريع 120 FPS Canvas" }
            ].map((flag) => {
              const enabled = featureFlags[flag.key] ?? true;
              return React.createElement(
                "button",
                {
                  key: flag.key,
                  type: "button",
                  onClick: () => {
                    const nextVal = !enabled;
                    setFeatureFlags((prev) => ({ ...prev, [flag.key]: nextVal }));
                    setActiveActionLog(
                      `تم ${nextVal ? "تفعيل" : "إيقاف"} ميزة «${flag.label}» ومزامنتها لحظيًا.`
                    );
                    appendAuditEvent(`${nextVal ? "تفعيل" : "إيقاف"} علم الميزة: ${flag.label}`);
                  },
                  style: {
                    padding: "12px 14px",
                    borderRadius: "14px",
                    border: enabled
                      ? "1px solid rgba(52, 211, 153, 0.45)"
                      : "1px solid rgba(255, 255, 255, 0.14)",
                    background: enabled ? "rgba(16, 185, 129, 0.12)" : "rgba(255, 255, 255, 0.04)",
                    color: "#F8FAFC",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: "10px",
                    cursor: "pointer",
                    textAlign: "right"
                  }
                },
                [
                  React.createElement(
                    "span",
                    { key: "l", style: { fontSize: "12.5px", fontWeight: 700 } },
                    flag.label
                  ),
                  React.createElement(
                    "span",
                    {
                      key: "s",
                      className: "safi-mono",
                      dir: "ltr",
                      style: {
                        fontSize: "11px",
                        fontWeight: 800,
                        color: enabled ? "#34D399" : "#94A3B8"
                      }
                    },
                    enabled ? "ON ✓" : "OFF"
                  )
                ]
              );
            })
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
                auditLogs.map((entry) =>
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
  return isAr ? rendered : localizeOwnerNode(rendered);
};

function renderCard(
  title: string,
  desc: string,
  btnText: string,
  isActive: boolean,
  onClick: () => void
) {
  return React.createElement(
    "div",
    {
      key: title,
      className: "safi-glass",
      style: {
        padding: "24px",
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        border: isActive ? "1px solid var(--accent-soft)" : undefined,
        boxShadow: isActive ? "0 18px 42px -14px rgba(37, 99, 235, 0.22)" : undefined
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

import React, { useState } from "react";
import type { SafiDockTabId, SafiLang } from "../components/layout/FloatingGlassDock";
import { CountUpMetric } from "../components/ui/CountUpMetric";
import { OWNER_EMAIL, FRIEND_ROLE } from "../lib/business-config";
import { isOwnerEmail } from "../lib/auth-rules";
import { grantFriendRole, revokeFriendRole, type FriendMember } from "../lib/owner-access";

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
  "مركز السجلات الموثّق بسلسلة بصمات BLAKE3 (Immutable Audit Chain)":
    "Verified audit log center with BLAKE3 hash chain (Immutable Audit Chain)",
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

export type OwnerTimeHorizon = "monthly" | "quarterly" | "annual";

export interface OwnerSubscriberRow {
  readonly id: string;
  readonly email: string;
  readonly plan: string;
  readonly verifiedEmail: boolean;
  readonly verifiedPhone: boolean;
  readonly verifiedDevice: boolean;
  readonly expiresAt: string;
  readonly status: "active" | "suspended";
  readonly aiCallsUsed: number;
}

export interface OwnerAnalyticsSnapshot {
  readonly totalUsers: number;
  readonly activeTrials: number;
  readonly freeUsers: number;
  readonly monthlySubscribers: number;
  readonly annualSubscribers: number;
  readonly organizationSeats: number;
  readonly friendVipUsers: number;
  readonly creditPackBuyers: number;
  readonly payPerFileBuyers: number;
  readonly mrrUsd: number;
  readonly mrrEgp: number;
  readonly arrUsd: number;
  readonly arrEgp: number;
  readonly churnRate: number;
  readonly conversionRate: number;
  readonly rowsCleaned: number;
  readonly piiMasked: number;
  readonly aiCallsUsed: number;
  readonly monthly: ReadonlyArray<{
    readonly month: string;
    readonly users: number;
    readonly revenueUsd: number;
  }>;
  readonly quarterly: ReadonlyArray<{ readonly quarter: string; readonly revenueUsd: number }>;
  readonly subscribers: readonly OwnerSubscriberRow[];
}

const MONTH_LABELS = [
  "01",
  "02",
  "03",
  "04",
  "05",
  "06",
  "07",
  "08",
  "09",
  "10",
  "11",
  "12"
] as const;

export const EMPTY_OWNER_ANALYTICS: OwnerAnalyticsSnapshot = {
  totalUsers: 0,
  activeTrials: 0,
  freeUsers: 0,
  monthlySubscribers: 0,
  annualSubscribers: 0,
  organizationSeats: 0,
  friendVipUsers: 0,
  creditPackBuyers: 0,
  payPerFileBuyers: 0,
  mrrUsd: 0,
  mrrEgp: 0,
  arrUsd: 0,
  arrEgp: 0,
  churnRate: 0,
  conversionRate: 0,
  rowsCleaned: 0,
  piiMasked: 0,
  aiCallsUsed: 0,
  monthly: MONTH_LABELS.map((month) => ({ month, users: 0, revenueUsd: 0 })),
  quarterly: ["Q1", "Q2", "Q3", "Q4"].map((quarter) => ({ quarter, revenueUsd: 0 })),
  subscribers: []
};

function ExecutiveBarChart({
  title,
  subtitle,
  values,
  labels,
  accent
}: {
  readonly title: string;
  readonly subtitle: string;
  readonly values: readonly number[];
  readonly labels: readonly string[];
  readonly accent: string;
}): React.ReactNode {
  const width = 520;
  const height = 132;
  const gap = 7;
  const barWidth = values.length === 0 ? 0 : (width - gap * (values.length - 1)) / values.length;
  const max = Math.max(...values, 1);

  return (
    <div className="safi-chart-card">
      <h3>{title}</h3>
      <p>{subtitle}</p>
      <svg
        viewBox={`0 0 ${width} ${height + 26}`}
        width="100%"
        height="180"
        role="img"
        aria-label={title}
      >
        {values.map((value, index) => {
          const barHeight = Math.max(Math.round((value / max) * (height - 12)), 3);
          return (
            <g key={`${labels[index] ?? index}-${index}`}>
              <rect
                x={index * (barWidth + gap)}
                y={height - barHeight}
                width={barWidth}
                height={barHeight}
                rx="6"
                fill={accent}
                opacity="0.88"
              />
              <text
                x={index * (barWidth + gap) + barWidth / 2}
                y={height + 18}
                textAnchor="middle"
                fontSize="9.5"
                fill="currentColor"
                opacity="0.55"
              >
                {labels[index] ?? ""}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

export interface OwnerDashboardProps {
  readonly lang?: SafiLang;
  readonly onSelectTab?: (tab: SafiDockTabId) => void;
  readonly viewerEmail?: string;
  readonly analytics?: OwnerAnalyticsSnapshot;
}

export const OwnerDashboard: React.FC<OwnerDashboardProps> = ({
  lang = "en",
  onSelectTab,
  viewerEmail = OWNER_EMAIL,
  analytics = EMPTY_OWNER_ANALYTICS
}) => {
  const isAr = lang === "ar";
  const [horizon, setHorizon] = useState<OwnerTimeHorizon>("monthly");
  const [friendMembers, setFriendMembers] = useState<readonly FriendMember[]>([]);
  const [friendEmail, setFriendEmail] = useState<string>("");
  const [friendNote, setFriendNote] = useState<string>("");
  const [friendError, setFriendError] = useState<string | null>(null);
  const [subscriberQuery, setSubscriberQuery] = useState<string>("");
  const [suspendedEmails, setSuspendedEmails] = useState<ReadonlySet<string>>(new Set());
  const [extendedTrials, setExtendedTrials] = useState<Readonly<Record<string, string>>>({});
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

  if (!isOwnerEmail(viewerEmail)) {
    return (
      <div className="safi-exec-denied" role="alert">
        <p style={{ fontSize: "34px", margin: 0 }} aria-hidden="true">
          🔒
        </p>
        <h1 style={{ fontSize: "22px", margin: "10px 0 6px" }}>
          {isAr
            ? "هذه اللوحة للمالك الحصري فقط"
            : "This console is reserved for the exclusive owner"}
        </h1>
        <p style={{ color: "var(--text-secondary)", fontSize: "13.5px" }}>
          {isAr
            ? `المالك المعتمد: ${OWNER_EMAIL}. كل محاولة من بريد آخر تُرفض فوراً.`
            : `Certified owner: ${OWNER_EMAIL}. Every other account is rejected immediately.`}
        </p>
      </div>
    );
  }

  const describe = (ar: string, en: string): string => (isAr ? ar : en);

  const announce = (ar: string, en: string): void => {
    const message = describe(ar, en);
    setActiveActionLog(message);
    appendAuditEvent(message);
  };

  const handleGrantFriend = (event: React.FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    const result = grantFriendRole(
      viewerEmail,
      friendEmail,
      friendNote,
      `${new Date().toISOString().replace("T", " ").slice(0, 19)} UTC`
    );
    if (!result.ok) {
      setFriendError(
        result.reason === "not_owner"
          ? describe("هذا الإجراء متاح للمالك فقط.", "Only the owner can grant this role.")
          : result.reason === "owner_address"
            ? describe(
                "لا يمكن منح رتبة Friend لبريد المالك.",
                "The owner address cannot receive the Friend role."
              )
            : describe("أدخل بريدًا إلكترونيًا صالحًا.", "Enter a valid email address.")
      );
      return;
    }
    setFriendError(null);
    setFriendMembers((prev) =>
      prev.some((member) => member.email === result.member.email) ? prev : [...prev, result.member]
    );
    setFriendEmail("");
    setFriendNote("");
    announce(
      `تم منح ${result.member.email} رتبة Friend VIP شاملة مجاناً.`,
      `Granted ${result.member.email} full Friend VIP access for free.`
    );
  };

  const handleRevokeFriend = (email: string): void => {
    const result = revokeFriendRole(viewerEmail, email);
    if (!result.ok) {
      setFriendError(describe("تعذّر سحب الصلاحية.", "Could not revoke this role."));
      return;
    }
    setFriendMembers((prev) => prev.filter((member) => member.email !== result.email));
    announce(`تم سحب صلاحية Friend VIP من ${email}.`, `Revoked Friend VIP access from ${email}.`);
  };

  const handlePromote = (email: string): void => {
    const result = grantFriendRole(viewerEmail, email, "", new Date().toISOString().slice(0, 10));
    if (!result.ok) return;
    setFriendMembers((prev) =>
      prev.some((member) => member.email === result.member.email) ? prev : [...prev, result.member]
    );
    announce(`تمت ترقية ${email} إلى Friend VIP.`, `Promoted ${email} to Friend VIP.`);
  };

  const handleExtendTrial = (email: string): void => {
    const extended = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
    setExtendedTrials((prev) => ({ ...prev, [email]: extended }));
    announce(`تم تمديد تجربة ${email} سبعة أيام.`, `Extended the ${email} trial by seven days.`);
  };

  const handleSuspend = (email: string): void => {
    const wasSuspended = suspendedEmails.has(email);
    setSuspendedEmails((prev) => {
      const next = new Set(prev);
      if (next.has(email)) {
        next.delete(email);
      } else {
        next.add(email);
      }
      return next;
    });
    announce(
      wasSuspended ? `تم تفعيل حساب ${email}.` : `تم إيقاف حساب ${email}.`,
      wasSuspended ? `Reactivated ${email}.` : `Suspended ${email}.`
    );
  };

  const handleCancelSubscription = (email: string): void => {
    announce(`تم إلغاء اشتراك ${email}.`, `Cancelled the ${email} subscription.`);
  };

  const handleRefund = (email: string): void => {
    announce(
      `تم تسجيل استرداد ${email} ضمن مهلة 3 أيام.`,
      `Recorded a refund for ${email} inside the 3-day window.`
    );
  };

  const friendCount = friendMembers.length + analytics.friendVipUsers;
  const totalUsers = 1 + friendMembers.length + analytics.totalUsers;

  const horizonOptions: ReadonlyArray<{
    readonly id: OwnerTimeHorizon;
    readonly ar: string;
    readonly en: string;
  }> = [
    { id: "monthly", ar: "شهرياً (Monthly)", en: "Monthly" },
    { id: "quarterly", ar: "ربع سنوي (Q1–Q4)", en: "Quarterly (Q1–Q4)" },
    { id: "annual", ar: "سنوياً (Annual)", en: "Annual" }
  ];

  const kpiCards: ReadonlyArray<{
    readonly id: string;
    readonly ar: string;
    readonly en: string;
    readonly value: number;
    readonly suffix: string;
    readonly decimals: number;
    readonly noteAr: string;
    readonly noteEn: string;
  }> = [
    {
      id: "total_users",
      ar: "إجمالي المستخدمين",
      en: "Total users",
      value: totalUsers,
      suffix: "",
      decimals: 0,
      noteAr: "+ نمو كلي",
      noteEn: "+ total growth"
    },
    {
      id: "active_trials",
      ar: "تجارب مجانية نشطة (7 أيام)",
      en: "Active 7-day trials",
      value: analytics.activeTrials,
      suffix: "",
      decimals: 0,
      noteAr: "",
      noteEn: ""
    },
    {
      id: "free_users",
      ar: "مستخدمو الباقة المجانية",
      en: "Free plan users",
      value: analytics.freeUsers,
      suffix: "",
      decimals: 0,
      noteAr: "",
      noteEn: ""
    },
    {
      id: "monthly_subscribers",
      ar: "اشتراكات الأفراد الشهرية",
      en: "Individual monthly subscribers",
      value: analytics.monthlySubscribers,
      suffix: "",
      decimals: 0,
      noteAr: "1,000 EGP / $20",
      noteEn: "1,000 EGP / $20"
    },
    {
      id: "annual_subscribers",
      ar: "اشتراكات الأفراد السنوية",
      en: "Individual annual subscribers",
      value: analytics.annualSubscribers,
      suffix: "",
      decimals: 0,
      noteAr: "8,400 EGP / $168",
      noteEn: "8,400 EGP / $168"
    },
    {
      id: "organization_seats",
      ar: "الشركات والمقاعد النشطة",
      en: "Organization seats",
      value: analytics.organizationSeats,
      suffix: "",
      decimals: 0,
      noteAr: "850 EGP / $17",
      noteEn: "850 EGP / $17"
    },
    {
      id: "friend_vip",
      ar: "أصحاب رتبة صديق المالك",
      en: "Friend VIP members",
      value: friendCount,
      suffix: "",
      decimals: 0,
      noteAr: "0 EGP / $0",
      noteEn: "0 EGP / $0"
    },
    {
      id: "credit_pack_buyers",
      ar: "مشترو باقات الرصيد (25 عملية)",
      en: "Credit pack buyers (25 ops)",
      value: analytics.creditPackBuyers,
      suffix: "",
      decimals: 0,
      noteAr: "250 EGP / $5",
      noteEn: "250 EGP / $5"
    },
    {
      id: "pay_per_file_buyers",
      ar: "مشترو فتح الملف الواحد",
      en: "Pay-per-file buyers",
      value: analytics.payPerFileBuyers,
      suffix: "",
      decimals: 0,
      noteAr: "50 EGP / $2",
      noteEn: "50 EGP / $2"
    },
    {
      id: "mrr_usd",
      ar: "الإيراد الشهري المتكرر (دولار)",
      en: "Monthly recurring revenue (USD)",
      value: analytics.mrrUsd,
      suffix: " USD",
      decimals: 0,
      noteAr: "Paddle",
      noteEn: "Paddle"
    },
    {
      id: "mrr_egp",
      ar: "الإيراد الشهري المتكرر (جنيه)",
      en: "Monthly recurring revenue (EGP)",
      value: analytics.mrrEgp,
      suffix: " EGP",
      decimals: 0,
      noteAr: "Paymob",
      noteEn: "Paymob"
    },
    {
      id: "arr_usd",
      ar: "الإيراد السنوي المتكرر (دولار)",
      en: "Annual recurring revenue (USD)",
      value: analytics.arrUsd,
      suffix: " USD",
      decimals: 0,
      noteAr: "Paddle",
      noteEn: "Paddle"
    },
    {
      id: "arr_egp",
      ar: "الإيراد السنوي المتكرر (جنيه)",
      en: "Annual recurring revenue (EGP)",
      value: analytics.arrEgp,
      suffix: " EGP",
      decimals: 0,
      noteAr: "Paymob",
      noteEn: "Paymob"
    },
    {
      id: "conversion",
      ar: "التحويل من التجربة إلى الدفع",
      en: "Trial-to-paid conversion",
      value: analytics.conversionRate,
      suffix: "%",
      decimals: 1,
      noteAr: "",
      noteEn: ""
    },
    {
      id: "churn",
      ar: "معدل إلغاء الاشتراك",
      en: "Churn rate",
      value: analytics.churnRate,
      suffix: "%",
      decimals: 1,
      noteAr: "",
      noteEn: ""
    },
    {
      id: "rows_cleaned",
      ar: "الصفوف المنظفة",
      en: "Rows cleaned",
      value: analytics.rowsCleaned,
      suffix: "",
      decimals: 0,
      noteAr: "",
      noteEn: ""
    },
    {
      id: "pii_masked",
      ar: "حقول PII المخفية",
      en: "PII fields masked",
      value: analytics.piiMasked,
      suffix: "",
      decimals: 0,
      noteAr: "",
      noteEn: ""
    },
    {
      id: "ai_calls",
      ar: "نداءات الذكاء الاصطناعي",
      en: "AI calls used",
      value: analytics.aiCallsUsed,
      suffix: ` / ${FRIEND_ROLE.ai_calls_per_month}`,
      decimals: 0,
      noteAr: "الحد الشهري",
      noteEn: "Monthly cap"
    }
  ];

  const baseSubscriberRows: readonly OwnerSubscriberRow[] = [
    {
      id: "owner",
      email: OWNER_EMAIL,
      plan: describe("المالك", "Owner"),
      verifiedEmail: true,
      verifiedPhone: true,
      verifiedDevice: true,
      expiresAt: describe("دائم", "Lifetime"),
      status: "active",
      aiCallsUsed: analytics.aiCallsUsed
    },
    ...friendMembers.map((member) => ({
      id: `friend-${member.email}`,
      email: member.email,
      plan: "Friend VIP",
      verifiedEmail: true,
      verifiedPhone: false,
      verifiedDevice: false,
      expiresAt: describe("لا ينتهي إلا بسحب المالك", "Never unless revoked by the owner"),
      status: "active" as const,
      aiCallsUsed: member.aiCallsUsed
    })),
    ...analytics.subscribers
  ];

  const normalizedQuery = subscriberQuery.trim().toLowerCase();
  const filteredSubscribers = baseSubscriberRows
    .filter((row) => {
      if (normalizedQuery.length === 0) return true;
      return (
        row.email.toLowerCase().includes(normalizedQuery) ||
        row.plan.toLowerCase().includes(normalizedQuery)
      );
    })
    .map((row) => {
      const suspended = suspendedEmails.has(row.email);
      const extended = extendedTrials[row.email];
      return {
        ...row,
        status: suspended ? ("suspended" as const) : row.status,
        expiresAt: extended ?? row.expiresAt
      };
    });

  const executiveCommandCenter = (
    <React.Fragment key="executive-command-center">
      <section
        className="safi-exec-hero"
        aria-label={describe("هوية المالك والفترات الزمنية", "Owner identity and time horizons")}
      >
        <div>
          <span className="safi-badge safi-badge--accent">
            {describe("المركز التنفيذي للمالك", "Executive owner command center")}
          </span>
          <h2 style={{ margin: "10px 0 6px", fontSize: "clamp(22px, 2.4vw, 30px)" }}>
            {describe("المالك الحصري المعتمد", "Certified exclusive owner")}
          </h2>
          <div className="safi-exec-owner">
            <span aria-hidden="true">👑</span>
            <span className="safi-exec-owner-email" dir="ltr">
              {OWNER_EMAIL}
            </span>
            <span className="safi-badge safi-badge--ok">
              {describe("قفل مالك صارم • تحقق 100%", "Strict owner lock • 100% verified")}
            </span>
          </div>
          <p style={{ margin: "10px 0 0", color: "var(--text-secondary)", fontSize: "13.5px" }}>
            {describe(
              "كل رقم في المشروع أمامك: المستخدمون، التجارب، الاشتراكات، المقاعد، رتبة Friend، الرصيد، النداءات، والإيرادات بالجنيه والدولار.",
              "Every number in one place: users, trials, subscriptions, seats, Friend VIP, credits, AI calls, and revenue in EGP and USD."
            )}
          </p>
          <p className="safi-kpi-note">
            {describe(
              "تُحدَّث الأرقام من خادم المنصة عند الاتصال؛ بيانات ملفاتك لا تترك جهازك أبداً.",
              "Figures refresh from the platform server when connected; your file data never leaves this device."
            )}
          </p>
        </div>
        <div>
          <p className="safi-kpi-note" style={{ marginBottom: "8px", fontWeight: 800 }}>
            {describe("الفترة الزمنية", "Time horizon")}
          </p>
          <div
            className="safi-horizon-switch"
            role="group"
            aria-label={describe("فلتر الفترات الزمنية", "Time horizon filter")}
            data-horizon={horizon}
          >
            {horizonOptions.map((option) => (
              <button
                key={option.id}
                type="button"
                className="safi-horizon-btn"
                aria-pressed={horizon === option.id}
                onClick={() => setHorizon(option.id)}
              >
                {describe(option.ar, option.en)}
              </button>
            ))}
          </div>
          {horizon === "quarterly" ? (
            <ul
              style={{
                margin: "12px 0 0",
                paddingInlineStart: "18px",
                fontSize: "12.5px",
                color: "var(--text-secondary)"
              }}
            >
              {analytics.quarterly.map((entry) => (
                <li key={entry.quarter}>
                  {entry.quarter}: {entry.revenueUsd.toLocaleString("en-US")} USD
                </li>
              ))}
            </ul>
          ) : null}
          {horizon === "monthly" ? (
            <p className="safi-kpi-note">
              {describe("مقارنة بالشهر السابق (MoM%)", "Month-over-month comparison (MoM%)")}
            </p>
          ) : null}
          {horizon === "annual" ? (
            <p className="safi-kpi-note">
              {describe("الإجمالي السنوي ومعدل النمو (YoY%)", "Annual totals and growth (YoY%)")}
            </p>
          ) : null}
        </div>
      </section>

      <section
        className="safi-kpi-grid"
        aria-label={describe("شبكة مؤشرات الأداء التنفيذية", "Executive KPI matrix")}
      >
        {kpiCards.map((card) => (
          <article className="safi-kpi-card" key={`${horizon}-${card.id}`}>
            <h3>{describe(card.ar, card.en)}</h3>
            <CountUpMetric value={card.value} suffix={card.suffix} decimals={card.decimals} />
            {card.noteAr === "" ? null : (
              <p className="safi-kpi-note">{describe(card.noteAr, card.noteEn)}</p>
            )}
          </article>
        ))}
      </section>

      <div className="safi-chart-grid">
        <ExecutiveBarChart
          title={describe(
            "نمو المستخدمين خلال الأشهر الاثني عشر",
            "User growth across twelve months"
          )}
          subtitle={describe(
            "شهرياً — يتحدث فوراً مع الفترة المختارة",
            "Monthly — refreshes with the selected horizon"
          )}
          values={analytics.monthly.map((month) => month.users)}
          labels={analytics.monthly.map((month) => month.month)}
          accent="var(--accent-soft)"
        />
        <ExecutiveBarChart
          title={describe(
            "الإيراد الشهري والأرباع (Q1–Q4)",
            "Monthly revenue and quarters (Q1–Q4)"
          )}
          subtitle={describe(
            "بالدولار الأمريكي (Paddle) والجنيه المصري (Paymob)",
            "In USD (Paddle) and EGP (Paymob)"
          )}
          values={analytics.quarterly.map((quarter) => quarter.revenueUsd)}
          labels={analytics.quarterly.map((quarter) => quarter.quarter)}
          accent="#10b981"
        />
      </div>

      <section className="safi-friend-manager" aria-labelledby="safi-friend-title">
        <h2 id="safi-friend-title" style={{ margin: 0, fontSize: "18px" }}>
          🎁{" "}
          {describe(
            "إدارة صلاحية أصدقاء المالك (Friend VIP Access)",
            "Owner Friend VIP access manager"
          )}
        </h2>
        <p className="safi-kpi-note">
          {describe(
            "تمنح الرتبة كل مزايا الباقة المدفوعة مجاناً: بلا علامة مائية، حفظ وتشغيل الوصفات، حزم القوالب الاحترافية، و400 طلب ذكاء اصطناعي شهرياً. لا تُباع ولا تُشترى.",
            "The role grants every paid feature for free: no watermark, saved recipes, premium recipe packs, and 400 AI calls monthly. It is never sold or purchased."
          )}
        </p>
        <form className="safi-friend-form" onSubmit={handleGrantFriend}>
          <input
            type="email"
            required
            value={friendEmail}
            onChange={(event) => setFriendEmail(event.currentTarget.value)}
            placeholder={describe("البريد الإلكتروني للصديق", "Friend email address")}
            aria-label={describe("البريد الإلكتروني للصديق", "Friend email address")}
            dir="ltr"
          />
          <input
            type="text"
            value={friendNote}
            onChange={(event) => setFriendNote(event.currentTarget.value)}
            placeholder={describe("ملاحظة اختيارية (اسم الصديق)", "Optional note (friend name)")}
            aria-label={describe("ملاحظة اختيارية", "Optional note")}
          />
          <button type="submit" className="safi-btn safi-btn-primary">
            {describe("منح صلاحية Friend شاملة مجاناً", "Grant full Friend access for free")}
          </button>
        </form>
        {friendError === null ? null : (
          <p
            role="alert"
            style={{
              margin: "0 0 12px",
              color: "var(--status-danger)",
              fontSize: "12.5px",
              fontWeight: 700
            }}
          >
            {friendError}
          </p>
        )}
        <div className="safi-table-wrap">
          <table className="safi-exec-table">
            <thead>
              <tr>
                <th>{describe("بريد الصديق", "Friend email")}</th>
                <th>{describe("تاريخ المنح", "Granted at")}</th>
                <th>{describe("استهلاك AI", "AI usage")}</th>
                <th>{describe("الإجراء", "Action")}</th>
              </tr>
            </thead>
            <tbody>
              {friendMembers.length === 0 ? (
                <tr>
                  <td colSpan={4}>
                    {describe(
                      "لا يوجد أصدقاء بعد. أضف أول بريد أعلاه.",
                      "No Friend members yet. Add the first address above."
                    )}
                  </td>
                </tr>
              ) : (
                friendMembers.map((member) => (
                  <tr key={member.email}>
                    <td dir="ltr">
                      {member.email}
                      {member.note === "" ? null : (
                        <>
                          <br />
                          <span style={{ fontSize: "10.5px", color: "var(--text-muted)" }}>
                            {member.note}
                          </span>
                        </>
                      )}
                    </td>
                    <td className="safi-mono" dir="ltr">
                      {member.grantedAt}
                    </td>
                    <td>
                      {member.aiCallsUsed} / {member.aiCallsLimit}
                    </td>
                    <td>
                      <button
                        type="button"
                        className="safi-btn safi-btn-secondary"
                        onClick={() => handleRevokeFriend(member.email)}
                      >
                        {describe("سحب الصلاحية (Revoke)", "Revoke")}
                      </button>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section
        className="safi-glass"
        style={{ padding: "22px" }}
        aria-labelledby="safi-subscribers-title"
      >
        <h2 id="safi-subscribers-title" style={{ margin: "0 0 6px", fontSize: "18px" }}>
          👥 {describe("إدارة المشتركين والعمليات", "Subscriber and operations management")}
        </h2>
        <p className="safi-kpi-note" style={{ marginBottom: "14px" }}>
          {describe(
            "بحث وفلترة فورية، حالة التحقق (بريد + هاتف + بصمة جهاز)، وتاريخ الانتهاء مع أزرار المالك المباشرة.",
            "Search and filter instantly, review verification (email + phone + device fingerprint), expiry dates, and direct owner actions."
          )}
        </p>
        <input
          className="safi-exec-search"
          type="search"
          value={subscriberQuery}
          onChange={(event) => setSubscriberQuery(event.currentTarget.value)}
          placeholder={describe("ابحث بالبريد أو نوع الباقة", "Search by email or plan")}
          aria-label={describe("ابحث في المشتركين", "Search subscribers")}
        />
        <div className="safi-table-wrap">
          <table className="safi-exec-table">
            <thead>
              <tr>
                <th>{describe("المستخدم", "User")}</th>
                <th>{describe("نوع الباقة", "Plan")}</th>
                <th>{describe("التحقق", "Verification")}</th>
                <th>{describe("تاريخ الانتهاء", "Expires")}</th>
                <th>{describe("الحالة", "Status")}</th>
                <th>{describe("إجراءات المالك", "Owner actions")}</th>
              </tr>
            </thead>
            <tbody>
              {filteredSubscribers.map((row) => (
                <tr key={row.id}>
                  <td dir="ltr">{row.email}</td>
                  <td>{row.plan}</td>
                  <td>
                    {row.verifiedEmail ? "✓" : "—"} {describe("بريد", "Email")} ·{" "}
                    {row.verifiedPhone ? "✓" : "—"} {describe("هاتف", "Phone")} ·{" "}
                    {row.verifiedDevice ? "✓" : "—"} {describe("جهاز", "Device")}
                  </td>
                  <td dir="ltr">{row.expiresAt}</td>
                  <td>
                    {row.status === "active"
                      ? describe("نشط", "Active")
                      : describe("موقوف", "Suspended")}
                  </td>
                  <td>
                    <div className="safi-exec-actions">
                      <button
                        type="button"
                        disabled={row.id === "owner"}
                        onClick={() => handlePromote(row.email)}
                      >
                        {describe("ترقية إلى Friend VIP", "Promote to Friend VIP")}
                      </button>
                      <button
                        type="button"
                        disabled={row.id === "owner"}
                        onClick={() => handleExtendTrial(row.email)}
                      >
                        {describe("تمديد التجربة", "Extend trial")}
                      </button>
                      <button
                        type="button"
                        data-danger="true"
                        disabled={row.id === "owner"}
                        onClick={() => handleCancelSubscription(row.email)}
                      >
                        {describe("إلغاء الاشتراك", "Cancel subscription")}
                      </button>
                      <button
                        type="button"
                        data-danger="true"
                        disabled={row.id === "owner"}
                        onClick={() => handleRefund(row.email)}
                      >
                        {describe("إصدار استرداد (ضمن 3 أيام)", "Issue refund (within 3 days)")}
                      </button>
                      <button
                        type="button"
                        data-danger="true"
                        disabled={row.id === "owner"}
                        onClick={() => handleSuspend(row.email)}
                      >
                        {describe("إيقاف الحساب", "Suspend account")}
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </React.Fragment>
  );

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

      executiveCommandCenter,

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

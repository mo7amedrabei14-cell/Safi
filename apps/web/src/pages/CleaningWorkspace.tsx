/**
 * `apps/web/src/pages/CleaningWorkspace.tsx` — Cleaning Workspace v1 (Grid, Understanding Panel, Steps Panel, Before/After Preview, Undo/Redo).
 */
import React, { useState } from "react";
import { Button, Toast, GlideDataGrid } from "@safi/ui";

export interface ColumnInfo {
  name: string;
  type: string;
  confidence: number;
  alternatives: string[];
  quality: number; // 0-100
}

export interface CleaningStep {
  id: string;
  title: string;
  description: string;
  applied: boolean;
}

export const CleaningWorkspace: React.FC = () => {
  const [columns, setColumns] = useState<ColumnInfo[]>([
    { name: "الاسم", type: "نص (Text)", confidence: 0.98, alternatives: ["اسم كامل"], quality: 95 },
    {
      name: "تاريخ الصرف",
      type: "تاريخ (Date)",
      confidence: 0.85,
      alternatives: ["تاريخ هجري", "نص"],
      quality: 88
    },
    {
      name: "المبلغ",
      type: "مبلغ (Decimal)",
      confidence: 0.92,
      alternatives: ["رقم صحيح"],
      quality: 92
    },
    {
      name: "رقم الهاتف",
      type: "هاتف (Phone)",
      confidence: 0.78,
      alternatives: ["نص"],
      quality: 75
    }
  ]);

  const [steps, setSteps] = useState<CleaningStep[]>([
    {
      id: "s1",
      title: "توحيد الحروف العربية",
      description: "توحيد أشكال الألف والتاء المربوطة",
      applied: true
    },
    {
      id: "s2",
      title: "إزالة الفراغات الزائدة",
      description: "تنظيف المسافات المزدوجة والمخفية",
      applied: true
    },
    {
      id: "s3",
      title: "إزالة التكرار الدقيق",
      description: "حذف الصفوف المتطابقة تمامًا",
      applied: false
    },
    {
      id: "s4",
      title: "توحيد صيغة التواريخ",
      description: "تحويل التواريخ إلى YYYY-MM-DD",
      applied: true
    }
  ]);

  const [data] = useState<string[][]>([
    ["أحمد محمد علي", "2026/01/15", "1,500", "01012345678"],
    ["فاطمة حسن", "15-01-2026", "2,300", "01234567890"],
    ["محمود إبراهيم", "2026.01.16", "4,100", "01123456789"],
    ["أحمد محمد علي", "2026/01/15", "1,500", "01012345678"] // duplicate
  ]);

  const [historyIdx, setHistoryIdx] = useState<number>(1);
  const [toastMsg, setToastMsg] = useState<string | null>("تم تحميل الملف بنجاح وجاهز للتنضيف.");
  const [isUnderstandingOpen, setIsUnderstandingOpen] = useState<boolean>(true);
  const [isStepsOpen, setIsStepsOpen] = useState<boolean>(true);
  const [selectedColIdx, setSelectedColIdx] = useState<number>(0);

  const handleToggleStep = (stepId: string) => {
    setSteps(steps.map((s) => (s.id === stepId ? { ...s, applied: !s.applied } : s)));
    setToastMsg("تم تحديث خطوات التنضيف ومعاينة النتائج فورًا.");
  };

  const handleColTypeChange = (colIdx: number, newType: string) => {
    const updated = [...columns];
    const target = updated[colIdx];
    if (!target) {
      return;
    }
    target.type = newType;
    setColumns(updated);
    setToastMsg(`تم تعديل نوع العمود "${target.name}" إلى ${newType}.`);
  };

  return React.createElement(
    "div",
    {
      style: {
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        direction: "rtl",
        backgroundColor: "#F8FAFC",
        fontFamily: "IBM Plex Sans Arabic, sans-serif",
        color: "#0F172A",
        overflow: "hidden"
      }
    },
    [
      // 1. Top Bar (56px)
      React.createElement(
        "header",
        {
          key: "header",
          style: {
            height: "56px",
            borderBottom: "1px solid #E2E8F0",
            backgroundColor: "#FFFFFF",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "0 24px",
            flexShrink: 0,
            zIndex: 10
          }
        },
        [
          React.createElement(
            "div",
            { key: "left-actions", style: { display: "flex", gap: "12px", alignItems: "center" } },
            [
              React.createElement(
                "h1",
                {
                  key: "logo",
                  style: { fontSize: "18px", fontWeight: 700, color: "#0D9488", margin: 0 }
                },
                "صافي البيانات — مساحة التنضيف"
              ),
              React.createElement(
                Button,
                {
                  key: "undo",
                  variant: "outline",
                  style: { minHeight: "36px", padding: "6px 12px", fontSize: "13px" },
                  onClick: () => {
                    if (historyIdx > 0) setHistoryIdx(historyIdx - 1);
                    setToastMsg("تم التراجع عن الخطوة الأخيرة.");
                  }
                },
                "↶ تراجع"
              ),
              React.createElement(
                Button,
                {
                  key: "redo",
                  variant: "outline",
                  style: { minHeight: "36px", padding: "6px 12px", fontSize: "13px" },
                  onClick: () => {
                    setHistoryIdx(historyIdx + 1);
                    setToastMsg("تمت إعادة الخطوة.");
                  }
                },
                "↷ إعادة"
              )
            ]
          ),
          React.createElement(
            "div",
            { key: "right-actions", style: { display: "flex", gap: "12px", alignItems: "center" } },
            [
              React.createElement(
                Button,
                {
                  key: "recipe",
                  variant: "secondary",
                  style: { minHeight: "36px", padding: "6px 12px", fontSize: "13px" },
                  onClick: () => setToastMsg("تم حفظ الوصفة بنجاح في متصفحك.")
                },
                "💾 حفظ كوصفة"
              ),
              React.createElement(
                Button,
                {
                  key: "export",
                  variant: "primary",
                  style: { minHeight: "36px", padding: "6px 16px", fontSize: "13px" },
                  onClick: () => setToastMsg("بدء تصدير الملف النظيف atomically وتوليد التقرير.")
                },
                "📥 تصدير الملف والتقرير"
              )
            ]
          )
        ]
      ),

      // Toast Notification area
      toastMsg &&
        React.createElement(
          "div",
          {
            key: "toast-container",
            style: { position: "fixed", top: "70px", left: "24px", zIndex: 1000 }
          },
          [
            React.createElement(Toast, {
              key: "toast-item",
              message: toastMsg,
              onClose: () => setToastMsg(null)
            })
          ]
        ),

      // 2. Main Workspace Layout (Sidebar Left, Grid Center, Sidebar Right)
      React.createElement(
        "div",
        {
          key: "body",
          style: { display: "flex", flex: 1, overflow: "hidden" }
        },
        [
          // Left Sidebar: Understanding Panel (320px)
          React.createElement(
            "aside",
            {
              key: "understanding-sidebar",
              style: {
                width: isUnderstandingOpen ? "320px" : "48px",
                backgroundColor: "#FFFFFF",
                borderLeft: "1px solid #E2E8F0",
                display: "flex",
                flexDirection: "column",
                transition: "width 0.3s ease",
                overflow: "hidden",
                flexShrink: 0
              }
            },
            [
              React.createElement(
                "div",
                {
                  key: "uh",
                  style: {
                    padding: "16px",
                    borderBottom: "1px solid #E2E8F0",
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center"
                  }
                },
                [
                  isUnderstandingOpen &&
                    React.createElement(
                      "h3",
                      { key: "title", style: { margin: 0, fontSize: "15px", fontWeight: 600 } },
                      "لوحة الفهم (الأعمدة)"
                    ),
                  React.createElement(
                    Button,
                    {
                      key: "toggle",
                      variant: "ghost",
                      style: { minWidth: "32px", minHeight: "32px", padding: 0 },
                      onClick: () => setIsUnderstandingOpen(!isUnderstandingOpen)
                    },
                    isUnderstandingOpen ? "◀" : "▶"
                  )
                ]
              ),
              isUnderstandingOpen &&
                React.createElement(
                  "div",
                  {
                    key: "uc",
                    style: {
                      padding: "16px",
                      overflowY: "auto",
                      flex: 1,
                      display: "flex",
                      flexDirection: "column",
                      gap: "16px"
                    }
                  },
                  columns.map((col, idx) =>
                    React.createElement(
                      "div",
                      {
                        key: idx,
                        style: {
                          padding: "12px",
                          borderRadius: "8px",
                          border:
                            selectedColIdx === idx ? "2px solid #0D9488" : "1px solid #E2E8F0",
                          backgroundColor: selectedColIdx === idx ? "#F0FDFA" : "#FAFAFA",
                          cursor: "pointer"
                        },
                        onClick: () => setSelectedColIdx(idx)
                      },
                      [
                        React.createElement(
                          "div",
                          {
                            key: "top",
                            style: {
                              display: "flex",
                              justifyContent: "space-between",
                              alignItems: "center",
                              marginBottom: "8px"
                            }
                          },
                          [
                            React.createElement(
                              "span",
                              { key: "name", style: { fontWeight: 600, fontSize: "14px" } },
                              col.name
                            ),
                            React.createElement(
                              "span",
                              {
                                key: "conf",
                                style: {
                                  fontSize: "12px",
                                  color: col.confidence > 0.8 ? "#15803D" : "#B45309"
                                }
                              },
                              `${Math.round(col.confidence * 100)}% ثقة`
                            )
                          ]
                        ),
                        React.createElement(
                          "div",
                          {
                            key: "type-row",
                            style: { fontSize: "13px", color: "#475569", marginBottom: "8px" }
                          },
                          `النوع: ${col.type}`
                        ),
                        React.createElement(
                          "div",
                          {
                            key: "actions",
                            style: { display: "flex", gap: "8px", alignItems: "center" }
                          },
                          [
                            React.createElement(
                              Button,
                              {
                                key: "ok",
                                variant: "secondary",
                                style: {
                                  minHeight: "28px",
                                  padding: "2px 8px",
                                  fontSize: "11px",
                                  backgroundColor: "#DCFCE7",
                                  color: "#166534"
                                },
                                onClick: (e) => {
                                  e.stopPropagation();
                                  setToastMsg(`تم تأكيد فهم العمود "${col.name}" بنجاح.`);
                                }
                              },
                              "✓ صحيح"
                            ),
                            React.createElement(
                              Button,
                              {
                                key: "bad",
                                variant: "secondary",
                                style: {
                                  minHeight: "28px",
                                  padding: "2px 8px",
                                  fontSize: "11px",
                                  backgroundColor: "#FEE2E2",
                                  color: "#991B1B"
                                },
                                onClick: (e) => {
                                  e.stopPropagation();
                                  const alt = col.alternatives[0] || "نص";
                                  handleColTypeChange(idx, alt);
                                }
                              },
                              `✗ تبديل (${col.alternatives[0] || "آخر"})`
                            )
                          ]
                        )
                      ]
                    )
                  )
                )
            ]
          ),

          // Center: Data Grid & Bottom Stats Bar
          React.createElement(
            "main",
            {
              key: "center-main",
              style: {
                flex: 1,
                display: "flex",
                flexDirection: "column",
                overflow: "hidden",
                padding: "16px"
              }
            },
            [
              React.createElement(
                "div",
                {
                  key: "grid-container",
                  style: {
                    flex: 1,
                    overflow: "auto",
                    backgroundColor: "#FFFFFF",
                    borderRadius: "12px",
                    border: "1px solid #E2E8F0",
                    padding: "16px"
                  }
                },
                [
                  React.createElement(GlideDataGrid, {
                    key: "glide-grid",
                    columns: columns.map((c) => `${c.name} (${c.type.split(" ")[0]})`),
                    data: data,
                    diffMask: [
                      [false, false, false, false],
                      [false, true, false, false],
                      [false, false, false, false],
                      [true, true, true, true] // duplicate row marked
                    ],
                    density: "comfortable"
                  })
                ]
              ),
              // Bottom Status Bar
              React.createElement(
                "footer",
                {
                  key: "statusbar",
                  style: {
                    marginTop: "12px",
                    height: "40px",
                    backgroundColor: "#FFFFFF",
                    borderRadius: "8px",
                    border: "1px solid #E2E8F0",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "0 16px",
                    fontSize: "13px",
                    color: "#475569"
                  }
                },
                [
                  React.createElement(
                    "div",
                    { key: "stats" },
                    "📊 عدد الصفوف: 4 | الأعمدة: 4 | درجة الجودة الشاملة: 88%"
                  ),
                  React.createElement(
                    "div",
                    { key: "shortcuts" },
                    "💡 نصيحة: انقر على أي عمود في لوحة الفهم الجانبية للتحقق من نوعه والبدائل المقترحة."
                  )
                ]
              )
            ]
          ),

          // Right Sidebar: Steps & Preview Panel (320px)
          React.createElement(
            "aside",
            {
              key: "steps-sidebar",
              style: {
                width: isStepsOpen ? "320px" : "48px",
                backgroundColor: "#FFFFFF",
                borderRight: "1px solid #E2E8F0",
                display: "flex",
                flexDirection: "column",
                transition: "width 0.3s ease",
                overflow: "hidden",
                flexShrink: 0
              }
            },
            [
              React.createElement(
                "div",
                {
                  key: "sh",
                  style: {
                    padding: "16px",
                    borderBottom: "1px solid #E2E8F0",
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center"
                  }
                },
                [
                  React.createElement(
                    Button,
                    {
                      key: "toggle2",
                      variant: "ghost",
                      style: { minWidth: "32px", minHeight: "32px", padding: 0 },
                      onClick: () => setIsStepsOpen(!isStepsOpen)
                    },
                    isStepsOpen ? "▶" : "◀"
                  ),
                  isStepsOpen &&
                    React.createElement(
                      "h3",
                      { key: "title", style: { margin: 0, fontSize: "15px", fontWeight: 600 } },
                      "خطوات التنضيف والمعاينة"
                    )
                ]
              ),
              isStepsOpen &&
                React.createElement(
                  "div",
                  {
                    key: "sc",
                    style: {
                      padding: "16px",
                      overflowY: "auto",
                      flex: 1,
                      display: "flex",
                      flexDirection: "column",
                      gap: "12px"
                    }
                  },
                  [
                    React.createElement(
                      "div",
                      {
                        key: "desc",
                        style: { fontSize: "13px", color: "#475569", marginBottom: "8px" }
                      },
                      "قم بقبول أو رفض خطوات التنضيف المقترحة ورؤية المعاينة قبل وبعد فورًا:"
                    ),
                    ...steps.map((st) =>
                      React.createElement(
                        "div",
                        {
                          key: st.id,
                          style: {
                            padding: "12px",
                            borderRadius: "8px",
                            border: "1px solid #E2E8F0",
                            backgroundColor: st.applied ? "#F0FDFA" : "#FAFAFA"
                          }
                        },
                        [
                          React.createElement(
                            "div",
                            {
                              key: "st-top",
                              style: {
                                display: "flex",
                                justifyContent: "space-between",
                                alignItems: "center",
                                marginBottom: "4px"
                              }
                            },
                            [
                              React.createElement(
                                "span",
                                { key: "st-name", style: { fontWeight: 600, fontSize: "13px" } },
                                st.title
                              ),
                              React.createElement("input", {
                                key: "chk",
                                type: "checkbox",
                                checked: st.applied,
                                onChange: () => handleToggleStep(st.id),
                                style: { width: "18px", height: "18px", cursor: "pointer" },
                                "aria-label": st.title
                              })
                            ]
                          ),
                          React.createElement(
                            "p",
                            {
                              key: "st-desc",
                              style: { margin: 0, fontSize: "12px", color: "#64748B" }
                            },
                            st.description
                          )
                        ]
                      )
                    ),
                    React.createElement(
                      "div",
                      {
                        key: "preview-box",
                        style: {
                          marginTop: "16px",
                          padding: "12px",
                          backgroundColor: "#F8FAFC",
                          borderRadius: "8px",
                          border: "1px dashed #CBD5E1"
                        }
                      },
                      [
                        React.createElement(
                          "h4",
                          {
                            key: "ph",
                            style: {
                              margin: "0 0 8px 0",
                              fontSize: "13px",
                              fontWeight: 600,
                              color: "#0F172A"
                            }
                          },
                          "👁️ معاينة قبل/بعد"
                        ),
                        React.createElement(
                          "p",
                          { key: "pt", style: { margin: 0, fontSize: "12px", color: "#475569" } },
                          "تطبيق التعديلات يزيل الأخطاء ويطابق الصيغ المعيارية بايتًا ببايت مع الاحتفاظ بالملف الأصلي آمنًا."
                        )
                      ]
                    )
                  ]
                )
            ]
          )
        ]
      )
    ]
  );
};

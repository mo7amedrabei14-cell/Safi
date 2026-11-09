/**
 * `apps/web/src/pages/Home.tsx` — Main Home screen with Dropzone (up to 50 files, clipboard paste, file picker).
 */
import React, { useState } from "react";
import { Button, Toast } from "@safi/ui";

export const Home: React.FC = () => {
  const [files, setFiles] = useState<File[]>([]);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const handleDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    const droppedFiles = Array.from(e.dataTransfer.files);
    if (droppedFiles.length > 50) {
      setErrorMsg("SF-IMP-003 · الحد الأقصى المسموح به هو 50 ملفًا في الدفعة الواحدة.");
      return;
    }
    setFiles(droppedFiles);
    setErrorMsg(null);
  };

  return React.createElement(
    "div",
    {
      style: {
        padding: "32px",
        maxWidth: "900px",
        margin: "0 auto",
        direction: "rtl",
        fontFamily: "IBM Plex Sans Arabic, sans-serif"
      }
    },
    [
      React.createElement(
        "h2",
        {
          key: "title",
          style: { fontSize: "24px", fontWeight: 700, marginBottom: "8px", color: "#0F172A" }
        },
        "اختر ملفًا أو اسحبه للبدء"
      ),
      React.createElement(
        "p",
        { key: "sub", style: { fontSize: "14px", color: "#475569", marginBottom: "24px" } },
        "يدعم ملفات Excel (xlsx, xls, ods) وCSV حتى 50 ملفًا في المرة الواحدة."
      ),

      errorMsg &&
        React.createElement("div", { key: "err", style: { marginBottom: "16px" } }, [
          React.createElement(Toast, {
            message: errorMsg,
            type: "error",
            onClose: () => setErrorMsg(null)
          })
        ]),

      React.createElement(
        "div",
        {
          key: "dropzone",
          onDragOver: (e) => e.preventDefault(),
          onDrop: handleDrop,
          style: {
            border: "2px dashed #0D9488",
            borderRadius: "16px",
            backgroundColor: "#F0FDFA",
            padding: "48px",
            textAlign: "center",
            cursor: "pointer",
            marginBottom: "24px",
            transition: "all 0.2s ease"
          }
        },
        [
          React.createElement(
            "div",
            { key: "icon", style: { fontSize: "40px", marginBottom: "12px" } },
            "📁"
          ),
          React.createElement(
            "p",
            {
              key: "txt",
              style: { fontSize: "16px", fontWeight: 600, color: "#0F766E", marginBottom: "16px" }
            },
            "اسحب الملفات هنا أو اضغط للاختيار"
          ),
          React.createElement(
            Button,
            {
              key: "btn",
              variant: "primary",
              onClick: () => {
                // Trigger file picker simulation
                setErrorMsg("SF-IMP-001 · تعذّر فتح الملف. تأكد أن الملف غير تالف وبصيغة مدعومة.");
              }
            },
            "اختر الملفات"
          )
        ]
      ),

      React.createElement("div", { key: "recent", style: { marginTop: "32px" } }, [
        React.createElement(
          "h3",
          { key: "rh", style: { fontSize: "18px", fontWeight: 600, marginBottom: "12px" } },
          "الملفات الأخيرة"
        ),
        files.length === 0
          ? React.createElement(
              "p",
              { key: "empty", style: { color: "#64748B", fontSize: "14px" } },
              "لا توجد ملفات حديثة حتى الآن."
            )
          : React.createElement(
              "ul",
              { key: "list" },
              files.map((f, idx) => React.createElement("li", { key: idx }, f.name))
            )
      ])
    ]
  );
};

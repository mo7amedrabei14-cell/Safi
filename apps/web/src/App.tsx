import React from "react";

export const App: React.FC = () => {
  return React.createElement(
    "div",
    {
      style: {
        fontFamily: "IBM Plex Sans Arabic, sans-serif",
        direction: "rtl",
        padding: "40px",
        backgroundColor: "#F5F7FA",
        minHeight: "100vh",
        color: "#0F172A"
      }
    },
    [
      React.createElement(
        "header",
        {
          key: "header",
          style: {
            position: "fixed",
            top: 0,
            right: 0,
            left: 0,
            height: "56px",
            backgroundColor: "rgba(255, 255, 255, 0.72)",
            backdropFilter: "blur(20px)",
            borderBottom: "1px solid #E2E8F0",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            paddingInline: "24px",
            zIndex: 100,
            scrollPaddingTop: "70px"
          }
        },
        [
          React.createElement(
            "h1",
            {
              key: "logo",
              style: { margin: 0, fontSize: "18px", fontWeight: 700, color: "#0B7C8C" }
            },
            "صافي البيانات | SafiData"
          ),
          React.createElement(
            "nav",
            {
              key: "nav",
              style: { display: "flex", gap: "16px", fontSize: "14px", fontWeight: 500 }
            },
            [
              React.createElement(
                "a",
                { key: "home", href: "#", style: { color: "inherit", textDecoration: "none" } },
                "الرئيسية"
              ),
              React.createElement(
                "a",
                { key: "lib", href: "#", style: { color: "inherit", textDecoration: "none" } },
                "المكتبة"
              ),
              React.createElement(
                "a",
                { key: "settings", href: "#", style: { color: "inherit", textDecoration: "none" } },
                "الإعدادات"
              )
            ]
          )
        ]
      ),
      React.createElement("main", { key: "main", style: { marginTop: "80px" } }, [
        React.createElement(
          "h2",
          { key: "h2", style: { fontSize: "24px", fontWeight: 700, marginBottom: "12px" } },
          "مرحبًا بك في منصة صافي البيانات"
        ),
        React.createElement(
          "p",
          { key: "p", style: { fontSize: "16px", color: "#475569" } },
          "الشريحة الأولى (S1) — الأساس والتنضيف والتقارير."
        )
      ])
    ]
  );
};

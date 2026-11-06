import React from "react";

export interface ErrorBoundaryProps {
  children: React.ReactNode;
}

export interface ErrorBoundaryState {
  hasError: boolean;
  error?: Error;
}

export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  override componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error("Uncaught error captured by Safi ErrorBoundary (SF-UNK-001):", error, errorInfo);
  }

  override render() {
    if (this.state.hasError) {
      return React.createElement(
        "div",
        {
          role: "alert",
          style: {
            padding: "40px",
            textAlign: "center",
            fontFamily: "IBM Plex Sans Arabic, sans-serif",
            direction: "rtl",
            backgroundColor: "#FEF2F2",
            color: "#991B1B",
            minHeight: "100vh",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center"
          }
        },
        [
          React.createElement(
            "h2",
            { key: "h", style: { fontSize: "22px", fontWeight: 700, marginBottom: "12px" } },
            "عذرًا، حدث خطأ غير متوقع في واجهة التطبيق"
          ),
          React.createElement(
            "p",
            {
              key: "code",
              style: {
                fontFamily: "monospace",
                fontSize: "16px",
                fontWeight: 600,
                marginBottom: "16px"
              }
            },
            "رمز التشخيص: SF-UNK-001"
          ),
          React.createElement(
            "p",
            {
              key: "msg",
              style: { fontSize: "14px", maxWidth: "400px", marginBottom: "24px", color: "#B91C1C" }
            },
            "يرجى إعادة تحميل الصفحة أو التواصل مع الدعم الفني مع ذكر رمز الخطأ أعلاه."
          ),
          React.createElement(
            "button",
            {
              key: "btn",
              onClick: () => window.location.reload(),
              style: {
                backgroundColor: "#B91C1C",
                color: "#FFFFFF",
                border: "none",
                padding: "10px 20px",
                borderRadius: "8px",
                fontWeight: 600,
                cursor: "pointer",
                minHeight: "44px"
              }
            },
            "إعادة تحميل الصفحة"
          )
        ]
      );
    }

    return this.props.children;
  }
}

/**
 * `packages/ui/src/react-bits/index.ts` — React Bits components, Savings Counter (`I13`),
 * and Accessible Sortable List with click-to-move alternatives (WCAG 2.2 2.5.7).
 */
import React, { useState } from "react";
import { Button } from "../index";

export interface SavingsCounterProps {
  hoursSaved: number;
  filesCleaned: number;
}

export const SavingsCounter: React.FC<SavingsCounterProps> = ({ hoursSaved, filesCleaned }) => {
  return React.createElement(
    "div",
    {
      style: {
        backgroundColor: "#F0FDFA",
        border: "1px solid #0D9488",
        borderRadius: "12px",
        padding: "20px",
        textAlign: "center",
        direction: "rtl",
        fontFamily: "inherit"
      }
    },
    [
      React.createElement(
        "h3",
        { key: "h", style: { margin: "0 0 8px 0", color: "#0F766E", fontSize: "18px" } },
        "عدّاد التوفير — صافي (I13)"
      ),
      React.createElement(
        "div",
        {
          key: "stats",
          style: {
            display: "flex",
            justifyContent: "center",
            gap: "32px",
            fontSize: "15px",
            color: "#134E4A"
          }
        },
        [
          React.createElement("div", { key: "h_saved" }, [
            React.createElement(
              "strong",
              { key: "h_val", style: { fontSize: "24px", display: "block" } },
              `${hoursSaved} ساعة`
            ),
            "تم توفيرها في تنظيف البيانات"
          ]),
          React.createElement("div", { key: "f_cleaned" }, [
            React.createElement(
              "strong",
              { key: "f_val", style: { fontSize: "24px", display: "block" } },
              `${filesCleaned} ملف`
            ),
            "تم تنظيفها بدقة تامة"
          ])
        ]
      )
    ]
  );
};

export interface AccessibleSortableListProps {
  items: string[];
  onReorder: (newItems: string[]) => void;
}

export const AccessibleSortableList: React.FC<AccessibleSortableListProps> = ({
  items,
  onReorder
}) => {
  const [list, setList] = useState(items);

  const moveItem = (index: number, direction: "up" | "down") => {
    const targetIdx = direction === "up" ? index - 1 : index + 1;
    if (targetIdx < 0 || targetIdx >= list.length) return;
    const updated = [...list];
    const moved = updated[index];
    const displaced = updated[targetIdx];
    if (moved === undefined || displaced === undefined) return;
    updated[index] = displaced;
    updated[targetIdx] = moved;
    setList(updated);
    onReorder(updated);
  };

  return React.createElement(
    "ul",
    {
      style: { listStyle: "none", padding: 0, margin: 0, direction: "rtl" }
    },
    list.map((item, idx) =>
      React.createElement(
        "li",
        {
          key: item,
          style: {
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "12px 16px",
            marginBottom: "8px",
            backgroundColor: "#F8FAFC",
            border: "1px solid #E2E8F0",
            borderRadius: "8px"
          }
        },
        [
          React.createElement(
            "span",
            { key: "txt", style: { fontWeight: 500 } },
            `${idx + 1}. ${item}`
          ),
          React.createElement("div", { key: "actions", style: { display: "flex", gap: "8px" } }, [
            React.createElement(
              Button,
              {
                key: "up",
                variant: "outline",
                style: { minHeight: "36px", minWidth: "36px", padding: "4px 8px" },
                disabled: idx === 0,
                onClick: () => moveItem(idx, "up"),
                "aria-label": `انقل ${item} لأعلى`
              },
              "↑"
            ),
            React.createElement(
              Button,
              {
                key: "down",
                variant: "outline",
                style: { minHeight: "36px", minWidth: "36px", padding: "4px 8px" },
                disabled: idx === list.length - 1,
                onClick: () => moveItem(idx, "down"),
                "aria-label": `انقل ${item} لأسفل`
              },
              "↓"
            )
          ])
        ]
      )
    )
  );
};

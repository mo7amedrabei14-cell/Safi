/**
 * Storybook stories for `packages/ui` components (Button, Dialog, Tooltip, Progress, Toast, GlideDataGrid, SavingsCounter).
 */
import React from "react";
import { Button, Progress, Toast, GlideDataGrid } from "./index";
import { SavingsCounter, AccessibleSortableList } from "./react-bits/index";

export default {
  title: "SafiData/UI Components"
};

export const PrimaryButton = () =>
  React.createElement(Button, { variant: "primary" }, "تنظيف البيانات");
export const SecondaryButton = () => React.createElement(Button, { variant: "secondary" }, "إلغاء");
export const DangerButton = () => React.createElement(Button, { variant: "danger" }, "حذف الجدول");

export const LinearProgress = () =>
  React.createElement(Progress, { value: 75, label: "تقدم التنظيف" });
export const CircularProgress = () =>
  React.createElement(Progress, { value: 90, label: "جودة البيانات", variant: "circular" });

export const SuccessToast = () =>
  React.createElement(Toast, { message: "تم تصدير الملف بنجاح وبدون أخطاء", type: "success" });

export const DataGridDemo = () =>
  React.createElement(GlideDataGrid, {
    columns: ["المعرف", "الاسم", "المبلغ"],
    data: [
      ["001", "أحمد محمد", "150.00"],
      ["002", "فاطمة علي", "200.00"]
    ],
    diffMask: [
      [false, true, false],
      [false, false, false]
    ],
    density: "comfortable"
  });

export const SavingsCounterDemo = () =>
  React.createElement(SavingsCounter, { hoursSaved: 142, filesCleaned: 1250 });

export const SortableListDemo = () =>
  React.createElement(AccessibleSortableList, {
    items: ["توحيد الهمزات والألفات", "إزالة الصفوف المكررة", "ملء الفراغات التلقائي"],
    onReorder: () => {}
  });

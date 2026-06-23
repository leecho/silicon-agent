import type { ReactNode } from "react";
import { joinClasses } from "./utils";

export function BubbleConfirm({
  cancelText = "取消",
  className,
  confirmText = "确认",
  description,
  onCancel,
  onConfirm,
  title = "需要确认",
  tone = "danger",
}: {
  cancelText?: string;
  className?: string;
  confirmText?: string;
  description?: ReactNode;
  onCancel: () => void;
  onConfirm: () => void;
  title?: ReactNode;
  tone?: "danger" | "primary";
}) {
  const confirmClass =
    tone === "danger"
      ? "bg-destructive text-destructive-foreground hover:opacity-90"
      : "bg-primary text-primary-foreground hover:brightness-110";

  return (
    <div className={joinClasses("absolute right-2 top-9 z-20 w-56 rounded-lg border border-border bg-popover p-3 text-left shadow-lg", className)}>
      <div className="absolute -top-1.5 right-4 h-3 w-3 rotate-45 border-l border-t border-border bg-popover" />
      <p className="text-[13px] font-medium text-foreground">{title}</p>
      {description && (
        <p className="mt-1 line-clamp-2 text-[11px] leading-4 text-foreground-muted">
          {description}
        </p>
      )}
      <div className="mt-3 flex justify-end gap-2">
        <button type="button" onClick={onCancel} className="rounded-md px-2 py-1 text-[13px] text-foreground-secondary hover:bg-accent hover:text-foreground">
          {cancelText}
        </button>
        <button type="button" onClick={onConfirm} className={joinClasses("rounded-md px-2 py-1 text-[13px]", confirmClass)}>
          {confirmText}
        </button>
      </div>
    </div>
  );
}

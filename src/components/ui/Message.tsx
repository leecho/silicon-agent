import type { ReactNode } from "react";
import { joinClasses } from "./utils";

type MessageTone = "neutral" | "info" | "success" | "warning" | "danger";

export function Message({
  children,
  className,
  title,
  tone = "neutral"
}: {
  children: ReactNode;
  className?: string;
  title?: string;
  tone?: MessageTone;
}) {
  const toneClass =
    tone === "danger"
      ? "border-danger-border bg-danger-subtle text-danger"
      : tone === "warning"
        ? "border-warning-border bg-warning-subtle text-warning"
        : tone === "success"
          ? "border-success-border bg-success-subtle text-success"
          : tone === "info"
            ? "border-border bg-muted text-primary"
            : "border-border bg-card text-foreground-secondary";

  return (
    <div className={joinClasses("rounded-lg border px-3 py-2 text-sm leading-6", toneClass, className)}>
      {title && <strong className="mb-0.5 block font-semibold text-foreground">{title}</strong>}
      {children}
    </div>
  );
}

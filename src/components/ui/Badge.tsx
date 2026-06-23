import type { ReactNode } from "react";
import { joinClasses } from "./utils";

type BadgeTone = "neutral" | "success" | "warning" | "danger" | "info" | "running";

export function Badge({
  children,
  className,
  tone = "neutral"
}: {
  children: ReactNode;
  className?: string;
  tone?: BadgeTone;
}) {
  const toneClass =
    tone === "success"
      ? "border border-success-border bg-success-subtle text-success"
      : tone === "warning"
        ? "border border-warning-border bg-warning-subtle text-warning"
        : tone === "danger"
          ? "border border-danger-border bg-danger-subtle text-danger"
          : tone === "info"
            ? "bg-muted text-primary"
            : tone === "running"
              ? "bg-muted text-foreground"
              : "bg-muted text-foreground-muted";

  return (
    <span className={joinClasses("rounded-full px-2 py-0.5 text-[11px] font-medium", toneClass, className)}>
      {children}
    </span>
  );
}

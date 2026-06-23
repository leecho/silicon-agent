import type { ButtonHTMLAttributes, ReactNode } from "react";
import { joinClasses } from "./utils";

export function Button({
  children,
  className,
  tone = "outline",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  children: ReactNode;
  tone?: "primary" | "secondary" | "ghost" | "danger" | "outline";
}) {
  const toneClass =
    tone === "primary"
      ? "bg-primary text-primary-foreground hover:brightness-110"
      : tone === "danger"
        ? "bg-danger text-primary-foreground hover:brightness-110"
        : tone === "ghost"
          ? "text-foreground-secondary hover:bg-accent hover:text-accent-foreground"
          : tone === "outline"
            ? "border border-border bg-transparent text-foreground-secondary hover:bg-accent hover:text-accent-foreground"
            : "border border-border bg-surface text-secondary-foreground hover:bg-accent hover:text-accent-foreground";

  return (
    <button
      className={joinClasses(
        "inline-flex items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-semibold transition disabled:cursor-not-allowed disabled:opacity-50",
        toneClass,
        className
      )}
      type="button"
      {...props}
    >
      {children}
    </button>
  );
}

import type { ReactNode } from "react";
import { joinClasses } from "./utils";

export function ButtonGroup({
  align = "end",
  children,
  className
}: {
  align?: "start" | "center" | "end" | "between";
  children: ReactNode;
  className?: string;
}) {
  const alignClass = align === "start" ? "justify-start" : align === "center" ? "justify-center" : align === "between" ? "justify-between" : "justify-end";
  return <div className={joinClasses("flex flex-wrap items-center gap-2", alignClass, className)}>{children}</div>;
}

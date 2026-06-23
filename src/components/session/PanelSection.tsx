import { ChevronRight } from "lucide-react";
import { useState, type ReactNode } from "react";

/**
 * 任务监控侧栏统一的折叠分区：标题（含可选计数/右侧操作）+ 右侧折叠箭头，点击标题行展开/折叠。
 * 样式对齐「产物」模块，供 任务 / 专家 / 产物 等分区共用。
 */
export function PanelSection({
  title,
  count,
  right,
  defaultOpen = true,
  children,
}: {
  title: string;
  count?: number;
  right?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="shrink-0 flex flex-col gap-3">
      <div
        className="flex cursor-pointer items-center justify-between gap-2"
        onClick={() => setOpen((v) => !v)}
      >
        <div className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
          {title}
          {count !== undefined && (
            <span className="text-xs font-normal text-foreground-muted">{count}</span>
          )}
        </div>
        <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
          {right}
          <ChevronRight
            className={`h-3.5 w-3.5 shrink-0 cursor-pointer text-foreground-muted transition ${open ? "rotate-90" : ""}`}
            aria-hidden="true"
            onClick={() => setOpen((v) => !v)}
          />
        </div>
      </div>
      {open && children}
    </div>
  );
}

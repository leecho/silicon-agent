import { useState } from "react";
import type { LucideIcon } from "lucide-react";
import { ChevronRight, Circle, CircleCheck, CircleChevronRight } from "lucide-react";
import type { TodoItem } from "../../types";
import { Tooltip } from "../ui";

function todoPresentation(status: string): {
  icon: LucideIcon;
  iconClassName: string;
  textClassName: string;
} {
  if (status === "completed") {
    return {
      icon: CircleCheck,
      iconClassName: "text-success",
      textClassName: "text-foreground-muted line-through",
    };
  }
  if (status === "in_progress") {
    return {
      icon: CircleChevronRight,
      iconClassName: "text-primary",
      textClassName: "text-primary",
    };
  }
  return {
    icon: Circle,
    iconClassName: "text-foreground",
    textClassName: "text-foreground",
  };
}

// 待办清单面板：固定显示在 feed 上方，随 agent 推进实时更新。
// 空列表显示占位，保持任务监控区域结构稳定。
export function TodoPanel({ todos }: { todos: TodoItem[] }) {
  const [open, setOpen] = useState(true);
  const total = todos.length;
  const completed = todos.filter((t) => t.status === "completed").length;

  return (
    <div className="shrink-0 flex flex-col gap-3">
      <div className="flex flex-col gap-2">
        <div
          className="flex cursor-pointer items-center justify-between gap-1.5"
          onClick={() => setOpen((v) => !v)}
        >
          <div className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
            待办{" "}
            <span className="text-xs text-foreground-muted">
              ({completed}/{total})
            </span>
          </div>
          <span className="text-xs text-foreground-muted">
            <ChevronRight
              className={`h-3.5 w-3.5 shrink-0 transition ${open ? "rotate-90" : ""}`}
              aria-hidden="true"
            />
          </span>
        </div>
        {open && (
          <ul className="space-y-0.5 pl-2">
            {todos.length === 0 ? (
              <li className="rounded-lg py-1.5 text-[13px] text-foreground-muted">
                暂无待办
              </li>
            ) : (
              todos.map((item) => {
                const presentation = todoPresentation(item.status);
                const Icon = presentation.icon;
                return (
                  <Tooltip content={item.content}>
                  <li
                    key={item.id}
                    className="flex cursor-pointer items-start gap-1.5 rounded-lg py-1.5 text-[13px] transition-colors"
                  >
                    <Icon
                      className={`h-4 w-4 mt-[1px] shrink-0 ${presentation.iconClassName}`}
                      aria-hidden="true"
                    />
                    <span className={`break-words truncate ${presentation.textClassName}`}>
                      {item.content}
                    </span>
                    </li>
                    </Tooltip>
                );
              })
            )}
          </ul>
        )}
      </div>
    </div>
  );
}

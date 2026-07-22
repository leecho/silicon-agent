import { Bot, Clock, FolderKanban, Sparkles } from "lucide-react";
import type { ScheduledTask } from "../../types";
import { relativeFromNow } from "./relativeTime";

export function TaskCard({
  task,
  onOpen,
}: {
  task: ScheduledTask;
  onOpen: () => void;
}) {
  const context = task.projectId
    ? { icon: FolderKanban, label: `项目 ${task.projectId}` }
    : task.agentId
      ? { icon: Bot, label: `智能体 ${task.agentId}` }
      : task.workingDir
        ? { icon: FolderKanban, label: task.workingDir.split(/[/\\]/).pop() || task.workingDir }
        : task.roleKind && task.roleId
          ? { icon: Sparkles, label: task.roleKind === "team" ? `团队 ${task.roleId}` : `专家 ${task.roleId}` }
          : null;
  const ContextIcon = context?.icon;

  return (
    <button
      type="button"
      className="relative block rounded-xl border border-border-subtle bg-surface p-4 text-left transition hover:border-border hover:bg-accent/40"
      onClick={onOpen}
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="min-w-0 truncate font-semibold text-foreground">{task.name}</div>
        <span className={`shrink-0 rounded-full px-2 py-0.5 text-xs ${task.enabled ? "bg-primary/10 text-primary" : "bg-muted text-foreground-muted"}`}>
          {task.enabled ? "启用" : "停用"}
        </span>
      </div>
      <div>
        <div className="overflow-hidden text-xs leading-5 text-foreground-muted" style={{ display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical" }}>
          {task.prompt}
        </div>
      </div>
      <div className="my-3 border-t border-dashed border-border-subtle" />
      <div className="flex flex-wrap items-center gap-2 text-xs text-foreground-muted">
        <span className="flex min-w-0 items-center gap-1 rounded-full bg-muted px-2 py-1">
          <Clock className="h-3.5 w-3.5" />
          <span className="truncate">{task.scheduleDisplay ?? task.scheduleSpec}</span>
        </span>
        {context && ContextIcon && (
          <span className="flex min-w-0 items-center gap-1 rounded-full bg-muted px-2 py-1">
            <ContextIcon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
            <span className="max-w-[160px] truncate">{context.label}</span>
          </span>
        )}
        {task.enabled && (
          <span className="ml-auto shrink-0">
            下次执行 {relativeFromNow(task.nextRunAt)}
          </span>
        )}
      </div>
    </button>
  );
}

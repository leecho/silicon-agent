import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, Clock3, PlayCircle, SkipForward, XCircle } from "lucide-react";
import { listTaskExecutions } from "../../api/scheduling";
import { Select } from "../../components/ui/Select";
import type { ScheduledTask, TaskExecution } from "../../types";
import { durationSecs, formatClock } from "./relativeTime";

const STATUS_LABEL: Record<string, string> = {
  running: "执行中",
  completed: "已完成",
  needs_attention: "需关注",
  failed: "失败",
  skipped: "已跳过",
};

const STATUS_CLASS: Record<string, string> = {
  running: "bg-muted text-primary",
  completed: "bg-muted text-foreground",
  needs_attention: "bg-muted text-foreground",
  failed: "bg-destructive/10 text-destructive",
  skipped: "bg-muted text-foreground-muted",
};

const STATUS_ICON = {
  running: PlayCircle,
  completed: CheckCircle2,
  needs_attention: AlertTriangle,
  failed: XCircle,
  skipped: SkipForward,
};

const STATUS_ICON_CLASS: Record<string, string> = {
  running: "text-primary",
  completed: "text-foreground",
  needs_attention: "text-foreground",
  failed: "text-destructive",
  skipped: "text-foreground-muted",
};

const TRIGGER_LABEL: Record<string, string> = {
  schedule: "定时",
  catchup: "补跑",
  manual: "手动触发",
};

export function ExecutionTimeline({
  lockedTaskId,
  tasks,
  refreshKey,
  onOpenSession,
}: {
  lockedTaskId?: string | null;
  tasks: ScheduledTask[];
  refreshKey: number;
  onOpenSession: (sessionId: string) => void;
}) {
  const [taskFilter, setTaskFilter] = useState<string>("");
  const [statusFilter, setStatusFilter] = useState<string>("");
  const [rows, setRows] = useState<TaskExecution[]>([]);

  useEffect(() => {
    void listTaskExecutions(lockedTaskId || taskFilter || undefined, statusFilter || undefined)
      .then(setRows)
      .catch(() => setRows([]));
  }, [lockedTaskId, taskFilter, statusFilter, refreshKey]);

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-2 text-sm">
        <h2 className="pl-2 text-base font-semibold text-foreground">执行记录</h2>
        {!lockedTaskId && (
          <Select
            value={taskFilter}
            onChange={setTaskFilter}
            options={[
              { label: "全部任务", value: "" },
              ...tasks.map((task) => ({ label: task.name, value: task.id })),
            ]}
          />
        )}
        <Select
          value={statusFilter}
          onChange={setStatusFilter}
          options={[
            { label: "全部状态", value: "" },
            ...Object.entries(STATUS_LABEL).map(([value, label]) => ({ label, value })),
          ]}
        />
      </div>
      {rows.length === 0 ? (
        <div className="py-10 text-center text-sm text-foreground-muted">
          暂无执行记录
        </div>
      ) : (
        <ul className="overflow-hidden rounded-lg border border-border-subtle">
          {rows.map((r, index) => {
            const StatusIcon = STATUS_ICON[r.status as keyof typeof STATUS_ICON] ?? Clock3;
            const statusIconClass = STATUS_ICON_CLASS[r.status] ?? "text-foreground-muted";
            const roundedClass =
              index === 0
                ? "rounded-t-lg"
                : index === rows.length - 1
                  ? "rounded-b-lg"
                  : "";
            return (
              <li key={r.id} className={index === rows.length - 1 ? "" : "border-b border-border-subtle"}>
                <button
                  type="button"
                  className={`group flex w-full items-start gap-3.5 px-4 py-4 text-left transition-colors hover:bg-card disabled:cursor-default disabled:hover:bg-transparent ${roundedClass}`}
                  disabled={!r.sessionId}
                  onClick={() => {
                    if (r.sessionId) onOpenSession(r.sessionId);
                  }}
                >
                  <span
                    className={`grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm ${statusIconClass}`}
                  >
                    <StatusIcon className="h-5 w-5" aria-hidden="true" />
                  </span>

                  <span className="min-w-0 flex-1">
                    <span className="flex min-w-0 items-center gap-3">
                      <span className="min-w-0 flex-1 truncate font-semibold text-foreground">
                        {r.taskName}
                      </span>
                      <span
                        className={`shrink-0 rounded-full px-2 py-0.5 text-xs ${STATUS_CLASS[r.status] ?? "bg-muted text-foreground-secondary"}`}
                      >
                        {STATUS_LABEL[r.status] ?? r.status}
                      </span>
                    </span>
                    <span className="mt-0.5 block truncate text-xs text-foreground-secondary">
                      {TRIGGER_LABEL[r.trigger] ?? r.trigger} · {formatClock(r.startedAt)} ·{" "}
                      {durationSecs(r.startedAt, r.finishedAt)}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

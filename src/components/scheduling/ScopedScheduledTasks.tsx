import { useEffect, useMemo, useState } from "react";
import { ChevronRight, Clock } from "lucide-react";

import {
  listScheduledTasks,
  subscribeScheduledTaskEvents,
} from "../../api/scheduling";
import type { ScheduledTask } from "../../types";

export function ScopedScheduledTasks({
  agentId,
  onGoList,
  onNewTask,
  projectId,
}: {
  agentId?: string | null;
  onGoList: () => void;
  onNewTask: () => void;
  projectId?: string | null;
}) {
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);

  const scoped = useMemo(
    () =>
      tasks.filter((task) =>
        projectId ? task.projectId === projectId : agentId ? task.agentId === agentId : false,
      ),
    [agentId, projectId, tasks],
  );
  const stats = useMemo(
    () => ({
      tasks: scoped.length,
    }),
    [scoped],
  );

  async function reload() {
    setTasks(await listScheduledTasks());
  }

  useEffect(() => {
    void reload().catch(console.error);
    let unlisten: (() => void) | undefined;
    void subscribeScheduledTaskEvents(() => void reload().catch(console.error)).then((u) => {
      unlisten = u;
    });
    return () => unlisten?.();
  }, []);

  return (
    <section className="rounded-xl border border-border-subtle bg-surface p-4">
      <div className="mb-3 flex items-center justify-between gap-3">
        <h3 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
          <Clock className="h-4 w-4 text-foreground-secondary" aria-hidden="true" />
          定时任务 {scoped.length}
        </h3>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="flex items-center gap-0.5 text-[12px] text-primary hover:text-foreground"
            onClick={onGoList}
          >
            查看全部
            <ChevronRight className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
          <button
            type="button"
            className="text-[12px] text-primary hover:text-foreground"
            onClick={onNewTask}
          >
            新建
          </button>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-2 text-left">
        <Metric label="任务数量" value={stats.tasks} />
      </div>
    </section>
  );
}

function Metric({
  label,
  value,
}: {
  label: string;
  value: number;
}) {
  return (
    <div className="rounded-lg border border-border-subtle bg-background py-3 px-3">
      <div className="text-lg font-semibold tabular-nums text-foreground">
        {value}
      </div>
      <div className="text-[11px] text-foreground-muted">{label}</div>
    </div>
  );
}

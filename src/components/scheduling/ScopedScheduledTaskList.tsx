import { useEffect, useMemo, useState } from "react";
import { Clock, Plus } from "lucide-react";

import {
  listScheduledTasks,
  subscribeScheduledTaskEvents,
} from "../../api/scheduling";
import { Button } from "../ui/Button";
import type { ScheduledTask } from "../../types";
import { TaskCard } from "../../pages/scheduling/TaskCard";

export function ScopedScheduledTaskList({
  agentId,
  label,
  onNewTask,
  onOpenTask,
  projectId,
}: {
  agentId?: string | null;
  label: string;
  onNewTask: () => void;
  onOpenTask: (taskId: string) => void;
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
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[900px]">
        <div className="mb-5 flex items-center justify-between gap-3">
          <div className="min-w-0">
            <h3 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
              <Clock className="h-4 w-4 text-foreground-secondary" aria-hidden="true" />
              定时任务 {scoped.length}
            </h3>
            <p className="mt-1 truncate text-xs text-foreground-muted">{label}</p>
          </div>
          <Button tone="primary" onClick={onNewTask}>
            <Plus className="h-4 w-4" aria-hidden="true" />
            新建定时任务
          </Button>
        </div>

        {scoped.length === 0 ? (
          <div className="grid min-h-[180px] place-items-center rounded-xl border border-dashed border-border-subtle text-xs text-foreground-muted">
            暂无定时任务
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {scoped.map((task) => (
              <TaskCard
                key={task.id}
                task={task}
                onOpen={() => onOpenTask(task.id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

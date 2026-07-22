import { useEffect, useRef, useState } from "react";
import { Info, Plus } from "lucide-react";
import { Button } from "../../components/ui/Button";
import { Switch } from "../../components/ui/Switch";
import {
  createScheduledTask,
  getKeepSystemAwake,
  listScheduledTasks,
  setKeepSystemAwake,
  subscribeScheduledTaskEvents,
  updateScheduledTask,
} from "../../api/scheduling";
import type { ScheduledTask, ScheduledTaskInput } from "../../types";
import { TaskCard } from "./TaskCard";
import { TaskFormDrawer } from "../../components/scheduling/TaskFormDrawer";
import { ExecutionTimeline } from "./ExecutionTimeline";
import { TaskDetailPage } from "./TaskDetailPage";
import type { AppLocation } from "../../hooks/useAppNavigation";

export function ScheduledTasksPage({
  agentId,
  create,
  onBack,
  onOpenTask,
  onReplace,
  projectId,
  taskId,
}: {
  agentId?: string | null;
  create?: boolean;
  onBack: () => void;
  onOpenTask: (taskId: string) => void;
  onReplace: (location: AppLocation) => void;
  projectId?: string | null;
  taskId?: string | null;
}) {
  const [tab, setTab] = useState<"tasks" | "executions">("tasks");
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [loading, setLoading] = useState(true);
  const [keepAwake, setKeepAwake] = useState(false);
  const [editing, setEditing] = useState<ScheduledTask | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  const formSavedRef = useRef(false);

  async function reload() {
    try {
      setLoading(true);
      setTasks(await listScheduledTasks());
    } catch {
      // ignore — no notifications provider requirement here
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
    getKeepSystemAwake().then(setKeepAwake).catch(() => {});
  }, []);

  useEffect(() => {
    if (!create) return;
    formSavedRef.current = false;
    setEditing(null);
    setShowForm(true);
  }, [create, agentId, projectId]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void subscribeScheduledTaskEvents(() => {
      void reload();
      setRefreshKey((k) => k + 1);
    }).then((u) => {
      unlisten = u;
    });
    return () => unlisten?.();
  }, []);

  async function handleToggleKeepAwake(v: boolean) {
    setKeepAwake(v);
    try {
      await setKeepSystemAwake(v);
    } catch {
      setKeepAwake(!v);
    }
  }

  async function handleSubmit(input: ScheduledTaskInput) {
    const scopedInput: ScheduledTaskInput = {
      ...input,
      agentId: agentId ?? input.agentId ?? null,
      projectId: projectId ?? input.projectId ?? null,
      workingDir: projectId || agentId ? null : input.workingDir,
    };
    if (editing) {
      const updated = await updateScheduledTask(editing.id, scopedInput);
      formSavedRef.current = true;
      onReplace({ section: "scheduling", taskId: updated.id });
    } else {
      const created = await createScheduledTask(scopedInput);
      formSavedRef.current = true;
      onReplace({ section: "scheduling", taskId: created.id });
    }
    await reload();
  }

  const selectedTask = taskId ? tasks.find((task) => task.id === taskId) ?? null : null;
  if (taskId && selectedTask) {
    return (
      <TaskDetailPage
        task={selectedTask}
        refreshKey={refreshKey}
        onBack={onBack}
        onDeleted={() => {
          onReplace({ section: "scheduling" });
          void reload();
        }}
        onReload={reload}
      />
    );
  }
  if (taskId) {
    return (
      <div className="h-full overflow-auto p-6 text-sm">
        <div className="mx-auto max-w-[860px] pt-4">
          <button
            type="button"
            className="mb-4 text-sm text-primary hover:text-foreground"
            onClick={onBack}
          >
            返回
          </button>
          <div className="rounded-xl border border-dashed border-border py-12 text-center text-foreground-muted">
            {loading ? "正在加载任务..." : "任务不存在或已被删除"}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-5 flex flex-wrap items-start justify-between gap-4 pt-4">
          <div className="min-w-0">
            <h1 className="text-xl font-semibold text-foreground">定时任务</h1>
            <p className="mt-1 text-xs text-foreground-muted">
              按计划自动执行任务，也可随时手动触发。
            </p>
          </div>
          <Button
            tone="primary"
            onClick={() => {
              setEditing(null);
              setShowForm(true);
            }}
          >
            <Plus className="h-4 w-4" aria-hidden="true" />
            新建定时任务
          </Button>
        </div>

        <div className="mb-6 flex flex-wrap items-center justify-between gap-3 rounded-lg bg-muted px-4 py-2.5">
          <span className="flex min-w-0 items-center gap-2 text-sm text-primary">
            <Info className="h-4 w-4 shrink-0 text-primary" aria-hidden="true" />
            定时任务仅在电脑保持唤醒时运行
          </span>
          <label className="flex cursor-pointer items-center gap-2 text-sm text-primary">
            <span>保持系统唤醒</span>
            <Switch checked={keepAwake} onChange={(v) => void handleToggleKeepAwake(v)} />
          </label>
        </div>

        <div className="mb-5 flex items-center justify-between gap-4">
          <div className="flex items-center gap-5">
            <TabButton active={tab === "tasks"} onClick={() => setTab("tasks")}>
              我的定时任务
            </TabButton>
            <TabButton active={tab === "executions"} onClick={() => setTab("executions")}>
              执行记录
            </TabButton>
          </div>
        </div>

        {tab === "tasks" ? (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {tasks.map((t) => (
              <TaskCard
                key={t.id}
                task={t}
                onOpen={() => onOpenTask(t.id)}
              />
            ))}
            {tasks.length === 0 && (
              <div className="col-span-full rounded-xl border border-dashed border-border py-12 text-center text-foreground-muted">
                暂无定时任务
              </div>
            )}
          </div>
        ) : (
          <ExecutionTimeline
            tasks={tasks}
            refreshKey={refreshKey}
            onOpenSession={(sid) => onReplace({ section: "session", sessionId: sid })}
          />
        )}

        {/* Create/Edit drawer */}
        {showForm && (
          <TaskFormDrawer
            fixedAgentId={agentId}
            fixedProjectId={projectId}
            initial={editing}
            onClose={() => {
              setShowForm(false);
              if (create && !formSavedRef.current) onReplace({ section: "scheduling" });
              formSavedRef.current = false;
            }}
            onSubmit={handleSubmit}
          />
        )}
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`text-sm transition-colors ${
        active
          ? "font-semibold text-foreground"
          : "font-semibold text-foreground-muted hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

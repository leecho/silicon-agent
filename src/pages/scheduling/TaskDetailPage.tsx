import { useState } from "react";
import {
  ArrowLeft,
  Bot,
  Clock,
  Edit3,
  FolderKanban,
  Gauge,
  KeyRound,
  Play,
  Sparkles,
  Trash2,
} from "lucide-react";

import {
  deleteScheduledTask,
  runTaskNow,
  setTaskEnabled,
  updateScheduledTask,
} from "../../api/scheduling";
import { TaskFormDrawer } from "../../components/scheduling/TaskFormDrawer";
import { Button } from "../../components/ui/Button";
import { Switch } from "../../components/ui/Switch";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { useSession } from "../../components/session/SessionProvider";
import type { ScheduledTask, ScheduledTaskInput } from "../../types";
import { ExecutionTimeline } from "./ExecutionTimeline";
import { relativeFromNow } from "./relativeTime";

export function TaskDetailPage({
  onBack,
  onDeleted,
  onReload,
  refreshKey,
  task,
}: {
  onBack: () => void;
  onDeleted: () => void;
  onReload: () => Promise<void>;
  refreshKey: number;
  task: ScheduledTask;
}) {
  const messages = useMessages();
  const notifications = useNotifications();
  const { openSession } = useSession();
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);

  async function toggleEnabled(enabled: boolean) {
    setBusy(true);
    try {
      await setTaskEnabled(task.id, enabled);
      await onReload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "更新任务失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  async function runNow() {
    setBusy(true);
    try {
      const sessionId = await runTaskNow(task.id);
      await onReload();
      if (sessionId) {
        openSession(sessionId);
      } else {
        notifications.notify({
          tone: "info",
          title: "任务已在运行中",
          message: "该任务有一次运行尚未结束，已跳过本次触发。",
        });
      }
    } catch (err) {
      notifications.notify({ tone: "error", title: "触发任务失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  async function deleteTask() {
    const ok = await messages.confirm({
      title: "删除定时任务",
      message: `确定删除「${task.name}」吗？历史执行会话会保留。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    setBusy(true);
    try {
      await deleteScheduledTask(task.id, false);
      onDeleted();
    } catch (err) {
      notifications.notify({ tone: "error", title: "删除任务失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  async function saveEdit(input: ScheduledTaskInput) {
    await updateScheduledTask(task.id, input);
    await onReload();
  }

  const context = taskContext(task);

  return (
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-5 flex items-start gap-3 pt-4">
          <button
            type="button"
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-foreground"
            onClick={onBack}
          >
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          </button>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h1 className="min-w-0 truncate text-xl font-semibold text-foreground">{task.name}</h1>
              <span className={`rounded-full px-2 py-0.5 text-xs ${task.enabled ? "bg-primary/10 text-primary" : "bg-muted text-foreground-muted"}`}>
                {task.enabled ? "启用" : "停用"}
              </span>
              {task.lastStatus && (
                <span className="rounded-full bg-muted px-2 py-0.5 text-xs text-foreground-secondary">
                  最近：{statusLabel(task.lastStatus)}
                </span>
              )}
            </div>
            <p className="mt-1 text-xs text-foreground-muted">
              {task.scheduleDisplay ?? task.scheduleSpec}
              {task.enabled && task.nextRunAt ? ` · 下次执行 ${relativeFromNow(task.nextRunAt)}` : ""}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Switch checked={task.enabled} onChange={(value) => void toggleEnabled(value)} />
            <Button disabled={busy} tone="outline" onClick={() => void runNow()}>
              <Play className="h-4 w-4" aria-hidden="true" />
              立即执行
            </Button>
            <Button disabled={busy} tone="outline" onClick={() => setEditing(true)}>
              <Edit3 className="h-4 w-4" aria-hidden="true" />
              编辑
            </Button>
            <Button disabled={busy} tone="danger" onClick={() => void deleteTask()}>
              <Trash2 className="h-4 w-4" aria-hidden="true" />
              删除
            </Button>
          </div>
        </div>

        <section className="rounded-xl border border-border-subtle bg-surface p-4">
          <h2 className="mb-3 text-sm font-semibold text-foreground">任务内容</h2>
          <div className="whitespace-pre-wrap rounded-lg bg-background px-3 py-3 text-sm leading-6 text-foreground-secondary">
            {task.prompt}
          </div>
        </section>

        <section className="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          <InfoRow icon={context.icon} label="上下文" value={context.label} />
          <InfoRow icon={Clock} label="计划" value={task.scheduleDisplay ?? task.scheduleSpec} />
          <InfoRow icon={Gauge} label="模型" value={task.modelId || "默认模型"} />
          <InfoRow icon={KeyRound} label="权限" value={task.permissionMode || "继承全局"} />
        </section>

        <section className="mt-6">
          <ExecutionTimeline
            lockedTaskId={task.id}
            tasks={[task]}
            refreshKey={refreshKey}
            onOpenSession={(sessionId) => openSession(sessionId)}
          />
        </section>

        {editing && (
          <TaskFormDrawer
            initial={task}
            onClose={() => setEditing(false)}
            onSubmit={saveEdit}
          />
        )}
      </div>
    </div>
  );
}

function InfoRow({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof Clock;
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-xl border border-border-subtle bg-surface px-3 py-3">
      <div className="mb-1 flex items-center gap-1.5 text-xs font-semibold text-foreground-muted">
        <Icon className="h-3.5 w-3.5" aria-hidden="true" />
        {label}
      </div>
      <div className="break-words text-sm text-foreground">{value}</div>
    </div>
  );
}

function taskContext(task: ScheduledTask): { icon: typeof Clock; label: string } {
  if (task.projectId) return { icon: FolderKanban, label: `项目 ${task.projectId}` };
  if (task.agentId) return { icon: Bot, label: `智能体 ${task.agentId}` };
  if (task.roleKind && task.roleId) {
    return {
      icon: Sparkles,
      label: task.roleKind === "team" ? `团队 ${task.roleId}` : `专家 ${task.roleId}`,
    };
  }
  if (task.workingDir) return { icon: FolderKanban, label: task.workingDir };
  return { icon: Sparkles, label: "默认上下文" };
}

function statusLabel(status: string): string {
  if (status === "running") return "执行中";
  if (status === "completed") return "已完成";
  if (status === "needs_attention") return "需关注";
  if (status === "failed") return "失败";
  if (status === "skipped") return "已跳过";
  return status;
}

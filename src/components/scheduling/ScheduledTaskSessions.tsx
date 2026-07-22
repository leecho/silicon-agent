import { useEffect, useState } from "react";
import {
  Clock,
  Loader2,
  MoreHorizontal,
  Pencil,
  Trash2,
} from "lucide-react";
import {
  listScheduledTasks,
  listTaskExecutions,
  subscribeScheduledTaskEvents,
} from "../../api/scheduling";
import { deleteSession, renameSession, subscribeAgentStreamEvents } from "../../api";
import {
  DropdownMenu,
  Tooltip,
  useMessages,
  useNotifications,
  type DropdownMenuPosition,
} from "../../components/ui";
import { useSession } from "../session/SessionProvider";
import type { ScheduledTask, TaskExecution } from "../../types";
import { SessionTreeContent, type SessionTreeNode } from "../session-tree/SessionTree";

export function ScheduledTaskSessions() {
  const { currentSessionId, openSession } = useSession();
  const messages = useMessages();
  const notifications = useNotifications();
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [execByTask, setExecByTask] = useState<Record<string, TaskExecution[]>>({});
  const [menuExec, setMenuExec] = useState<TaskExecution | null>(null);
  const [menuPosition, setMenuPosition] = useState<DropdownMenuPosition>({ x: 0, y: 0 });
  const [busyId, setBusyId] = useState<string | null>(null);
  async function reload() {
    try {
      setTasks(await listScheduledTasks());
    } catch {
      // ignore
    }
  }

  async function reloadExecs(taskId: string) {
    try {
      const list = await listTaskExecutions(taskId);
      setExecByTask((m) => ({ ...m, [taskId]: list }));
    } catch {
      // ignore
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  useEffect(() => {
    let taskUnsub: (() => void) | undefined;
    let streamUnsub: (() => void) | undefined;
    let cancelled = false;

    void subscribeScheduledTaskEvents(() => {
      void reload();
      // 同时刷新已展开任务的执行列表
      setExpanded((prev) => {
        prev.forEach((taskId) => void reloadExecs(taskId));
        return prev;
      });
    }).then((u) => {
      if (cancelled) {
        u();
      } else {
        taskUnsub = u;
      }
    });

    void subscribeAgentStreamEvents((e) => {
      if (e.kind === "run_finished" || e.kind === "run_started") {
        void reload();
      }
    }).then((u) => {
      if (cancelled) {
        u();
      } else {
        streamUnsub = u;
      }
    });

    return () => {
      cancelled = true;
      taskUnsub?.();
      streamUnsub?.();
    };
  }, []);

  async function toggle(taskId: string) {
    setExpanded((s) => {
      const n = new Set(s);
      if (n.has(taskId)) {
        n.delete(taskId);
      } else {
        n.add(taskId);
      }
      return n;
    });
    if (!execByTask[taskId]) {
      await reloadExecs(taskId);
    }
  }

  function openExecMenu(exec: TaskExecution, x: number, y: number) {
    setMenuExec(exec);
    setMenuPosition({
      x: Math.max(8, Math.min(x, window.innerWidth - 196)),
      y: Math.max(8, Math.min(y, window.innerHeight - 160)),
    });
  }

  async function handleRename(exec: TaskExecution) {
    setMenuExec(null);
    const current = exec.sessionTitle ?? "";
    const name = (
      await messages.prompt({
        title: "重命名会话",
        message: "输入新名称",
        defaultValue: current,
        placeholder: "会话标题",
        confirmText: "保存",
      })
    )?.trim();
    if (!name || name === current) return;
    try {
      await renameSession(exec.sessionId, name);
      await reloadExecs(exec.taskId);
    } catch (err) {
      notifications.notify({
        tone: "error",
        title: "重命名失败",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function handleDelete(exec: TaskExecution) {
    setMenuExec(null);
    const ok = await messages.confirm({
      title: "删除会话",
      message: "确定删除？此操作不可撤销。",
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    setBusyId(exec.sessionId);
    try {
      await deleteSession(exec.sessionId);
      if (exec.sessionId === currentSessionId) {
        openSession(null);
      }
      await Promise.all([reloadExecs(exec.taskId), reload()]);
    } catch (err) {
      notifications.notify({
        tone: "error",
        title: "删除失败",
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setBusyId(null);
    }
  }

  if (tasks.length === 0) return null;

  function executionActions(exec: TaskExecution, title: string) {
    const busy = busyId === exec.sessionId;
    return (
      <Tooltip content="更多">
        <button
          type="button"
          aria-label={`更多操作：${title}`}
          disabled={busy}
          onClick={(event) => {
            event.stopPropagation();
            openExecMenu(exec, event.clientX - 148, event.clientY + 6);
          }}
          className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-foreground-muted opacity-0 transition hover:bg-muted hover:text-foreground focus:opacity-100 group-hover:opacity-100 disabled:opacity-60"
        >
          {busy ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          ) : (
            <MoreHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
          )}
        </button>
      </Tooltip>
    );
  }

  const nodes: SessionTreeNode[] = tasks.map((task) => {
    // 仅展示仍有存活 session 的执行项（sessionTitle 非空）；已删除会话的执行项不在树中显示。
    const execs = (execByTask[task.id] ?? []).filter((exec) => exec.sessionTitle != null);
    return {
      id: task.id,
      label: task.name,
      icon: <Clock className="h-[15px] w-[15px]" aria-hidden="true" />,
      badge: task.executionCount,
      expanded: expanded.has(task.id),
      onClick: () => void toggle(task.id),
      children: execs.map((exec): SessionTreeNode => {
        const title = exec.sessionTitle || "未命名会话";
        return {
          id: exec.id,
          label: title,
          tooltip: title,
          active: exec.sessionId === currentSessionId,
          onClick: () => openSession(exec.sessionId),
          onContextMenu: (event) => {
            event.preventDefault();
            openExecMenu(exec, event.clientX, event.clientY);
          },
          trailing:
            exec.status === "running" ? (
              <Loader2
                className="h-3.5 w-3.5 shrink-0 animate-spin text-foreground-muted"
                aria-hidden="true"
              />
            ) : undefined,
          actions: executionActions(exec, title),
        };
      }),
    };
  });

  return (
    <>
      <SessionTreeContent nodes={nodes} />
      {menuExec && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setMenuExec(null)} />
          <DropdownMenu
            position={menuPosition}
            items={[
              {
                icon: Pencil,
                id: "rename",
                label: "重命名",
                onSelect: () => void handleRename(menuExec),
              },
              { id: "delete-separator", type: "separator" },
              {
                danger: true,
                icon: Trash2,
                id: "delete",
                label: "删除",
                onSelect: () => void handleDelete(menuExec),
              },
            ]}
          />
        </>
      )}
    </>
  );
}

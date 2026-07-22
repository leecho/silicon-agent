import { useMemo, useState } from "react";
import { ChevronDown, ChevronRight, KanbanSquare, List, PackageOpen } from "lucide-react";

import { Badge } from "../../components/ui/Badge";
import type { ProjectTask } from "../../types";
import { EmptyState } from "./EmptyState";

const TASK_COLS: Array<{ keys: ProjectTask["status"][]; label: string }> = [
  { keys: ["pending"], label: "待办" },
  { keys: ["in_progress"], label: "进行中" },
  { keys: ["done"], label: "已完成" },
  { keys: ["failed", "cancelled"], label: "失败/取消" },
];

type TaskViewMode = "board" | "list";

type ProjectTaskBoardProps = {
  tasks: ProjectTask[];
  artifactCountByTaskId?: Record<string, number>;
  onOpen: (t: ProjectTask) => void;
  onOpenArtifacts?: (t: ProjectTask) => void;
};

type TaskTree = {
  mainTasks: ProjectTask[];
  subsByMain: Map<string, ProjectTask[]>;
  orphanTasks: ProjectTask[];
};

function compareTaskSortAsc(a: ProjectTask, b: ProjectTask) {
  return a.sort - b.sort || a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id);
}

function compareTaskSortDesc(a: ProjectTask, b: ProjectTask) {
  return b.sort - a.sort || b.createdAt.localeCompare(a.createdAt) || b.id.localeCompare(a.id);
}

function buildTaskTree(tasks: ProjectTask[]): TaskTree {
  const mainTasks = tasks.filter((t) => !t.parentTaskId).sort(compareTaskSortDesc);
  const mainIds = new Set(mainTasks.map((t) => t.id));
  const subsByMain = new Map<string, ProjectTask[]>();
  const orphanTasks: ProjectTask[] = [];

  for (const task of tasks) {
    if (!task.parentTaskId) continue;
    if (!mainIds.has(task.parentTaskId)) {
      orphanTasks.push(task);
      continue;
    }
    const subs = subsByMain.get(task.parentTaskId) ?? [];
    subs.push(task);
    subsByMain.set(task.parentTaskId, subs);
  }

  for (const subs of subsByMain.values()) subs.sort(compareTaskSortAsc);
  orphanTasks.sort(compareTaskSortAsc);

  return { mainTasks, subsByMain, orphanTasks };
}

export function TaskStatusBadge({ status }: { status: ProjectTask["status"] }) {
  const map: Record<ProjectTask["status"], { label: string; tone: "neutral" | "info" | "success" | "danger" }> = {
    pending: { label: "待办", tone: "neutral" },
    in_progress: { label: "进行中", tone: "info" },
    done: { label: "完成", tone: "success" },
    failed: { label: "失败", tone: "danger" },
    cancelled: { label: "已取消", tone: "neutral" },
  };
  const s = map[status] ?? map.pending;
  return <Badge tone={s.tone}>{s.label}</Badge>;
}

function getTaskSessionId(task: ProjectTask) {
  return task.runSessionId || task.threadSessionId;
}

function getArtifactCount(task: ProjectTask, artifactCountByTaskId: Record<string, number>) {
  return artifactCountByTaskId[task.id] ?? 0;
}

function getTaskGroupArtifactCount(main: ProjectTask, subs: ProjectTask[], artifactCountByTaskId: Record<string, number>) {
  return [main, ...subs].reduce((total, task) => total + getArtifactCount(task, artifactCountByTaskId), 0);
}

export function ProjectTaskBoard({
  tasks: allTasks,
  artifactCountByTaskId = {},
  onOpen,
  onOpenArtifacts,
}: ProjectTaskBoardProps) {
  const [viewMode, setViewMode] = useState<TaskViewMode>("board");
  // 看板只看叶子工作项（子任务）；主任务是本轮分组，不作卡片，避免重复计数。
  const boardTasks = allTasks.filter((t) => t.parentTaskId);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-border-subtle px-4 py-2">
        <div>
          <h3 className="text-sm font-semibold text-foreground">任务</h3>
          <p className="text-[11px] text-foreground-muted">{boardTasks.length} 个任务</p>
        </div>
        <TaskViewSwitch value={viewMode} onChange={setViewMode} />
      </div>

      <div className="min-h-0 flex-1">
        {allTasks.length === 0 ? (
          <div className="p-10">
            <EmptyState icon={<KanbanSquare className="h-6 w-6" aria-hidden="true" />} title="还没有任务" hint="去「会话」说一个需要干活的需求，主持人会把计划列成任务派给成员，这里就会出现。" />
          </div>
        ) : viewMode === "board" ? (
          <BoardView tasks={boardTasks} artifactCountByTaskId={artifactCountByTaskId} onOpen={onOpen} onOpenArtifacts={onOpenArtifacts} />
        ) : (
          <ListView tasks={allTasks} artifactCountByTaskId={artifactCountByTaskId} onOpen={onOpen} onOpenArtifacts={onOpenArtifacts} />
        )}
      </div>
    </div>
  );
}

function TaskViewSwitch({ value, onChange }: { value: TaskViewMode; onChange: (value: TaskViewMode) => void }) {
  const options: Array<{ value: TaskViewMode; label: string; icon: typeof KanbanSquare }> = [
    { value: "board", label: "看板", icon: KanbanSquare },
    { value: "list", label: "列表", icon: List },
  ];

  return (
    <div className="flex items-center gap-0.5 rounded-lg border border-border-subtle bg-surface p-0.5">
      {options.map(({ value: itemValue, label, icon: Icon }) => {
        const active = value === itemValue;
        return (
          <button
            key={itemValue}
            type="button"
            onClick={() => onChange(itemValue)}
            className={`flex h-8 items-center gap-1.5 rounded-md px-2.5 text-[12px] transition ${active ? "bg-background text-foreground shadow-sm" : "text-foreground-muted hover:text-foreground"}`}
            aria-pressed={active}
          >
            <Icon className="h-3.5 w-3.5" aria-hidden="true" />
            {label}
          </button>
        );
      })}
    </div>
  );
}

function BoardView({
  tasks,
  artifactCountByTaskId,
  onOpen,
  onOpenArtifacts,
}: {
  tasks: ProjectTask[];
  artifactCountByTaskId: Record<string, number>;
  onOpen: (t: ProjectTask) => void;
  onOpenArtifacts?: (t: ProjectTask) => void;
}) {
  return (
    <div className="h-full overflow-x-auto p-4">
      <div className="flex h-full min-w-[760px] gap-3">
        {TASK_COLS.map((col) => {
          const items = tasks.filter((t) => col.keys.includes(t.status));
          return (
            <div key={col.label} className="flex w-[260px] shrink-0 flex-col rounded-xl border border-border-subtle bg-surface">
              <div className="flex items-center justify-between border-b border-border-subtle px-3 py-2 text-[12px] font-medium text-foreground-secondary">
                <span>{col.label}</span><span className="text-foreground-muted">{items.length}</span>
              </div>
              <ul className="min-h-0 flex-1 space-y-2 overflow-auto p-2">
                {items.map((t) => {
                  const openable = !!getTaskSessionId(t);
                  return (
                    <li key={t.id} className="rounded-lg border border-border-subtle bg-background transition hover:border-border">
                      <button
                        type="button"
                        onClick={() => onOpen(t)}
                        disabled={!openable}
                        className={`w-full px-3 py-2.5 text-left ${openable ? "" : "cursor-default opacity-90"}`}
                      >
                        <div className="mb-1 truncate text-[13px] font-medium text-foreground">{t.title}</div>
                        <p className="truncate text-[11px] text-foreground-muted">{t.assignee || "自办"}</p>
                      </button>
                      <div className="border-t border-border-subtle px-2 py-1.5">
                        <TaskArtifactCountButton task={t} count={getArtifactCount(t, artifactCountByTaskId)} onOpenArtifacts={onOpenArtifacts} />
                      </div>
                    </li>
                  );
                })}
                {items.length === 0 && <li className="px-1 py-2 text-center text-[11px] text-foreground-muted">—</li>}
              </ul>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TaskArtifactCountButton({
  count,
  task,
  onOpenArtifacts,
}: {
  count: number;
  task: ProjectTask;
  onOpenArtifacts?: (task: ProjectTask) => void;
}) {
  const clickable = !!onOpenArtifacts;
  return (
    <button
      type="button"
      onClick={(event) => {
        event.stopPropagation();
        onOpenArtifacts?.(task);
      }}
      disabled={!clickable}
      className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[11px] transition ${
        clickable ? "text-foreground-muted hover:bg-accent hover:text-foreground" : "cursor-default text-foreground-muted"
      }`}
      aria-label={`${task.title} 产物 ${count} 个`}
      title={`${count} 个产物`}
    >
      <PackageOpen className="h-3.5 w-3.5" aria-hidden="true" />
      <span>{count}</span>
      <span>产物</span>
    </button>
  );
}

function ListView({
  tasks,
  artifactCountByTaskId,
  onOpen,
  onOpenArtifacts,
}: {
  tasks: ProjectTask[];
  artifactCountByTaskId: Record<string, number>;
  onOpen: (t: ProjectTask) => void;
  onOpenArtifacts?: (t: ProjectTask) => void;
}) {
  const { mainTasks, subsByMain, orphanTasks } = useMemo(() => buildTaskTree(tasks), [tasks]);
  const [openIds, setOpenIds] = useState<string[] | null>(null);
  const defaultOpenIds = mainTasks[0] ? [mainTasks[0].id] : [];
  const openSet = new Set(openIds ?? defaultOpenIds);
  const toggleMain = (id: string) => {
    const next = new Set(openSet);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setOpenIds(Array.from(next));
  };

  const renderSubTask = (sub: ProjectTask) => {
    const openable = !!getTaskSessionId(sub);
    return (
      <li key={sub.id}>
        <div className="grid grid-cols-[minmax(220px,1fr)_120px_minmax(160px,220px)_96px] gap-3 px-3 py-2.5 text-left transition hover:bg-accent">
          <button
            type="button"
            onClick={() => onOpen(sub)}
            disabled={!openable}
            className={`min-w-0 border-l border-border-subtle pl-5 text-left text-[13px] font-medium text-foreground ${openable ? "" : "cursor-default"}`}
          >
            <span className="block truncate">{sub.title}</span>
          </button>
          <span><TaskStatusBadge status={sub.status} /></span>
          <span className="min-w-0 truncate text-[12px] text-foreground-secondary">{sub.assignee || "自办"}</span>
          <TaskArtifactCountButton task={sub} count={getArtifactCount(sub, artifactCountByTaskId)} onOpenArtifacts={onOpenArtifacts} />
        </div>
      </li>
    );
  };

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto w-full border-b border-border-subtle bg-surface">
        <div className="grid grid-cols-[minmax(220px,1fr)_120px_minmax(160px,220px)_96px] gap-3 border-b border-border-subtle px-3 py-2 text-[11px] font-medium text-foreground-muted">
          <span>任务</span>
          <span>状态</span>
          <span>负责人 / 子任务</span>
          <span>产物</span>
        </div>
        <ul className="divide-y divide-border-subtle">
          {mainTasks.map((main) => {
            const subs = subsByMain.get(main.id) ?? [];
            const open = openSet.has(main.id);
            return (
              <li key={main.id}>
                <div className="grid grid-cols-[minmax(220px,1fr)_120px_minmax(160px,220px)_96px] gap-3 px-3 py-3 text-left transition hover:bg-accent">
                  <button
                    type="button"
                    onClick={() => toggleMain(main.id)}
                    className="flex min-w-0 items-center gap-2 text-left text-[13px] font-semibold text-foreground"
                    aria-expanded={open}
                  >
                    {open ? <ChevronDown className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" /> : <ChevronRight className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />}
                    <span className="min-w-0 truncate" title={main.title}>{main.title}</span>
                  </button>
                  <span><TaskStatusBadge status={main.status} /></span>
                  <span className="min-w-0 truncate text-[12px] text-foreground-muted">{subs.length} 个子任务</span>
                  <TaskArtifactCountButton task={main} count={getTaskGroupArtifactCount(main, subs, artifactCountByTaskId)} onOpenArtifacts={onOpenArtifacts} />
                </div>
                {open && (
                  subs.length > 0 ? (
                    <ul className="border-t border-border-subtle bg-background/40">{subs.map((sub) => renderSubTask(sub))}</ul>
                  ) : (
                    <div className="border-t border-border-subtle px-8 py-2 text-[12px] text-foreground-muted">尚未拆分子任务</div>
                  )
                )}
              </li>
            );
          })}
          {orphanTasks.length > 0 && (
            <li>
              <div className="grid grid-cols-[minmax(220px,1fr)_120px_minmax(160px,220px)_96px] gap-3 px-3 py-3 text-left">
                <span className="min-w-0 truncate text-[13px] font-semibold text-foreground">未归类任务</span>
                <span className="text-[12px] text-foreground-muted">—</span>
                <span className="min-w-0 truncate text-[12px] text-foreground-muted">{orphanTasks.length} 个子任务</span>
                <TaskArtifactCountButton
                  task={orphanTasks[0]}
                  count={orphanTasks.reduce((total, task) => total + getArtifactCount(task, artifactCountByTaskId), 0)}
                  onOpenArtifacts={onOpenArtifacts}
                />
              </div>
              <ul className="border-t border-border-subtle bg-background/40">{orphanTasks.map((sub) => renderSubTask(sub))}</ul>
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}

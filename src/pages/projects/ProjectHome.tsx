import { useEffect, useState } from "react";
import { ClipboardList, Crown, FileIcon, FolderKanban, FolderOpen, Pencil, Trash2, UserPlus, Users } from "lucide-react";

import { pickDirectory, setProjectInstructions, setProjectWorkspace, updateProject } from "../../api";
import { Button } from "../../components/ui/Button";
import { BubbleConfirm } from "../../components/ui/BubbleConfirm";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { MarkdownText } from "../../components/ui/MarkdownText";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Project, ProjectChildRun, ProjectMember, ProjectTask } from "../../types";
import { ProjectMemberAvatar } from "./ProjectMemberAvatar";
import { TaskStatusBadge } from "./ProjectTaskBoard";
import { Tooltip } from "../../components/ui";

function baseName(p: string): string {
  const t = p.replace(/[/\\]+$/, "");
  const i = Math.max(t.lastIndexOf("/"), t.lastIndexOf("\\"));
  return i >= 0 ? t.slice(i + 1) : t;
}

/** 常驻概况区：项目名称 / 描述 / 工作目录。始终显示在项目详情顶部，不随 Tab 切换（项目指令改为独立 Tab）。 */
export function ProjectOverviewPanel({
  project,
  onOpenWorkspace,
  onReload,
}: {
  project: Project;
  onOpenWorkspace: () => void;
  onReload: () => void;
}) {
  const [editOpen, setEditOpen] = useState(false);
  const wsName = project.workspaceDir ? baseName(project.workspaceDir) : undefined;
  const wsLabel = wsName ?? "默认工作目录";
  return (
    <div>
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <span className="grid h-12 w-12 shrink-0 place-items-center rounded-xl border border-border-subtle bg-background text-primary">
            <FolderKanban className="h-6 w-6" aria-hidden="true" />
          </span>
          <div className="min-w-0 pt-0.5">
            <h2 className="truncate text-base font-semibold text-foreground">{project.name}</h2>
            {project.description?.trim() && <p className="mt-1 line-clamp-2 text-[13px] leading-6 text-foreground-secondary">{project.description}</p>}
          </div>
        </div>
        <button type="button" onClick={() => setEditOpen(true)} className="flex shrink-0 items-center gap-1 text-[12px] text-primary">
          <Pencil className="h-3.5 w-3.5" aria-hidden="true" /> 编辑
        </button>
      </div>
      {editOpen && <ProjectEditDrawer project={project} onClose={() => setEditOpen(false)} onSaved={() => { setEditOpen(false); onReload(); }} />}
    </div>
  );
}

/** 项目指令 Tab：完整 markdown 展示 + 编辑（PM 章程）。 */
export function ProjectInstructionsPanel({ project, onReload }: { project: Project; onReload: () => void }) {
  const [editOpen, setEditOpen] = useState(false);
  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
            <FileIcon className="h-4 w-4 text-foreground-secondary" aria-hidden="true" /> 项目指令
          </h3>
          <button type="button" onClick={() => setEditOpen(true)} className="flex items-center gap-1 text-[12px] text-primary">
            <Pencil className="h-3.5 w-3.5" aria-hidden="true" /> 编辑指令
          </button>
        </div>
        {project.instructions?.trim() ? (
          <div className="rounded-xl border border-border-subtle bg-surface px-4 py-3">
            <MarkdownText value={project.instructions} className="text-[13px] leading-6 text-foreground-secondary" />
          </div>
        ) : (
          <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">
            未设置——当前用通用项目经理。点击「编辑指令」为 PM 设定章程。
          </p>
        )}
      </div>
      <ProjectInstructionsEditDrawer project={project} open={editOpen} onClose={() => setEditOpen(false)} onSaved={() => { setEditOpen(false); onReload(); }} />
    </div>
  );
}

/** 成员 Tab：成员列表 + 添加 / 从团队导入 + 成员详情抽屉。 */
export function ProjectMembersPanel({
  members,
  runs,
  tasks,
  onAddMember,
  onImportTeam,
  onRemoveMember,
  onOpenSession,
}: {
  members: ProjectMember[];
  runs: ProjectChildRun[];
  tasks: ProjectTask[];
  onAddMember: () => void;
  onImportTeam: () => void;
  onRemoveMember: (id: string) => void;
  onOpenSession: (id: string) => void;
}) {
  const [confirmingMember, setConfirmingMember] = useState<string | null>(null);
  const [selectedMemberId, setSelectedMemberId] = useState<string | null>(null);
  const selectedMember = members.find((m) => m.id === selectedMemberId) ?? null;

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
            <Users className="h-4 w-4 text-foreground-secondary" aria-hidden="true" /> 成员 {members.length}
          </h3>
          <div className="flex items-center gap-3">
            <button type="button" onClick={onImportTeam} className="flex items-center gap-1 text-[12px] text-primary hover:text-foreground">
              <Users className="h-3.5 w-3.5" aria-hidden="true" /> 从团队导入
            </button>
            <button type="button" onClick={onAddMember} className="flex items-center gap-1 text-[12px] text-primary">
              <UserPlus className="h-3.5 w-3.5" aria-hidden="true" /> 添加
            </button>
          </div>
        </div>
        {members.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">把专家拉进来；标一个为协调者(PM)。</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {members.map((m) => {
              const label = m.displayName || m.expertName;
              return (
                <li key={m.id} className="group relative flex min-w-0 items-center gap-2 rounded-md border border-border-subtle bg-surface px-2.5 py-2 transition hover:border-border">
                  <button type="button" onClick={() => setSelectedMemberId(m.id)} className="flex min-w-0 flex-1 items-center gap-2 text-left">
                    <ProjectMemberAvatar member={m} />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1">
                        <span className="truncate text-[13px] font-medium text-foreground">{label}</span>
                        {m.isCoordinator && <Crown className="h-3 w-3 shrink-0 text-amber-500" aria-hidden="true" />}
                      </div>
                      {m.roleLabel && <span className="block truncate text-[11px] text-foreground-muted">{m.roleLabel}</span>}
                    </div>
                  </button>
                  <button
                    type="button"
                    title="移除"
                    onClick={() => setConfirmingMember((cur) => (cur === m.id ? null : m.id))}
                    className="ml-auto grid h-6 w-6 shrink-0 place-items-center rounded-md text-destructive opacity-0 transition hover:bg-destructive/10 focus-visible:bg-destructive/10 group-hover:opacity-100 group-focus-within:opacity-100"
                  >
                    <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                  </button>
                  {confirmingMember === m.id && (
                    <BubbleConfirm
                      title="移除成员？"
                      description={`将从项目中移除「${label}」。`}
                      confirmText="删除"
                      onCancel={() => setConfirmingMember(null)}
                      onConfirm={() => {
                        setConfirmingMember(null);
                        onRemoveMember(m.id);
                      }}
                    />
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>

      {selectedMember && (
        <ProjectMemberDetailDrawer
          member={selectedMember}
          runs={runs}
          tasks={tasks}
          open
          onClose={() => setSelectedMemberId(null)}
          onOpenSession={(id) => {
            setSelectedMemberId(null);
            onOpenSession(id);
          }}
        />
      )}
    </div>
  );
}

function getMemberTasks(member: ProjectMember, tasks: ProjectTask[], runs: ProjectChildRun[]) {
  const runsBySessionId = new Map(runs.map((run) => [run.sessionId, run]));
  const displayName = member.displayName?.trim();
  return tasks
    .filter((task) => {
      if (!task.parentTaskId) return false;
      if (task.runSessionId) {
        const run = runsBySessionId.get(task.runSessionId);
        if (run) return run.expertName === member.expertName;
      }
      if (task.assignee === member.expertName) return true;
      return Boolean(displayName && task.assignee === displayName);
    })
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || b.createdAt.localeCompare(a.createdAt) || a.sort - b.sort || a.id.localeCompare(b.id));
}

function countMemberTasks(tasks: ProjectTask[], statuses: ProjectTask["status"][]) {
  return tasks.filter((task) => statuses.includes(task.status)).length;
}

function findTaskRun(task: ProjectTask, runs: ProjectChildRun[]) {
  if (!task.runSessionId) return null;
  return runs.find((run) => run.sessionId === task.runSessionId) ?? null;
}

function ProjectMemberDetailDrawer({
  member,
  runs,
  tasks,
  open,
  onClose,
  onOpenSession,
}: {
  member: ProjectMember;
  runs: ProjectChildRun[];
  tasks: ProjectTask[];
  open: boolean;
  onClose: () => void;
  onOpenSession: (id: string) => void;
}) {
  const label = member.displayName || member.expertName;
  const memberTasks = getMemberTasks(member, tasks, runs);
  const stats = [
    { label: "待办", value: countMemberTasks(memberTasks, ["pending"]) },
    { label: "进行中", value: countMemberTasks(memberTasks, ["in_progress"]) },
    { label: "已完成", value: countMemberTasks(memberTasks, ["done"]) },
    { label: "失败/取消", value: countMemberTasks(memberTasks, ["failed", "cancelled"]) },
  ];

  return (
    <Drawer className="w-[min(640px,94vw)] bg-popover text-popover-foreground" open={open} onClose={onClose} title="成员详情">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">成员详情 <span className="text-[12px] font-normal text-foreground-muted">· {label}</span></h2>
      </DrawerHeader>
      <div className="min-h-0 overflow-auto bg-popover px-5 py-4">
        <div className="flex items-start gap-3 border-b border-border-subtle pb-4">
          <ProjectMemberAvatar member={member} />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-1.5">
              <h3 className="min-w-0 truncate text-base font-semibold text-foreground">{label}</h3>
              {member.isCoordinator && <Crown className="h-3.5 w-3.5 shrink-0 text-amber-500" aria-hidden="true" />}
            </div>
            <p className="mt-0.5 truncate text-[12px] text-foreground-muted">{member.expertName}</p>
            {member.roleLabel && <p className="mt-2 text-[13px] text-foreground-secondary">{member.roleLabel}</p>}
          </div>
        </div>

        <section className="border-b border-border-subtle py-4">
          <h3 className="mb-2 text-sm font-semibold text-foreground">职责</h3>
          <p className="whitespace-pre-wrap text-[13px] leading-6 text-foreground-secondary">
            {member.responsibilities?.trim() || "未设置职责。"}
          </p>
        </section>

        <section className="py-4">
          <div className="mb-3 flex items-center justify-between gap-3">
            <h3 className="text-sm font-semibold text-foreground">负责的任务</h3>
            <span className="text-[12px] text-foreground-muted">{memberTasks.length} 个任务</span>
          </div>
          <div className="mb-3 grid grid-cols-4 gap-2 text-center">
            {stats.map((item) => (
              <div key={item.label} className="rounded-lg border border-border-subtle bg-background py-2">
                <div className="text-base font-semibold text-foreground">{item.value}</div>
                <div className="text-[11px] text-foreground-muted">{item.label}</div>
              </div>
            ))}
          </div>
          {memberTasks.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-border-subtle bg-background px-4 py-8 text-center">
              <ClipboardList className="h-5 w-5 text-foreground-muted" aria-hidden="true" />
              <p className="text-[12px] text-foreground-muted">暂无分配任务。</p>
            </div>
          ) : (
            <ul className="divide-y divide-border-subtle rounded-lg border border-border-subtle bg-background">
              {memberTasks.map((task) => {
                const run = findTaskRun(task, runs);
                return (
                  <li key={task.id}>
                    <button
                      type="button"
                      onClick={() => { if (task.runSessionId) onOpenSession(task.runSessionId); }}
                      disabled={!task.runSessionId}
                      className={`grid w-full grid-cols-[minmax(0,1fr)_auto] gap-3 px-3 py-3 text-left transition ${task.runSessionId ? "hover:bg-accent" : "cursor-default"}`}
                    >
                      <span className="min-w-0">
                        <span className="block truncate text-[13px] font-medium text-foreground" title={task.title}>{task.title}</span>
                        <span className="mt-1 block truncate text-[11px] text-foreground-muted">
                          {run?.threadTitle || run?.task || task.assignee || "未关联运行"}
                        </span>
                      </span>
                      <span className="shrink-0"><TaskStatusBadge status={task.status} /></span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>
    </Drawer>
  );
}

/** 项目编辑抽屉：名称 / 描述 / 工作目录 / 项目指令。 */
function ProjectEditDrawer({ project, onClose, onSaved }: { project: Project; onClose: () => void; onSaved: () => void }) {
  const notify = useNotifications();
  const [name, setName] = useState(project.name);
  const [desc, setDesc] = useState(project.description ?? "");
  const [workspaceDir, setWorkspaceDir] = useState<string | null>(project.workspaceDir ?? null);
  const [busy, setBusy] = useState(false);

  async function save() {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      await updateProject(project.id, name.trim(), desc.trim());
      if (workspaceDir && workspaceDir !== (project.workspaceDir ?? null)) await setProjectWorkspace(project.id, workspaceDir);
      onSaved();
    } catch (err) {
      setBusy(false);
      notify.notify({ tone: "error", title: "保存失败", message: String(err) });
    }
  }

  return (
    <Drawer width="640px" className="w-[min(560px)] bg-popover text-popover-foreground" open onClose={onClose} title="编辑项目">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 flex-1 truncate text-base font-semibold text-foreground">编辑项目</h2>
      </DrawerHeader>
      <div className="min-h-0 space-y-4 overflow-auto bg-popover px-5 py-4">
        <div className="space-y-1">
          <label className="text-[12px] font-medium text-foreground-secondary">项目名称 *</label>
          <input className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className="space-y-1">
          <label className="text-[12px] font-medium text-foreground-secondary">项目描述</label>
          <input className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" value={desc} onChange={(e) => setDesc(e.target.value)} />
        </div>
        <div className="space-y-1">
          <label className="text-[12px] font-medium text-foreground-secondary">工作目录</label>
          <div className="flex items-center gap-2">
            <Button tone="outline" onClick={() => void pickDirectory().then((d) => { if (d) setWorkspaceDir(d); }).catch(() => {})}>
              <FolderOpen className="h-4 w-4" aria-hidden="true" /> {workspaceDir ? "更换目录" : "选择目录"}
            </Button>
            <span className="min-w-0 flex-1 truncate text-[12px] text-foreground-muted" title={workspaceDir ?? undefined}>{workspaceDir ?? "默认（自动创建）"}</span>
          </div>
        </div>
      </div>
      <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
        <Button tone="outline" onClick={onClose}>取消</Button>
        <Button tone="primary" onClick={() => void save()} disabled={!name.trim() || busy}>保存</Button>
      </div>
    </Drawer>
  );
}

/** 项目指令编辑：独立抽屉保存 PM 章程。 */
function ProjectInstructionsEditDrawer({ project, open, onClose, onSaved }: { project: Project; open: boolean; onClose: () => void; onSaved: () => void }) {
  const notify = useNotifications();
  const [text, setText] = useState(project.instructions ?? "");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setText(project.instructions ?? "");
    }
  }, [open, project.id, project.instructions]);

  async function save() {
    setBusy(true);
    try {
      await setProjectInstructions(project.id, text.trim());
      onSaved();
    } catch (err) {
      notify.notify({ tone: "error", title: "保存失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Drawer width="640px" className="w-[640px] bg-popover text-popover-foreground" open={open} onClose={onClose} title="编辑项目指令">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">编辑项目指令 <span className="text-[12px] font-normal text-foreground-muted">· {project.name}</span></h2>
      </DrawerHeader>
      <div className="flex min-h-0 flex-col bg-popover">
        <div className="flex min-h-0 flex-1 flex-col px-5 py-4">
          <textarea className="min-h-[260px] w-full flex-1 resize-none rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" placeholder="作为 PM 的章程：目标、风格、红线、如何调度成员。留空则用通用 PM。" value={text} onChange={(e) => setText(e.target.value)} autoFocus />
        </div>
        <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
          <Button tone="outline" onClick={onClose}>取消</Button>
          <Button tone="primary" onClick={() => void save()} disabled={busy}>保存</Button>
        </div>
      </div>
    </Drawer>
  );
}

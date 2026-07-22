import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, ChevronDown, ChevronRight, MessagesSquare, Pencil, Plus, Trash2 } from "lucide-react";

import {
  addProjectMember,
  deleteSession,
  getProjectUsage,
  getSessionMessageUsage,
  getTeamDetail,
  importTeamMember,
  listProjectArtifacts,
  listProjectChildRuns,
  listProjectMembers,
  listProjectSessions,
  listProjectSkills,
  listProjectTasks,
  listProjectWorkspaceFiles,
  listStandaloneExperts,
  listTeams,
  openProjectWorkspace,
  openProjectWorkspaceFile,
  readProjectWorkspaceFile,
  removeProjectMember,
  renameSession,
  subscribeAgentStreamEvents,
} from "../../api";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { useSession } from "../../components/session/SessionProvider";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { ExpertSummary, Project, ProjectArtifact, ProjectChildRun, ProjectMember, ProjectSkill, ProjectTask, ScopedUsageView, SessionInfo, Team, UsageMessageRow, UsageRange, UsageSessionRow } from "../../types";
import { ProjectArtifactList } from "./ProjectArtifactList";
import { MemberPickerDialog } from "./MemberPicker";
import { ProjectOverviewPanel, ProjectMembersPanel, ProjectInstructionsPanel } from "./ProjectHome";
import { KnowledgeScopeCard } from "../knowledge-bases/KnowledgeScopeCard";
import { MemorySection } from "../settings/sections/MemorySection";
import { ScopedScheduledTaskList } from "../../components/scheduling/ScopedScheduledTaskList";
import { ProjectTaskBoard } from "./ProjectTaskBoard";
import { TeamPickerDialog } from "./TeamPicker";
import { WorkspaceBrowser } from "../../components/session/WorkspaceBrowser";
import { Tabs } from "../../components/ui/Tabs";
import { formatTokens, formatTs, sessionLabel } from "../settings/sections/usage/usageFormat";
import { SkillDetailDrawer } from "../skills/SkillDetailDrawer";

type ProjectViewMode = "instructions" | "board" | "artifacts" | "chat" | "usage" | "memory" | "knowledge" | "scheduling" | "skills" | "members" | "workspace";

const PROJECT_TABS: { value: ProjectViewMode; label: string }[] = [
  { value: "instructions", label: "指令" },
  { value: "board", label: "任务" },
  { value: "artifacts", label: "产物" },
  { value: "workspace", label: "工作目录" },
  { value: "usage", label: "用量" },
  { value: "memory", label: "记忆" },
  { value: "knowledge", label: "知识" },
  { value: "skills", label: "技能" },
  { value: "scheduling", label: "定时任务" },
  { value: "members", label: "成员" },
  { value: "chat", label: "会话" },
];

function workspaceBaseName(p?: string | null): string {
  if (!p) return "默认工作目录";
  const t = p.replace(/[/\\]+$/, "");
  const seg = t.slice(Math.max(t.lastIndexOf("/"), t.lastIndexOf("\\")) + 1);
  return seg || p;
}

function normalizeArtifactTaskKey(value: string) {
  return value.trim().toLocaleLowerCase();
}

function buildArtifactCountByTaskId(tasks: ProjectTask[], artifacts: ProjectArtifact[]) {
  const counts: Record<string, number> = {};
  const tasksByTitle = new Map<string, ProjectTask[]>();
  const tasksByRunSessionId = new Map<string, ProjectTask>();

  for (const task of tasks) {
    const titleKey = normalizeArtifactTaskKey(task.title);
    const sameTitleTasks = tasksByTitle.get(titleKey) ?? [];
    sameTitleTasks.push(task);
    tasksByTitle.set(titleKey, sameTitleTasks);

    if (task.runSessionId) tasksByRunSessionId.set(task.runSessionId, task);
  }

  for (const artifact of artifacts) {
    const titleKey = normalizeArtifactTaskKey(artifact.task);
    const titleMatches = titleKey ? tasksByTitle.get(titleKey) ?? [] : [];
    const sessionMatch = tasksByRunSessionId.get(artifact.sessionId);
    const task =
      sessionMatch && titleMatches.some((item) => item.id === sessionMatch.id)
        ? sessionMatch
        : titleMatches.length === 1
          ? titleMatches[0]
          : sessionMatch;

    if (!task) continue;
    counts[task.id] = (counts[task.id] ?? 0) + 1;
  }

  return counts;
}

export function ProjectView({
  project,
  onBack,
  onNewScheduledTask,
  onOpenScheduledTask,
  onReload,
}: {
  project: Project;
  onBack: () => void;
  onNewScheduledTask: (projectId: string) => void;
  onOpenScheduledTask: (taskId: string) => void;
  onReload: () => void;
}) {
  const messages = useMessages();
  const notify = useNotifications();
  const { enterDraftWithProject, openSession } = useSession();
  const [members, setMembers] = useState<ProjectMember[]>([]);
  const [projectSessions, setProjectSessions] = useState<SessionInfo[]>([]);
  const [adding, setAdding] = useState(false);
  const [agents, setExperts] = useState<ExpertSummary[]>([]);
  const [teamImportOpen, setTeamImportOpen] = useState(false);
  const [teams, setTeams] = useState<Team[]>([]);
  const [view, setView] = useState<ProjectViewMode>("board");
  const [runs, setRuns] = useState<ProjectChildRun[]>([]);
  const [tasks, setTasks] = useState<ProjectTask[]>([]);
  const [artifacts, setArtifacts] = useState<ProjectArtifact[]>([]);
  const [projectSkills, setProjectSkills] = useState<ProjectSkill[]>([]);
  const [workspaceFiles, setWorkspaceFiles] = useState<string[]>([]);
  const [workspaceLoading, setWorkspaceLoading] = useState(false);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);

  /** 拉取项目工作目录文件列表（进入「工作目录」Tab 或手动刷新时）。 */
  async function reloadWorkspaceFiles() {
    setWorkspaceLoading(true);
    setWorkspaceError(null);
    try {
      setWorkspaceFiles(await listProjectWorkspaceFiles(project.id));
    } catch (err) {
      setWorkspaceError(String(err));
    } finally {
      setWorkspaceLoading(false);
    }
  }

  async function reloadMembers() {
    try {
      setMembers(await listProjectMembers(project.id));
    } catch (err) {
      notify.notify({ tone: "error", title: "加载成员失败", message: String(err) });
    }
  }
  async function reloadRuns() {
    try {
      const [nextRuns, nextTasks, nextArtifacts] = await Promise.all([
        listProjectChildRuns(project.id),
        listProjectTasks(project.id),
        listProjectArtifacts(project.id),
      ]);
      setRuns(nextRuns);
      setTasks(nextTasks);
      setArtifacts(nextArtifacts);
    } catch {
      /* 投影失败不打扰 */
    }
  }
  async function reloadProjectSessions(): Promise<SessionInfo[]> {
    const sessions = await listProjectSessions(project.id);
    setProjectSessions(sessions);
    return sessions;
  }
  async function reloadProjectSkills() {
    try {
      setProjectSkills(await listProjectSkills(project.id));
    } catch {
      setProjectSkills([]);
    }
  }

  useEffect(() => {
    void reloadMembers();
    void reloadRuns();
    void reloadProjectSessions();
    void reloadProjectSkills();
    let off: (() => void) | undefined;
    void subscribeAgentStreamEvents((e) => {
      if (e.kind === "run_started" || e.kind === "run_finished" || e.kind === "tool_result" || e.kind === "permission_required" || e.kind === "ask_required" || e.kind === "plan_required" || e.kind === "tasks_updated" || e.kind === "artifacts_updated") {
        void reloadRuns();
      }
    }).then((fn) => (off = fn));
    return () => off?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project.id]);

  // 进入「工作目录」Tab 时拉取文件列表（切项目时重置）。
  useEffect(() => {
    if (view !== "workspace") return;
    void reloadWorkspaceFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project.id, view]);

  /** 打开成员 Picker：懒加载可选专家后弹框。 */
  async function openMemberPicker() {
    try {
      if (agents.length === 0) setExperts(await listStandaloneExperts());
    } catch (err) {
      notify.notify({ tone: "error", title: "加载专家失败", message: String(err) });
    }
    setAdding(true);
  }

  /** Picker 确定：把新增（未在册）的专家加入项目；职业由后端按 profession 回填。 */
  async function addMembers(names: string[]) {
    setAdding(false);
    const existing = new Set(members.map((m) => m.expertName));
    const toAdd = names.filter((n) => !existing.has(n));
    if (toAdd.length === 0) return;
    try {
      for (const expertName of toAdd) await addProjectMember({ projectId: project.id, expertName });
      await Promise.all([reloadMembers(), reloadProjectSkills()]);
    } catch (err) {
      notify.notify({ tone: "error", title: "添加成员失败", message: String(err) });
    }
  }

  /** 打开「从团队导入」：懒加载团队列表后弹框。 */
  async function openTeamImport() {
    try {
      if (teams.length === 0) setTeams(await listTeams());
    } catch (err) {
      notify.notify({ tone: "error", title: "加载团队失败", message: String(err) });
    }
    setTeamImportOpen(true);
  }

  /** 选定团队后：二次确认 → 清空现有成员 → 导入该团队成员（职业由后端按 profession 回填）。 */
  async function importFromTeam(teamId: string) {
    setTeamImportOpen(false);
    try {
      const d = await getTeamDetail(teamId);
      const names = d.members.map((m) => m.name);
      const teamName = teams.find((t) => t.id === teamId)?.displayName || teams.find((t) => t.id === teamId)?.name || "该团队";
      if (names.length === 0) {
        notify.notify({ tone: "error", title: "导入失败", message: "该团队没有成员" });
        return;
      }
      const confirmed = await messages.confirm({
        title: "导入团队成员",
        message: `从团队「${teamName}」导入 ${names.length} 名成员（复制为项目私有副本）？这会清空当前 ${members.length} 名成员并替换。`,
        tone: "warning",
        confirmText: "导入并替换",
      });
      if (!confirmed) return;
      for (const m of members) await removeProjectMember(m.id);
      for (const expertName of names) await importTeamMember(project.id, teamId, expertName);
      await Promise.all([reloadMembers(), reloadProjectSkills()]);
    } catch (err) {
      notify.notify({ tone: "error", title: "导入团队失败", message: String(err) });
    }
  }

  /** 新建项目会话先进入草稿；点击发送时再创建真实 session。 */
  function newProjectSession() {
    enterDraftWithProject(project.id);
  }
  async function renameProjectSession(id: string, title: string) {
    if (!title.trim()) return;
    await renameSession(id, title.trim()).catch((e) => notify.notify({ tone: "error", title: "重命名失败", message: String(e) }));
    await reloadProjectSessions();
  }
  async function deleteProjectSession(id: string) {
    await deleteSession(id).catch((e) => notify.notify({ tone: "error", title: "删除失败", message: String(e) }));
    await reloadProjectSessions();
  }

  const artifactCountByTaskId = useMemo(
    () => buildArtifactCountByTaskId(tasks, artifacts),
    [artifacts, tasks],
  );

  function openTaskSession(task: ProjectTask) {
    const sessionId = task.runSessionId || task.threadSessionId;
    if (sessionId) openSession(sessionId);
  }

  return (
    <div className="flex h-full flex-col text-sm">
      {/* 一级导航：返回 + 项目名（对齐智能体详情标题栏） */}
      <div className="flex items-center gap-2 border-b border-border-subtle px-4 pt-2.5 pb-1.5 session-header">
        <button type="button" onClick={onBack} className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-accent-foreground">
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
        </button>
        <span className="min-w-0 flex-1 truncate text-[15px] font-semibold text-foreground">{project.name}</span>
      </div>
      {/* 常驻概况区：项目概况（名称/描述/工作目录）+ 编辑 */}
      <div className="border-b border-border-subtle px-6 py-3">
        <ProjectOverviewPanel
          project={project}
          onOpenWorkspace={() => void openProjectWorkspace(project.id).catch(() => {})}
          onReload={onReload}
        />
      </div>

      {/* Tab 栏：各功能区（本地状态切换，不接入路由） */}
      <div className="overflow-x-auto border-b border-border-subtle px-4 py-2">
        <Tabs items={PROJECT_TABS} value={view} onChange={setView} />
      </div>

      <div className="min-h-0 flex-1 overflow-hidden">
        {view === "instructions" ? (
          <ProjectInstructionsPanel project={project} onReload={onReload} />
        ) : view === "board" ? (
          <ProjectTaskBoard
            tasks={tasks}
            artifactCountByTaskId={artifactCountByTaskId}
            onOpen={openTaskSession}
            onOpenArtifacts={() => setView("artifacts")}
          />
        ) : view === "artifacts" ? (
          <ProjectArtifactList project={project} onOpenSession={openSession} />
        ) : view === "workspace" ? (
          <WorkspaceBrowser
            workspaceLabel={workspaceBaseName(project.workspaceDir)}
            workspacePath={project.workspaceDir ?? undefined}
            files={workspaceFiles}
            loading={workspaceLoading}
            error={workspaceError}
            truncated={workspaceFiles.length >= 500}
            onOpenDir={() => void openProjectWorkspace(project.id).catch(() => {})}
            onRefresh={() => void reloadWorkspaceFiles()}
            readFile={(rel) => readProjectWorkspaceFile(project.id, rel)}
            onOpenFile={(rel) => void openProjectWorkspaceFile(project.id, rel).catch(() => {})}
          />
        ) : view === "chat" ? (
          <ProjectSessionList sessions={projectSessions} onOpen={openSession} onNew={newProjectSession} onRename={renameProjectSession} onDelete={deleteProjectSession} />
        ) : view === "memory" ? (
          <div className="h-full overflow-auto p-6">
            <div className="mx-auto max-w-[860px]">
              <MemorySection scope={{ kind: "project", id: project.id, label: project.name }} />
            </div>
          </div>
        ) : view === "knowledge" ? (
          <div className="h-full overflow-auto p-6">
            <div className="mx-auto max-w-[860px]">
              <KnowledgeScopeCard scopeType="project" scopeId={project.id} />
            </div>
          </div>
        ) : view === "scheduling" ? (
          <ScopedScheduledTaskList
            projectId={project.id}
            label={project.name}
            onNewTask={() => onNewScheduledTask(project.id)}
            onOpenTask={onOpenScheduledTask}
          />
        ) : view === "skills" ? (
          <ProjectSkillList skills={projectSkills} />
        ) : view === "members" ? (
          <ProjectMembersPanel
            members={members}
            runs={runs}
            tasks={tasks}
            onAddMember={() => void openMemberPicker()}
            onImportTeam={() => void openTeamImport()}
            onRemoveMember={(id) => void removeProjectMember(id).then(() => Promise.all([reloadMembers(), reloadProjectSkills()]))}
            onOpenSession={openSession}
          />
        ) : (
          <ProjectUsage projectId={project.id} onOpenSession={openSession} />
        )}
      </div>

      {adding && (
        <MemberPickerDialog
          agents={agents}
          initial={members.map((m) => m.expertName)}
          onClose={() => setAdding(false)}
          onConfirm={(names) => void addMembers(names)}
        />
      )}
      {teamImportOpen && (
        <TeamPickerDialog
          teams={teams}
          onClose={() => setTeamImportOpen(false)}
          onPick={(teamId) => void importFromTeam(teamId)}
        />
      )}
    </div>
  );
}

/** 项目会话列表：点击进入真正的 SessionPage；可新建/重命名/删除。 */
function ProjectSessionList({ sessions, onOpen, onNew, onRename, onDelete }: {
  sessions: SessionInfo[];
  onOpen: (id: string) => void;
  onNew: () => void;
  onRename: (id: string, title: string) => void;
  onDelete: (id: string) => void;
}) {
  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-semibold text-foreground">会话 {sessions.length}</h3>
          <Button tone="primary" onClick={onNew}><Plus className="h-4 w-4" aria-hidden="true" /> 新建会话</Button>
        </div>
        {sessions.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">还没有会话。新建一个，开始和项目团队协作。</p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {sessions.map((session) => (
              <li key={session.id} className="group flex items-center gap-2 rounded-lg border border-border-subtle bg-surface px-3 py-2.5 transition hover:border-border">
                <MessagesSquare className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />
                <button type="button" onClick={() => onOpen(session.id)} className="min-w-0 flex-1 truncate text-left text-[13px] font-medium text-foreground">{session.title}</button>
                <button type="button" title="重命名" onClick={() => { const v = window.prompt("会话名称", session.title); if (v) onRename(session.id, v); }} className="rounded px-1 py-1 text-foreground-muted opacity-0 transition hover:text-foreground group-hover:opacity-100"><Pencil className="h-3.5 w-3.5" aria-hidden="true" /></button>
                <button type="button" title="删除" onClick={() => { if (window.confirm(`删除会话「${session.title}」？`)) onDelete(session.id); }} className="rounded px-1 py-1 text-foreground-muted opacity-0 transition hover:text-destructive group-hover:opacity-100"><Trash2 className="h-3.5 w-3.5" aria-hidden="true" /></button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function ProjectSkillList({ skills }: { skills: ProjectSkill[] }) {
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-4">
          <h3 className="text-sm font-semibold text-foreground">专属技能 {skills.length}</h3>
          <p className="mt-1 text-xs text-foreground-muted">
            这些技能来自项目成员或导入团队，项目运行时会按需加载。
          </p>
        </div>
        {skills.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">
            这个项目还没有专属技能。
          </p>
        ) : (
          <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
            {skills.map((item, index) => {
              const sourceLabel = item.sourceKind === "team" ? "团队" : "专家";
              return (
                <li
                  key={item.skill.id}
                  className={index === skills.length - 1 ? "" : "border-b border-border-subtle"}
                >
                  <button
                    type="button"
                    onClick={() => setSelectedSkillId(item.skill.id)}
                    title="查看技能详情"
                    className="block w-full px-4 py-2.5 text-left transition-colors hover:bg-accent"
                  >
                    <div className="flex items-center gap-2">
                      <p className="truncate text-sm font-medium text-foreground">{item.skill.name}</p>
                      {!item.skill.enabled && <Badge tone="neutral">已禁用</Badge>}
                      <Badge tone={item.sourceKind === "team" ? "info" : "neutral"}>
                        {sourceLabel}
                      </Badge>
                      <Badge tone="neutral">{item.sourceName}</Badge>
                    </div>
                    {item.skill.description && (
                      <p className="mt-0.5 line-clamp-2 text-xs leading-5 text-foreground-muted [overflow-wrap:anywhere]">
                        {item.skill.description}
                      </p>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
      <SkillDetailDrawer skillId={selectedSkillId} onClose={() => setSelectedSkillId(null)} />
    </div>
  );
}

const USAGE_RANGES: { id: UsageRange; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "30d", label: "30天" },
  { id: "7d", label: "7天" },
];

/** 项目用量详情：总计卡片 + 按会话列表；点击会话行进入该会话。 */
function ProjectUsage({ projectId, onOpenSession }: { projectId: string; onOpenSession: (id: string) => void }) {
  const [range, setRange] = useState<UsageRange>("all");
  const [data, setData] = useState<ScopedUsageView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    getProjectUsage(projectId, range)
      .then((v) => { if (!cancelled) setData(v); })
      .catch((e) => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, [projectId, range]);

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-lg font-semibold tabular-nums text-foreground">
              {data == null ? "—" : `${formatTokens(data.totals.total)} tokens`}
            </div>
            <div className="text-[11px] text-foreground-muted">本项目累计 token 用量（点击会话看明细）</div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {USAGE_RANGES.map((r) => (
              <button
                key={r.id}
                type="button"
                onClick={() => setRange(r.id)}
                className={`rounded-lg px-3 py-1.5 text-sm transition ${range === r.id ? "bg-accent font-semibold text-foreground" : "text-foreground-secondary hover:bg-accent hover:text-foreground"}`}
              >
                {r.label}
              </button>
            ))}
          </div>
        </div>
        {error && <p className="text-sm text-destructive">加载失败：{error}</p>}
        {data && (
          <>
            {data.bySession.length === 0 ? (
              <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">该范围内暂无用量。</p>
            ) : (
              <ul className="flex flex-col gap-1.5">
                {data.bySession.map((s) => (
                  <ProjectUsageSessionRow key={s.sessionId} row={s} onOpenSession={onOpenSession} />
                ))}
              </ul>
            )}
          </>
        )}
      </div>
    </div>
  );
}

/** 会话→消息二层行：点击会话标题展开，懒加载该会话的按消息用量；点击消息行进入会话。 */
function ProjectUsageSessionRow({ row, onOpenSession }: { row: UsageSessionRow; onOpenSession: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const [msgs, setMsgs] = useState<UsageMessageRow[] | null>(null);
  const [loading, setLoading] = useState(false);

  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next && msgs == null && !loading) {
      setLoading(true);
      getSessionMessageUsage(row.sessionId)
        .then((rows) => setMsgs(rows))
        .catch(() => setMsgs([]))
        .finally(() => setLoading(false));
    }
  };

  return (
    <li className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
      <button
        type="button"
        onClick={toggle}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left transition hover:bg-accent/40"
        aria-expanded={open}
      >
        {open ? <ChevronDown className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" /> : <ChevronRight className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />}
        <span className="min-w-0 flex-1 truncate text-foreground">{sessionLabel(row.sessionId, row.title)}</span>
        <span className="shrink-0 tabular-nums text-foreground-secondary">{formatTokens(row.total)}</span>
      </button>
      {open && (
        <div className="border-t border-border-subtle bg-background px-2 py-1.5">
          {loading && <p className="px-2 py-2 text-xs text-foreground-muted">加载中…</p>}
          {msgs && msgs.length === 0 && <p className="px-2 py-2 text-xs text-foreground-muted">无消息级用量。</p>}
          {msgs && msgs.length > 0 && (
            <ul className="flex flex-col gap-0.5">
              {msgs.map((m) => (
                <li key={m.messageId}>
                  <button
                    type="button"
                    onClick={() => onOpenSession(row.sessionId)}
                    title="进入会话"
                    className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-accent/40"
                  >
                    <span className="min-w-0 flex-1 truncate text-[12px] text-foreground-secondary">
                      {m.snippet.trim() || `${m.role || "消息"} · ${formatTs(m.ts)}`}
                    </span>
                    <span className="shrink-0 tabular-nums text-[12px] text-foreground-muted">{formatTokens(m.total)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </li>
  );
}

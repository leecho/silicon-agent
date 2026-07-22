import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Pencil } from "lucide-react";
import {
  deleteAgent,
  deleteSession,
  listAgentSkills,
  listSoulVersions,
  listAgentArtifacts,
  listAgentSessions,
  listAgentTasks,
  listAgentWorkspaceFiles,
  listStandaloneExperts,
  openAgentWorkspace,
  openAgentWorkspaceFile,
  readAgentWorkspaceFile,
  renameSession,
  setEvolutionEnabled,
  subscribeAgentStreamEvents,
} from "../../api";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { useSession } from "../../components/session/SessionProvider";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Agent, ExpertSummary, ProjectArtifact, ProjectSkill, ProjectTask, SessionInfo, UsageTotals } from "../../types";
import { ProjectTaskBoard } from "../projects/ProjectTaskBoard";
import { AgentArtifactList } from "./AgentArtifactList";
import { AgentOverviewPanel, AgentIdentityPanel } from "./AgentOverview";
import { AgentSessionList } from "./AgentSessionList";
import { AgentUsage } from "./AgentUsage";
import { Tabs } from "../../components/ui/Tabs";
import { Tooltip } from "../../components/ui/Tooltip";
import { WorkspaceBrowser } from "../../components/session/WorkspaceBrowser";
import { KnowledgeScopeCard } from "../knowledge-bases/KnowledgeScopeCard";
import { AgentEvolutionDrawer, AgentIdentityAnchorEditDrawer, AgentIdentityAnchorViewDrawer, AgentIdentityEditDrawer, AgentInstructionsViewDrawer, AgentSoulEditDrawer, AgentSourceExpertSwitchDrawer } from "./AgentViewDrawers";
import { MemorySection } from "../settings/sections/MemorySection";
import { ScopedScheduledTaskList } from "../../components/scheduling/ScopedScheduledTaskList";
import { SkillDetailDrawer } from "../skills/SkillDetailDrawer";

type AgentViewMode = "identity" | "tasks" | "artifacts" | "chat" | "usage" | "memory" | "knowledge" | "scheduling" | "skills" | "workspace";

const AGENT_TABS: { value: AgentViewMode; label: string }[] = [
  { value: "identity", label: "身份人格" },
  { value: "tasks", label: "任务" },
  { value: "artifacts", label: "产物" },
  { value: "workspace", label: "工作目录" },
  { value: "usage", label: "用量" },
  { value: "memory", label: "记忆" },
  { value: "knowledge", label: "知识" },
  { value: "skills", label: "技能" },
  { value: "scheduling", label: "定时任务" },
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

/** 智能体详情：ProjectView 同构的工作台，按智能体维度聚合任务、产物、会话和用量。 */
export function AgentView({
  agent,
  onBack,
  onNewScheduledTask,
  onOpenScheduledTask,
  onReload,
}: {
  agent: Agent;
  onBack: () => void;
  onNewScheduledTask: (agentId: string) => void;
  onOpenScheduledTask: (taskId: string) => void;
  onReload: () => void;
}) {
  const notify = useNotifications();
  const messages = useMessages();
  const { enterDraftWithAgent, openSession } = useSession();
  const [view, setView] = useState<AgentViewMode>("identity");
  const [identityOpen, setIdentityOpen] = useState(false);
  const [identityAnchorViewOpen, setIdentityAnchorViewOpen] = useState(false);
  const [instructionsOpen, setInstructionsOpen] = useState(false);
  const [identityAnchorEditOpen, setIdentityAnchorEditOpen] = useState(false);
  const [soulEditOpen, setSoulEditOpen] = useState(false);
  const [evolutionOpen, setEvolutionOpen] = useState(false);
  const [sourceExpertOpen, setSourceExpertOpen] = useState(false);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [tasks, setTasks] = useState<ProjectTask[]>([]);
  const [artifacts, setArtifacts] = useState<ProjectArtifact[]>([]);
  const [agentSkills, setAgentSkills] = useState<ProjectSkill[]>([]);
  const [sourceExperts, setSourceExperts] = useState<ExpertSummary[]>([]);
  const [pendingSoulProposalCount, setPendingSoulProposalCount] = useState(0);
  const [workspaceFiles, setWorkspaceFiles] = useState<string[]>([]);
  const [workspaceLoading, setWorkspaceLoading] = useState(false);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);

  /** 拉取智能体工作目录文件列表（进入「工作目录」Tab 或手动刷新时）。 */
  async function reloadWorkspaceFiles() {
    setWorkspaceLoading(true);
    setWorkspaceError(null);
    try {
      setWorkspaceFiles(await listAgentWorkspaceFiles(agent.id));
    } catch (err) {
      setWorkspaceError(String(err));
    } finally {
      setWorkspaceLoading(false);
    }
  }

  async function reloadSessions() {
    try {
      setSessions(await listAgentSessions(agent.id));
    } catch (err) {
      notify.notify({ tone: "error", title: "加载会话失败", message: String(err) });
    }
  }

  async function reloadWork() {
    try {
      const [nextTasks, nextArtifacts] = await Promise.all([
        listAgentTasks(agent.id),
        listAgentArtifacts(agent.id),
      ]);
      setTasks(nextTasks);
      setArtifacts(nextArtifacts);
    } catch {
      /* Agent 聚合投影失败不打扰基础详情。 */
    }
  }

  async function reloadAgentSkills() {
    try {
      setAgentSkills(await listAgentSkills(agent.id));
    } catch {
      setAgentSkills([]);
    }
  }

  async function reloadSourceExperts() {
    try {
      setSourceExperts(await listStandaloneExperts());
    } catch {
      setSourceExperts([]);
    }
  }

  async function reloadPendingSoulProposals() {
    try {
      const versions = await listSoulVersions(agent.id);
      setPendingSoulProposalCount(versions.filter((v) => v.status === "pending").length);
    } catch {
      setPendingSoulProposalCount(0);
    }
  }

  useEffect(() => {
    void reloadSessions();
    void reloadWork();
    void reloadAgentSkills();
    void reloadSourceExperts();
    let off: (() => void) | undefined;
    void subscribeAgentStreamEvents((event) => {
      if (
        event.kind === "run_started" ||
        event.kind === "run_finished" ||
        event.kind === "tool_result" ||
        event.kind === "tasks_updated" ||
        event.kind === "artifacts_updated"
      ) {
        void reloadSessions();
        void reloadWork();
      }
    }).then((fn) => (off = fn));
    return () => off?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent.id]);

  useEffect(() => {
    if (view !== "identity") return;
    let cancelled = false;
    listSoulVersions(agent.id)
      .then((versions) => {
        if (!cancelled) setPendingSoulProposalCount(versions.filter((v) => v.status === "pending").length);
      })
      .catch(() => {
        if (!cancelled) setPendingSoulProposalCount(0);
      });
    return () => { cancelled = true; };
  }, [agent.id, view]);

  // 进入「工作目录」Tab 时拉取文件列表（切智能体时重置）。
  useEffect(() => {
    if (view !== "workspace") return;
    void reloadWorkspaceFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent.id, view]);

  const artifactCountByTaskId = useMemo(() => buildArtifactCountByTaskId(tasks, artifacts), [artifacts, tasks]);
  const sourceExpertDisplayName = useMemo(() => {
    const sourceId = agent.sourceExpertId?.trim();
    if (!sourceId) return "未绑定";
    const expert = sourceExperts.find((item) => item.name === sourceId || item.id === sourceId);
    return expert?.displayName || expert?.name || sourceId;
  }, [agent.sourceExpertId, sourceExperts]);

  function newAgentSession() {
    enterDraftWithAgent(agent.id);
  }

  function openTaskSession(task: ProjectTask) {
    const sessionId = task.runSessionId || task.threadSessionId;
    if (sessionId) openSession(sessionId);
  }

  async function renameAgentSession(id: string, title: string) {
    if (!title.trim()) return;
    await renameSession(id, title.trim()).catch((err) => notify.notify({ tone: "error", title: "重命名失败", message: String(err) }));
    await reloadSessions();
  }

  async function deleteAgentSession(id: string) {
    await deleteSession(id).catch((err) => notify.notify({ tone: "error", title: "删除失败", message: String(err) }));
    await reloadSessions();
  }

  async function handleDelete() {
    const ok = await messages.confirm({
      title: "删除智能体",
      message: `确定删除「${agent.displayName || agent.name}」吗？它的私有记忆会一并删除（历史会话保留）。操作不可撤销。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteAgent(agent.id);
      onBack();
      onReload();
    } catch (err) {
      notify.notify({ tone: "error", title: "删除失败", message: String(err) });
    }
  }

  async function handleOpenWorkspace() {
    try {
      await openAgentWorkspace(agent.id);
    } catch (err) {
      notify.notify({ tone: "error", title: "打开工作目录失败", message: String(err) });
    }
  }

  async function handleSetEvolutionEnabled(enabled: boolean) {
    try {
      await setEvolutionEnabled(agent.id, enabled);
      onReload();
    } catch (err) {
      notify.notify({ tone: "error", title: "保存失败", message: String(err) });
    }
  }

  return (
    <div className="flex h-full flex-col text-sm">
      {/* 一级导航：返回 + 智能体名（对齐项目详情标题栏） */}
      <div className="flex items-center gap-2 border-b border-border-subtle px-4 pt-2.5 pb-1.5 session-header">
        <button type="button" onClick={onBack} className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-accent-foreground">
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
        </button>
        <span className="min-w-0 flex-1 truncate text-[15px] font-semibold text-foreground">{agent.displayName || agent.name}</span>
      </div>
      {/* 常驻概况区：头像/名称/职业/工作目录 + 编辑身份 */}
      <div className="border-b border-border-subtle px-6 py-3">
        <AgentOverviewPanel
          agent={agent}
          onOpenWorkspace={handleOpenWorkspace}
          onEditIdentity={() => setIdentityOpen(true)}
        />
      </div>
      {/* Tab 栏：各功能区（本地状态切换，不接入路由） */}
      <div className="overflow-x-auto border-b border-border-subtle px-4 py-2">
        <Tabs items={AGENT_TABS} value={view} onChange={setView} />
      </div>

      <div className="min-h-0 flex-1 overflow-hidden">
        {view === "identity" ? (
          <AgentIdentityPanel
            agent={agent}
            pendingSoulProposalCount={pendingSoulProposalCount}
            onSetEvolutionEnabled={handleSetEvolutionEnabled}
            onEditIdentityAnchor={() => setIdentityAnchorEditOpen(true)}
            onViewIdentityAnchor={() => setIdentityAnchorViewOpen(true)}
            onEditSoul={() => setSoulEditOpen(true)}
            onViewInstructions={() => setInstructionsOpen(true)}
            onOpenEvolution={() => setEvolutionOpen(true)}
          />
        ) : view === "tasks" ? (
          <ProjectTaskBoard
            tasks={tasks}
            artifactCountByTaskId={artifactCountByTaskId}
            onOpen={openTaskSession}
            onOpenArtifacts={() => setView("artifacts")}
          />
        ) : view === "artifacts" ? (
          <AgentArtifactList agent={agent} artifacts={artifacts} onOpenSession={openSession} onReload={() => void reloadWork()} />
        ) : view === "workspace" ? (
          <WorkspaceBrowser
            workspaceLabel={workspaceBaseName(agent.workingDir)}
            workspacePath={agent.workingDir ?? undefined}
            files={workspaceFiles}
            loading={workspaceLoading}
            error={workspaceError}
            truncated={workspaceFiles.length >= 500}
            onOpenDir={handleOpenWorkspace}
            onRefresh={() => void reloadWorkspaceFiles()}
            readFile={(rel) => readAgentWorkspaceFile(agent.id, rel)}
            onOpenFile={(rel) => void openAgentWorkspaceFile(agent.id, rel).catch(() => {})}
          />
        ) : view === "chat" ? (
          <AgentSessionList sessions={sessions} onDelete={deleteAgentSession} onNew={newAgentSession} onOpen={openSession} onRename={renameAgentSession} />
        ) : view === "memory" ? (
          <div className="h-full overflow-auto p-6">
            <div className="mx-auto max-w-[860px]">
              <MemorySection scope={{ kind: "agent", id: agent.id, label: agent.displayName || agent.name }} />
            </div>
          </div>
        ) : view === "knowledge" ? (
          <div className="h-full overflow-auto p-6">
            <div className="mx-auto max-w-[860px]">
              <KnowledgeScopeCard scopeType="agent" scopeId={agent.id} />
            </div>
          </div>
        ) : view === "scheduling" ? (
          <ScopedScheduledTaskList
            agentId={agent.id}
            label={agent.displayName || agent.name}
            onNewTask={() => onNewScheduledTask(agent.id)}
            onOpenTask={onOpenScheduledTask}
          />
        ) : view === "skills" ? (
          <AgentSkillList
            skills={agentSkills}
            sourceExpertDisplayName={sourceExpertDisplayName}
            onSwitchSourceExpert={() => setSourceExpertOpen(true)}
          />
        ) : (
          <AgentUsage agentId={agent.id} onOpenSession={openSession} />
        )}
      </div>

      <AgentIdentityEditDrawer agent={agent} open={identityOpen} onClose={() => setIdentityOpen(false)} onSaved={() => { setIdentityOpen(false); onReload(); }} />
      <AgentIdentityAnchorViewDrawer agent={agent} open={identityAnchorViewOpen} onClose={() => setIdentityAnchorViewOpen(false)} />
      <AgentInstructionsViewDrawer agent={agent} open={instructionsOpen} onClose={() => setInstructionsOpen(false)} />
      <AgentIdentityAnchorEditDrawer agent={agent} open={identityAnchorEditOpen} onClose={() => setIdentityAnchorEditOpen(false)} onSaved={() => { setIdentityAnchorEditOpen(false); onReload(); }} />
      <AgentSoulEditDrawer agent={agent} open={soulEditOpen} onClose={() => setSoulEditOpen(false)} onSaved={() => { setSoulEditOpen(false); onReload(); }} />
      <AgentEvolutionDrawer agent={agent} open={evolutionOpen} onClose={() => setEvolutionOpen(false)} onSaved={() => { onReload(); void reloadPendingSoulProposals(); }} />
      <AgentSourceExpertSwitchDrawer agent={agent} experts={sourceExperts.filter((item) => item.enabled)} open={sourceExpertOpen} onClose={() => setSourceExpertOpen(false)} onSaved={() => { setSourceExpertOpen(false); onReload(); void reloadAgentSkills(); void reloadSourceExperts(); }} />
    </div>
  );
}

function AgentSkillList({ skills, sourceExpertDisplayName, onSwitchSourceExpert }: {
  skills: ProjectSkill[];
  sourceExpertDisplayName: string;
  onSwitchSourceExpert: () => void;
}) {
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-4 flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-foreground">专属技能 {skills.length}</h3>
            <p className="mt-1 text-xs text-foreground-muted">
              这些技能来自源专家，智能体运行时会按需加载。
            </p>
          </div>
          <Tooltip content="点击切换专家">
            <button type="button" onClick={onSwitchSourceExpert} className="flex min-w-0 shrink-0 items-center gap-1 text-[12px] text-primary hover:text-foreground">
              <span className="min-w-0 truncate">专家：{sourceExpertDisplayName}</span>
              <Pencil className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
            </button>
          </Tooltip>
        </div>
        {skills.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">
            这个智能体还没有专属技能。
          </p>
        ) : (
          <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
            {skills.map((item, index) => (
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
                    <Badge tone="neutral">专家</Badge>
                    <Badge tone="neutral">{item.sourceName}</Badge>
                  </div>
                  {item.skill.description && (
                    <p className="mt-0.5 line-clamp-2 text-xs leading-5 text-foreground-muted [overflow-wrap:anywhere]">
                      {item.skill.description}
                    </p>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <SkillDetailDrawer skillId={selectedSkillId} onClose={() => setSelectedSkillId(null)} />
    </div>
  );
}

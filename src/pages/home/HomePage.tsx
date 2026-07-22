import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  Bot,
  File,
  FileEdit,
  FolderKanban,
  MessageSquare,
  Play,
  Settings,
} from "lucide-react";

import {
  attachFile,
  createSession,
  deleteSession,
  getGlobalPermissionMode,
  getRecentWorkspaces,
  listAgents,
  listActiveTeams,
  listExperts,
  listEnabledModels,
  listProjects,
  listSessions,
  pickDirectory,
  pickFile,
  saveAttachment,
  setDraftContent,
  setSessionRole,
  setSessionMode,
  setSessionModel,
  setSessionPermissionMode,
  setSessionWorkspace,
  submitProjectDraftMessage,
  submitUserMessage,
} from "../../api";
import { Composer } from "../../components/session/Composer";
import { useSession } from "../../components/session/SessionProvider";
import { Button } from "../../components/ui";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { extractAttachments } from "../../lib/attachments";
import { avatarEmoji } from "../../lib/avatar";
import { hasEnabledModels } from "../../lib/modelAvailability";
import type {
  Agent,
  ExpertSummary,
  EnabledProviderModels,
  PermissionMode,
  Project,
  SessionInfo,
  Team,
} from "../../types";

const HOME_SECTION_LIMIT = 5;

function baseName(p: string): string {
  const t = p.replace(/[/\\]+$/, "");
  const i = Math.max(t.lastIndexOf("/"), t.lastIndexOf("\\"));
  return i >= 0 ? t.slice(i + 1) : t;
}

function formatUpdatedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function HomePage({
  onOpenAgent,
  onOpenAgents,
  onOpenProject,
  onOpenProjects,
  onOpenSettings,
}: {
  onOpenAgent: (agentId: string) => void;
  onOpenAgents: () => void;
  onOpenProject: (projectId: string) => void;
  onOpenProjects: () => void;
  onOpenSettings: () => void;
}) {
  const {
    materializeDraft,
    openDraft,
    openSession,
    refreshSessions,
  } = useSession();
  const notify = useNotifications();

  const sessionIdRef = useRef<string | null>(null);
  const createdHereRef = useRef(false);
  const submittedRef = useRef(false);
  const latestContentRef = useRef("");

  const [modelGroups, setModelGroups] = useState<EnabledProviderModels[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);
  const [roleExperts, setRoleExperts] = useState<ExpertSummary[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [recents, setRecents] = useState<string[]>([]);
  const [overviewLoading, setOverviewLoading] = useState(true);
  const [draftSession, setDraftSession] = useState<SessionInfo | null>(null);
  const [draftProjectId, setDraftProjectId] = useState<string | null>(null);
  const [draftSelectedModelId, setDraftSelectedModelId] = useState<string | null>(null);
  const [draftPermissionMode, setDraftPermissionMode] = useState<PermissionMode | null>(null);
  const [draftModeValue, setDraftModeValue] = useState<"normal" | "plan">("normal");
  const [globalPermMode, setGlobalPermMode] = useState<PermissionMode>("manual");

  async function reloadOverview() {
    setOverviewLoading(true);
    try {
      const [
        modelResult,
        teamResult,
        expertResult,
        agentResult,
        projectResult,
        sessionResult,
        workspaceResult,
        permissionResult,
      ] = await Promise.allSettled([
        listEnabledModels(),
        listActiveTeams(),
        listExperts(),
        listAgents(),
        listProjects(),
        listSessions(),
        getRecentWorkspaces(),
        getGlobalPermissionMode(),
      ]);

      const nextModels = modelResult.status === "fulfilled" ? modelResult.value : [];
      const nextTeams = teamResult.status === "fulfilled" ? teamResult.value : [];
      const nextExperts = expertResult.status === "fulfilled" ? expertResult.value : [];
      const nextAgents = agentResult.status === "fulfilled" ? agentResult.value : [];
      const nextProjects = projectResult.status === "fulfilled" ? projectResult.value : [];
      const nextSessions = sessionResult.status === "fulfilled" ? sessionResult.value : [];

      setModelGroups(nextModels);
      setTeams(nextTeams);
      setRoleExperts(nextExperts);
      setAgents(nextAgents);
      setProjects(nextProjects);
      setSessions(nextSessions);
      setRecents(workspaceResult.status === "fulfilled" ? workspaceResult.value : []);
      if (permissionResult.status === "fulfilled") setGlobalPermMode(permissionResult.value);
    } catch (err) {
      console.error(err);
      notify.error("加载首页失败：" + String(err));
    } finally {
      setOverviewLoading(false);
    }
  }

  useEffect(() => {
    void reloadOverview();
    const reload = () => void reloadOverview();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    return () => {
      if (submittedRef.current) return;
      const id = sessionIdRef.current;
      const content = latestContentRef.current.trim();
      if (content) {
        void (async () => {
          let realId = id;
          if (!realId) {
            try {
              realId = (await createSession(true)).id;
            } catch (err) {
              console.error(err);
              return;
            }
          }
          await setDraftContent(realId, latestContentRef.current).catch(console.error);
          refreshSessions();
        })();
        return;
      }
      if (id && createdHereRef.current) {
        void deleteSession(id)
          .then(() => refreshSessions())
          .catch(console.error);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const ensureDraftSession = async (): Promise<string | null> => {
    if (sessionIdRef.current) return sessionIdRef.current;
    try {
      const created = await createSession(true);
      sessionIdRef.current = created.id;
      createdHereRef.current = true;
      setDraftSession(created);
      materializeDraft(created.id);
      return created.id;
    } catch (err) {
      console.error(err);
      notify.error("创建草稿失败：" + String(err));
      return null;
    }
  };

  const onAttachFile = async (): Promise<string | null> => {
    const id = await ensureDraftSession();
    if (!id) return null;
    try {
      const src = await pickFile();
      if (!src) return null;
      return await attachFile(id, src);
    } catch (err) {
      console.error(err);
      notify.error("添加附件失败：" + String(err));
      return null;
    }
  };

  const onPasteFile = async (file: File): Promise<string | null> => {
    const id = await ensureDraftSession();
    if (!id) return null;
    try {
      const buf = await file.arrayBuffer();
      const data = Array.from(new Uint8Array(buf));
      const ext = file.type.split("/")[1] || "bin";
      const name = file.name || `pasted.${ext}`;
      return await saveAttachment(id, name, data);
    } catch (err) {
      console.error(err);
      notify.error("保存附件失败：" + String(err));
      return null;
    }
  };

  const pickModel = async (modelId: string | null) => {
    if (draftProjectId && !sessionIdRef.current) {
      setDraftSelectedModelId(modelId);
      return;
    }
    const id = await ensureDraftSession();
    if (!id) return;
    try {
      await setSessionModel(id, modelId);
      setDraftSession((prev) => (prev ? { ...prev, selectedModelId: modelId } : prev));
    } catch (err) {
      console.error(err);
      notify.error("设置模型失败：" + String(err));
    }
  };

  const pickRole = async (kind: string, id: string) => {
    const sid = await ensureDraftSession();
    if (!sid) return;
    try {
      await setSessionRole(sid, kind, id);
      setDraftSession((prev) =>
        prev
          ? {
              ...prev,
              roleKind: (kind || null) as "expert" | "team" | null,
              roleId: id || null,
            }
          : prev,
      );
    } catch (err) {
      console.error(err);
      notify.error("设置角色失败：" + String(err));
    }
  };

  const switchPermissionMode = async (mode: PermissionMode | null) => {
    if (draftProjectId && !sessionIdRef.current) {
      setDraftPermissionMode(mode);
      return;
    }
    const id = await ensureDraftSession();
    if (!id) return;
    try {
      const next = await setSessionPermissionMode(id, mode);
      setDraftSession(next.session);
    } catch (err) {
      console.error(err);
      notify.error("切换权限模式失败");
    }
  };

  const togglePlan = async () => {
    if (draftProjectId && !sessionIdRef.current) {
      setDraftModeValue((current) => (current === "plan" ? "normal" : "plan"));
      return;
    }
    const id = await ensureDraftSession();
    if (!id) return;
    const nextMode = (draftSession?.mode ?? draftModeValue) === "plan" ? "normal" : "plan";
    try {
      await setSessionMode(id, nextMode);
      setDraftSession((prev) => (prev ? { ...prev, mode: nextMode } : prev));
      setDraftModeValue(nextMode);
    } catch (err) {
      console.error(err);
      notify.error("切换计划模式失败");
    }
  };

  const pickWorkspace = async () => {
    const dir = await pickDirectory();
    if (!dir) return;
    setDraftProjectId(null);
    const id = await ensureDraftSession();
    if (!id) return;
    try {
      const next = await setSessionWorkspace(id, dir);
      setDraftSession(next.session);
      setRecents(await getRecentWorkspaces());
      notify.success("已设置工作目录");
    } catch (err) {
      console.error(err);
      notify.error("设置工作目录失败：" + String(err));
    }
  };

  const pickRecent = async (path: string) => {
    setDraftProjectId(null);
    const id = await ensureDraftSession();
    if (!id) return;
    try {
      const next = await setSessionWorkspace(id, path);
      setDraftSession(next.session);
      notify.success("已设置工作目录");
    } catch (err) {
      console.error(err);
      notify.error("设置工作目录失败：" + String(err));
    }
  };

  const onSubmit = async (text: string): Promise<void> => {
    if (!text.trim()) return;
    if (draftProjectId) {
      submittedRef.current = true;
      try {
        const projectSessionId = await submitProjectDraftMessage({
          projectId: draftProjectId,
          content: text,
          sourceDraftSessionId: sessionIdRef.current,
          mode: (draftSession?.mode ?? draftModeValue) === "plan" ? "plan" : null,
          permissionMode: draftSession?.permissionMode ?? draftPermissionMode,
          selectedModelId: draftSession?.selectedModelId ?? draftSelectedModelId,
        });
        refreshSessions();
        openSession(projectSessionId);
      } catch (err) {
        console.error(err);
        notify.error("发送失败：" + String(err));
        submittedRef.current = false;
        throw err;
      }
      return;
    }
    const id = await ensureDraftSession();
    if (!id) throw new Error("创建草稿失败");
    submittedRef.current = true;
    try {
      await submitUserMessage(id, text);
      refreshSessions();
      openSession(id);
    } catch (err) {
      console.error(err);
      notify.error("发送失败：" + String(err));
      submittedRef.current = false;
      throw err;
    }
  };

  const modelReady = hasEnabledModels(modelGroups);
  const drafts = sessions.filter((session) => session.isDraft).slice(0, 4);
  const homeAgents = agents.slice(0, HOME_SECTION_LIMIT);
  const homeProjects = projects.slice(0, HOME_SECTION_LIMIT);
  const recentSessions = sessions
    .filter((session) => (!session.origin || session.origin === "user") && !session.isDraft && !session.projectId)
    .slice(0, 5);
  const dDir = draftSession?.workingDir?.trim() || "";
  const dWsName = dDir ? baseName(dDir) : undefined;
  const effectiveMode = draftSession?.mode ?? draftModeValue;
  const effectivePermissionMode = draftSession?.permissionMode ?? draftPermissionMode;
  const effectiveSelectedModelId = draftSession?.selectedModelId ?? draftSelectedModelId;

  return (
    <div className="h-full overflow-auto bg-background p-6 pt-10 text-sm text-foreground">
      <div className="mx-auto flex min-h-full w-full max-w-[860px] flex-col gap-6">
        <section className="border-b border-dashed border-border pb-5 pt-4">
          <div className="mb-4 flex items-center justify-between gap-4">
            <div className="min-w-0">
              <h1 className="text-xl font-semibold text-foreground">今天要处理什么？</h1>
              <p className="mt-1 text-sm text-foreground-muted">
                直接说你想完成的事，选择专家、团队帮你完成任务。
              </p>
            </div>
            {!modelReady && (
              <Button tone="primary" onClick={onOpenSettings} className="shrink-0">
                <Settings className="h-4 w-4" aria-hidden="true" />
                配置模型
              </Button>
            )}
          </div>
            <Composer
              sessionId={sessionIdRef.current ?? ""}
              disabled={!modelReady}
              onSubmit={onSubmit}
              onDraftChange={(serialized) => {
                latestContentRef.current = serialized;
              }}
              onAttachFile={onAttachFile}
              onPasteFile={onPasteFile}
              projects={projects}
              selectedProjectId={draftProjectId}
              onPickProject={setDraftProjectId}
              workspaceName={dWsName}
              workspacePath={dDir || undefined}
              onPickWorkspace={pickWorkspace}
              recentWorkspaces={recents}
              onPickRecent={pickRecent}
              modelGroups={modelGroups}
              selectedModelId={effectiveSelectedModelId}
              onPickModel={(id) => void pickModel(id)}
              teams={teams}
              roleExperts={roleExperts}
              roleKind={draftProjectId ? "" : (draftSession?.roleKind ?? "")}
              roleId={draftProjectId ? "" : (draftSession?.roleId ?? "")}
              onPickRole={draftProjectId ? undefined : (k, i) => void pickRole(k, i)}
              planMode={effectiveMode === "plan"}
              onTogglePlan={() => void togglePlan()}
              permissionMode={effectivePermissionMode}
              globalPermMode={globalPermMode}
              onChangePermission={(m) => void switchPermissionMode(m)}
            />
        </section>

        <section className="grid gap-8 lg:grid-cols-2">
          <SectionList title="最近会话" loading={overviewLoading}>
            <SessionList
              emptyLabel="暂无最近会话"
              sessions={recentSessions}
              onOpen={(id) => openSession(id)}
            />
          </SectionList>
          <SectionList title="草稿" loading={overviewLoading}>
            <SessionList
              emptyLabel="暂无草稿"
              sessions={drafts}
              titleOf={(session) => draftTitle(session)}
              onOpen={(id) => openDraft(id)}
            />
          </SectionList>
        </section>

        <section className="grid gap-8 lg:grid-cols-2">
          <SectionList title="项目" actionLabel="查看全部" onAction={onOpenProjects} loading={overviewLoading}>
            <ProjectSummaryList projects={homeProjects} onOpenProject={onOpenProject} />
          </SectionList>
          <SectionList title="智能体" actionLabel="查看全部" onAction={onOpenAgents} loading={overviewLoading}>
            <AgentSummaryList agents={homeAgents} onOpenAgent={onOpenAgent} />
          </SectionList>
        </section>
      </div>
    </div>
  );
}

function draftTitle(session: SessionInfo): string {
  const parsed = extractAttachments(session.draftContent ?? "");
  const content = parsed.body.trim();
  return content ? content.slice(0, 40) : session.title || "未命名草稿";
}

function ProjectSummaryList({
  onOpenProject,
  projects,
}: {
  onOpenProject: (projectId: string) => void;
  projects: Project[];
}) {
  if (projects.length === 0) return <EmptyLine label="暂无项目" />;
  return (
    <div className="grid gap-1.5">
      {projects.map((project) => (
        <button
          key={project.id}
          type="button"
          onClick={() => onOpenProject(project.id)}
          className="flex min-w-0 items-center gap-2 rounded-md border border-border-subtle bg-background px-2.5 py-2 text-left transition hover:border-border hover:bg-accent"
        >
          <span className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-surface text-primary">
            <FolderKanban className="h-3.5 w-3.5" aria-hidden="true" />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13px] font-medium text-foreground">
              {project.name}
            </span>
            <span className="mt-0.5 block truncate text-[11px] text-foreground-muted">
              {project.description || project.workspaceDir || formatUpdatedAt(project.updatedAt)}
            </span>
          </span>
        </button>
      ))}
    </div>
  );
}

function AgentSummaryList({
  agents,
  onOpenAgent,
}: {
  agents: Agent[];
  onOpenAgent: (agentId: string) => void;
}) {
  if (agents.length === 0) return <EmptyLine label="暂无智能体" />;
  return (
    <div className="grid gap-1.5">
      {agents.map((agent) => {
        const emoji = avatarEmoji(agent.avatar);
        return (
          <button
            key={agent.id}
            type="button"
            onClick={() => onOpenAgent(agent.id)}
            className="flex min-w-0 items-center gap-2 rounded-md border border-border-subtle bg-background px-2.5 py-2 text-left transition hover:border-border hover:bg-accent"
          >
            <span
              className={`grid h-7 w-7 shrink-0 place-items-center rounded-md bg-surface text-[15px] ${
                agent.enabled ? "text-primary" : "text-foreground-muted"
              }`}
            >
              {emoji ? <span aria-hidden="true">{emoji}</span> : <Bot className="h-3.5 w-3.5" aria-hidden="true" />}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px] font-medium text-foreground">
                {agent.displayName || agent.name}
              </span>
              <span className="mt-0.5 block truncate text-[11px] text-foreground-muted">
                {agent.profession || agent.workingDir || formatUpdatedAt(agent.updatedAt)}
              </span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

function SectionList({
  actionLabel,
  children,
  loading,
  onAction,
  title,
}: {
  actionLabel?: string;
  children: ReactNode;
  loading?: boolean;
  onAction?: () => void;
  title: string;
}) {
  return (
    <section>
      <div className="mb-3 flex items-center justify-between gap-3">
        <h2 className="text-sm pl-2 font-semibold text-foreground">{title}</h2>
        {actionLabel && onAction && (
          <button
            type="button"
            onClick={onAction}
            className="text-xs font-medium text-primary hover:text-foreground"
          >
            {actionLabel}
          </button>
        )}
      </div>
      {loading ? <div className="h-24 animate-pulse rounded-lg bg-surface" /> : children}
    </section>
  );
}

function SessionList({
  emptyLabel,
  onOpen,
  sessions,
  subtitleOf,
  titleOf,
}: {
  emptyLabel: string;
  onOpen: (id: string) => void;
  sessions: SessionInfo[];
  subtitleOf?: (session: SessionInfo) => string;
  titleOf?: (session: SessionInfo) => string;
}) {
  if (sessions.length === 0) return <EmptyLine label={emptyLabel} />;
  return (
    <div className="grid gap-1.5">
      {sessions.map((session) => (
        <button
          key={session.id}
          type="button"
          onClick={() => onOpen(session.id)}
          className="flex min-w-0 items-center gap-2 rounded-md border border-border-subtle bg-background px-2.5 py-2 text-left transition hover:border-border hover:bg-accent"
        >
          <span className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-surface text-foreground-secondary">
            { session.isDraft ? <FileEdit className="h-3.5 w-3.5" aria-hidden="true"></FileEdit> : <MessageSquare className="h-3.5 w-3.5" aria-hidden="true" />}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13px] font-medium text-foreground">
              {titleOf?.(session) ?? session.title}
            </span>
            <span className="mt-0.5 block truncate text-[11px] text-foreground-muted">
              {subtitleOf?.(session) ?? formatUpdatedAt(session.updatedAt)}
            </span>
          </span>
          {session.isRunning && <Play className="h-3.5 w-3.5 shrink-0 text-primary" aria-hidden="true" />}
        </button>
      ))}
    </div>
  );
}

function EmptyLine({ label }: { label: string }) {
  return (
    <div className="rounded-lg bg-background px-3 py-6 text-center text-xs text-foreground-muted">
      {label}
    </div>
  );
}

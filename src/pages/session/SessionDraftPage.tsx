import { useEffect, useRef, useState } from "react";
import {
  attachFile,
  createSession,
  deleteSession,
  getGlobalPermissionMode,
  getRecentWorkspaces,
  getSession,
  listActiveTeams,
  listAgents,
  listExperts,
  listEnabledModels,
  listProjects,
  pickDirectory,
  pickFile,
  saveAttachment,
  setDraftContent,
  setSessionAgent,
  setSessionRole,
  setSessionMode,
  setSessionModel,
  setSessionPermissionMode,
  setSessionWorkspace,
  submitProjectDraftMessage,
  submitUserMessage,
} from "../../api";
import type {
  Agent,
  ExpertSummary,
  Team,
  EnabledProviderModels,
  PermissionMode,
  Project,
  SessionInfo,
} from "../../types";
import { Composer } from "../../components/session/Composer";
import { useSession } from "../../components/session/SessionProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { PopularExpertsBar } from "./PopularExpertsBar";
import {
  extractAttachments,
  type Attachment,
} from "../../lib/attachments";

function baseName(p: string): string {
  const t = p.replace(/[/\\]+$/, "");
  const i = Math.max(t.lastIndexOf("/"), t.lastIndexOf("\\"));
  return i >= 0 ? t.slice(i + 1) : t;
}

// 草稿页：欢迎语 + Composer。草稿不立即建会话——
// - 纯文字：仅在「离开本页且有内容」时创建会话并保存（无内容则不创建）。
// - 加附件/选模型/选目录/选权限：因需要落地，立即惰性建草稿会话。
// 本组件由 App 用 key（草稿身份）挂载，切换草稿即重挂，卸载时执行「按需保存/清空草稿」。
export function SessionDraftPage() {
  const {
    currentSessionId,
    draftToOpen,
    draftSeedContent,
    draftSeedRole,
    draftSeedAgentId,
    draftSeedProjectId,
    materializeDraft,
    openSession,
    refreshSessions,
  } = useSession();
  const notify = useNotifications();

  // 草稿会话 id（null 表示尚未创建）。用 ref 以便在卸载清理里同步读取。
  const sessionIdRef = useRef<string | null>(draftToOpen ?? null);
  // 最新序列化内容（Composer 上报），用于卸载时保存。
  const latestContentRef = useRef<string>("");
  // 已提交标记：提交后卸载不再当作草稿保存。
  const submittedRef = useRef(false);
  // 本页是否「新建」了这个草稿会话（区别于打开已存草稿）。仅新建且最终为空的草稿才在卸载时删除——
  // 否则 StrictMode 下打开已存草稿会因首挂载 cleanup 抢先（内容尚未注水）而被误删。
  const createdHereRef = useRef(false);

  const [draftSession, setDraftSession] = useState<SessionInfo | null>(null);
  const [initial, setInitial] = useState<{
    content: string;
    attachments: Attachment[];
  } | null>(draftToOpen ? null : { content: draftSeedContent ?? "", attachments: [] });
  const [modelGroups, setModelGroups] = useState<EnabledProviderModels[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);
  const [roleExperts, setRoleExperts] = useState<ExpertSummary[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [draftProjectId, setDraftProjectId] = useState<string | null>(draftSeedProjectId);
  const [draftAgentId, setDraftAgentId] = useState<string | null>(draftSeedAgentId);
  const [draftSelectedModelId, setDraftSelectedModelId] = useState<string | null>(null);
  const [draftPermissionMode, setDraftPermissionMode] = useState<PermissionMode | null>(null);
  const [draftModeValue, setDraftModeValue] = useState<"normal" | "plan">("normal");
  const [globalPermMode, setGlobalPermMode] = useState<PermissionMode>("manual");
  const [recents, setRecents] = useState<string[]>([]);

  // 加载启用模型 / 全局权限 / 最近目录（供草稿页下拉）。
  useEffect(() => {
    listEnabledModels().then(setModelGroups).catch(console.error);
    listActiveTeams().then(setTeams).catch(console.error);
    listExperts().then(setRoleExperts).catch(console.error);
    listAgents().then(setAgents).catch(console.error);
    listProjects().then(setProjects).catch(console.error);
    getGlobalPermissionMode().then(setGlobalPermMode).catch(console.error);
    getRecentWorkspaces().then(setRecents).catch(console.error);
  }, []);

  useEffect(() => {
    setDraftProjectId(draftSeedProjectId);
  }, [draftSeedProjectId]);

  useEffect(() => {
    setDraftAgentId(draftSeedAgentId);
  }, [draftSeedAgentId]);

  // 打开已存草稿：加载其内容注水。
  useEffect(() => {
    if (!draftToOpen) return;
    let cancelled = false;
    getSession(draftToOpen)
      .then((d) => {
        if (cancelled || !d) return;
        setDraftSession(d.session);
        const content = d.session.draftContent ?? "";
        latestContentRef.current = content;
        const parsed = extractAttachments(content);
        setInitial({
          content: parsed.body,
          attachments: parsed.attachments.map((a, i) => ({
            id: `init-${i}`,
            relPath: a.relPath,
            name: a.name,
            kind: a.kind,
          })),
        });
      })
      .catch(console.error);
    return () => {
      cancelled = true;
    };
  }, [draftToOpen]);

  // 卸载时：有内容→（必要时建会话并）保存；无内容但已建空会话→删除。
  useEffect(() => {
    return () => {
      if (submittedRef.current) return;
      const content = latestContentRef.current;
      const id = sessionIdRef.current;
      if (content.trim()) {
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
          await setDraftContent(realId, content).catch(console.error);
          refreshSessions();
        })();
      } else if (id && createdHereRef.current) {
        // 仅删除「本页新建且最终为空」的草稿（连同其默认工作目录）；打开的已存草稿不动。
        void deleteSession(id)
          .then(() => refreshSessions())
          .catch(console.error);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 惰性建草稿会话（加附件/选项/提交时调用）。
  const ensureDraftSession = async (): Promise<string | null> => {
    if (sessionIdRef.current) return sessionIdRef.current;
    if (currentSessionId) {
      sessionIdRef.current = currentSessionId;
      return currentSessionId;
    }
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

  // 「使用专家/团队」入口：进入草稿后用预选角色激活一次（建草稿会话 + 设角色 + 同步下拉）。
  const seedRoleAppliedRef = useRef(false);
  useEffect(() => {
    if (draftSeedRole && !seedRoleAppliedRef.current) {
      seedRoleAppliedRef.current = true;
      void pickRole(draftSeedRole.kind, draftSeedRole.id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draftSeedRole]);

  const pickAgent = async (id: string) => {
    if (id) setDraftProjectId(null);
    setDraftAgentId(id || null);
    const sid = await ensureDraftSession();
    if (!sid) return;
    try {
      await setSessionAgent(sid, id || null);
      setDraftSession((prev) => (prev ? { ...prev, agentId: id || null } : prev));
    } catch (err) {
      console.error(err);
      notify.error("设置智能体失败：" + String(err));
    }
  };

  // 「使用智能体」入口：进入草稿后必须把预选智能体写入草稿会话。
  // 仅靠 draftAgentId 会让 UI 看起来选中了智能体，但提交后的正式会话没有 agent_id。
  const seedAgentAppliedRef = useRef(false);
  useEffect(() => {
    if (draftSeedAgentId && !seedAgentAppliedRef.current) {
      seedAgentAppliedRef.current = true;
      void pickAgent(draftSeedAgentId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draftSeedAgentId]);

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
    const cur = draftSession?.mode === "plan" ? "normal" : "plan";
    try {
      await setSessionMode(id, cur);
      setDraftSession((prev) => (prev ? { ...prev, mode: cur } : prev));
    } catch (err) {
      console.error(err);
      notify.error("切换计划模式失败");
    }
  };

  // 切到本地目录：清掉「智能体」实体上下文（智能体带专属目录），让角色选择恢复。
  // 专家/团队角色与本地目录正交、保留；项目由 setDraftProjectId(null) 清除。
  const clearAgentContext = async (sid: string) => {
    if (!draftSession?.agentId && !draftAgentId) return;
    try {
      await setSessionAgent(sid, null);
      setDraftAgentId(null);
      setDraftSession((prev) => (prev ? { ...prev, agentId: null } : prev));
    } catch (err) {
      console.error(err);
    }
  };

  const pickWorkspace = async () => {
    const dir = await pickDirectory();
    if (!dir) return;
    setDraftProjectId(null);
    const id = await ensureDraftSession();
    if (!id) return;
    await clearAgentContext(id);
    try {
      const next = await setSessionWorkspace(id, dir);
      setDraftSession(next.session);
      getRecentWorkspaces().then(setRecents).catch(console.error);
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
    await clearAgentContext(id);
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
        openSession(projectSessionId);
      } catch (err) {
        console.error(err);
        notify.error("发送失败：" + String(err));
        submittedRef.current = false;
        throw err;
      } finally {
        refreshSessions();
      }
      return;
    }
    const id = await ensureDraftSession();
    if (!id) throw new Error("创建草稿失败");
    submittedRef.current = true; // 卸载时不再当草稿保存。
    try {
      await submitUserMessage(id, text);
      openSession(id); // 退出草稿态、进入正式会话视图。
    } catch (err) {
      console.error(err);
      notify.error("发送失败：" + String(err));
      submittedRef.current = false;
      throw err;
    } finally {
      refreshSessions();
    }
  };

  const dDir = draftSession?.workingDir?.trim() || "";
  const dWsName = dDir ? baseName(dDir) : undefined;
  const effectiveMode = draftSession?.mode ?? draftModeValue;
  const effectivePermissionMode = draftSession?.permissionMode ?? draftPermissionMode;
  const effectiveSelectedModelId = draftSession?.selectedModelId ?? draftSelectedModelId;
  // 角色：草稿会话存在后只认它的 role（清除后即 ""，不再回退种子角色）；种子角色仅在会话未建时作初值。
  const sessionRoleKind = draftSession ? (draftSession.roleKind ?? "") : (draftSeedRole?.kind ?? "");
  const sessionRoleId = draftSession ? (draftSession.roleId ?? "") : (draftSeedRole?.id ?? "");
  const effectiveRoleKind = draftProjectId ? "" : sessionRoleKind;
  const effectiveRoleId = draftProjectId ? "" : sessionRoleId;
  const selectedAgentId = draftProjectId
    ? null
    : (draftSession?.agentId ?? draftAgentId ?? null);

  // 等待打开的草稿注水完成。
  if (draftToOpen && initial === null) {
    return <div className="p-6 text-foreground-muted">加载中…</div>;
  }

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col">
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6">
        <h1 className="text-2xl font-semibold text-foreground">
          不止聊天，搞定一切
        </h1>
        <p className="text-sm text-foreground-muted">
          本地运行、自主规划、安全可控的 AI 工作搭子
        </p>
        <div className="min-w-[720px] max-w-[720px]">
          {!draftProjectId && (
            <PopularExpertsBar
              agents={roleExperts}
              roleKind={effectiveRoleKind}
              roleId={effectiveRoleId}
              seedRole={draftSeedRole}
              onPickExpert={(expertId) => void pickRole("expert", expertId)}
            />
          )}
          <Composer
            sessionId={sessionIdRef.current ?? ""}
            disabled={false}
            onSubmit={onSubmit}
            onEnsureSessionId={ensureDraftSession}
            initialContent={initial?.content || undefined}
            initialAttachments={
              initial && initial.attachments.length > 0 ? initial.attachments : undefined
            }
            onDraftChange={(serialized) => {
              latestContentRef.current = serialized;
            }}
            onAttachFile={onAttachFile}
            onPasteFile={onPasteFile}
            projects={projects}
            selectedProjectId={draftProjectId}
            onPickProject={(projectId) => {
              // 选项目：与「智能体」上下文互斥——清掉智能体角色。
              setDraftProjectId(projectId);
              void (async () => {
                const sid = sessionIdRef.current;
                if (sid) await clearAgentContext(sid);
              })();
            }}
            agents={agents}
            selectedAgentId={selectedAgentId}
            onPickAgent={(id) => {
              void pickAgent(id);
            }}
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
            roleKind={effectiveRoleKind}
            roleId={effectiveRoleId}
            onPickRole={(draftProjectId || selectedAgentId) ? undefined : (k, i) => void pickRole(k, i)}
            planMode={effectiveMode === "plan"}
            onTogglePlan={() => void togglePlan()}
            permissionMode={effectivePermissionMode}
            globalPermMode={globalPermMode}
            onChangePermission={(m) => void switchPermissionMode(m)}
          />
        </div>
      </div>
      
      </div>
  );
}

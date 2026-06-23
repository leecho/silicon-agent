import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

type SessionContextValue = {
  currentSessionId: string | null;
  newSessionRequestKey: number;
  openSession: (id: string | null) => void;
  refreshSessions: () => void;
  requestNewSession: () => void;
  sessionRefreshKey: number;
  /** 是否处于草稿编辑态（渲染草稿页而非会话页/空态）。 */
  draftMode: boolean;
  /** 待打开的已存草稿 id（非空表示从「草稿」区点开，需用其 draft_content 注水）。 */
  draftToOpen: string | null;
  /** 新草稿的预填正文（如「使用 AI 创建技能」入口注入的提示词）；消费后由下一次进入草稿重置。 */
  draftSeedContent: string | null;
  /** 新草稿的预选角色（如「使用专家」入口）：进入草稿后由草稿页激活该角色；消费后重置。 */
  draftSeedRole: { kind: string; id: string } | null;
  draftSeedAgentId: string | null;
  draftSeedProjectId: string | null;
  /** 进入新草稿（清空 currentSessionId，不创建任何东西）。 */
  enterDraft: () => void;
  /** 进入新草稿并预填 composer 正文（用于「使用 AI 创建技能」等带提示词的入口）。 */
  enterDraftWithContent: (content: string) => void;
  enterDraftWithExpert: (expertId: string, content?: string) => void;
  enterDraftWithAgent: (agentId: string, content?: string) => void;
  enterDraftWithTeam: (teamId: string, content?: string) => void;
  enterDraftWithProject: (projectId: string, content?: string) => void;
  /** 打开一条已存在的草稿（保持草稿态并定位到该 id）。 */
  openDraft: (id: string) => void;
  /** 草稿惰性建会话后挂载其 id（保持草稿态）。 */
  materializeDraft: (id: string) => void;
};

const SessionContext = createContext<SessionContextValue | null>(null);
type SessionOpenTarget = { sessionId?: string | null; draftId?: string | null };

export function SessionProvider({
  children,
  onOpenSession,
}: {
  children: ReactNode;
  onOpenSession?: (target?: SessionOpenTarget) => void;
}) {
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [sessionRefreshKey, setSessionRefreshKey] = useState(0);
  const [newSessionRequestKey, setNewSessionRequestKey] = useState(0);
  const [draftMode, setDraftMode] = useState(false);
  const [draftToOpen, setDraftToOpen] = useState<string | null>(null);
  const [draftSeedContent, setDraftSeedContent] = useState<string | null>(null);
  const [draftSeedRole, setDraftSeedRole] = useState<{ kind: string; id: string } | null>(
    null,
  );
  const [draftSeedAgentId, setDraftSeedAgentId] = useState<string | null>(null);
  const [draftSeedProjectId, setDraftSeedProjectId] = useState<string | null>(null);

  const openSession = useCallback(
    (id: string | null) => {
      setCurrentSessionId(id);
      setDraftMode(false);
      setDraftToOpen(null);
      setDraftSeedAgentId(null);
      setDraftSeedProjectId(null);
      onOpenSession?.({ sessionId: id });
    },
    [onOpenSession],
  );

  const refreshSessions = useCallback(() => {
    setSessionRefreshKey((key) => key + 1);
  }, []);

  // 进入新草稿的核心动作：清空当前会话、置草稿态、触发重挂，不触碰预填内容。
  const enterDraftCore = useCallback(() => {
    setCurrentSessionId(null);
    setDraftMode(true);
    setDraftToOpen(null);
    setNewSessionRequestKey((key) => key + 1);
    onOpenSession?.();
  }, [onOpenSession]);

  // 进入新草稿（首次落地动作时才惰性建会话）；清空预填，保证普通「新任务」为空白。
  const enterDraft = useCallback(() => {
    setDraftSeedContent(null);
    setDraftSeedRole(null);
    setDraftSeedAgentId(null);
    setDraftSeedProjectId(null);
    enterDraftCore();
  }, [enterDraftCore]);

  // 进入新草稿并预填 composer 正文（带提示词的入口，如「使用 AI 创建技能」）。
  const enterDraftWithContent = useCallback(
    (content: string) => {
      setDraftSeedContent(content);
      setDraftSeedRole(null);
      setDraftSeedAgentId(null);
      setDraftSeedProjectId(null);
      enterDraftCore();
    },
    [enterDraftCore],
  );

  const enterDraftWithExpert = useCallback(
    (expertId: string, content?: string) => {
      setDraftSeedContent(content ?? null);
      setDraftSeedRole({ kind: "expert", id: expertId });
      setDraftSeedAgentId(null);
      setDraftSeedProjectId(null);
      enterDraftCore();
    },
    [enterDraftCore],
  );

  const enterDraftWithAgent = useCallback(
    (agentId: string, content?: string) => {
      setDraftSeedContent(content ?? null);
      setDraftSeedRole(null);
      setDraftSeedAgentId(agentId);
      setDraftSeedProjectId(null);
      enterDraftCore();
    },
    [enterDraftCore],
  );

  const enterDraftWithTeam = useCallback(
    (teamId: string, content?: string) => {
      setDraftSeedContent(content ?? null);
      setDraftSeedRole({ kind: "team", id: teamId });
      setDraftSeedAgentId(null);
      setDraftSeedProjectId(null);
      enterDraftCore();
    },
    [enterDraftCore],
  );

  const enterDraftWithProject = useCallback(
    (projectId: string, content?: string) => {
      setDraftSeedContent(content ?? null);
      setDraftSeedRole(null);
      setDraftSeedAgentId(null);
      setDraftSeedProjectId(projectId);
      enterDraftCore();
    },
    [enterDraftCore],
  );

  // 打开已存在的草稿：保持草稿态、定位到该 id，并标记需用其内容注水。
  const openDraft = useCallback(
    (id: string) => {
      setCurrentSessionId(id);
      setDraftMode(true);
      setDraftToOpen(id);
      setDraftSeedContent(null);
      setDraftSeedRole(null);
      setDraftSeedAgentId(null);
      setDraftSeedProjectId(null);
      onOpenSession?.({ draftId: id });
    },
    [onOpenSession],
  );

  // 草稿惰性建会话后挂载其 id（保持草稿态，不退出草稿、不触发注水）。
  const materializeDraft = useCallback((id: string) => {
    setCurrentSessionId(id);
  }, []);

  const requestNewSession = useCallback(() => {
    enterDraft();
  }, [enterDraft]);

  const value = useMemo<SessionContextValue>(
    () => ({
      currentSessionId,
      newSessionRequestKey,
      openSession,
      refreshSessions,
      requestNewSession,
      sessionRefreshKey,
      draftMode,
      draftToOpen,
      draftSeedContent,
      draftSeedRole,
      draftSeedAgentId,
      draftSeedProjectId,
      enterDraft,
      enterDraftWithContent,
      enterDraftWithExpert,
      enterDraftWithAgent,
      enterDraftWithTeam,
      enterDraftWithProject,
      openDraft,
      materializeDraft,
    }),
    [
      currentSessionId,
      newSessionRequestKey,
      openSession,
      refreshSessions,
      requestNewSession,
      sessionRefreshKey,
      draftMode,
      draftToOpen,
      draftSeedContent,
      draftSeedRole,
      draftSeedAgentId,
      draftSeedProjectId,
      enterDraft,
      enterDraftWithContent,
      enterDraftWithExpert,
      enterDraftWithAgent,
      enterDraftWithTeam,
      enterDraftWithProject,
      openDraft,
      materializeDraft,
    ],
  );

  return (
    <SessionContext.Provider value={value}>{children}</SessionContext.Provider>
  );
}

export function useSession() {
  const context = useContext(SessionContext);
  if (!context) throw new Error("useSession must be used within SessionProvider");
  return context;
}

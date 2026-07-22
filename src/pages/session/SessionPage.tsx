import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { CircleAlert, CirclePlus, CornerDownLeft, FolderTree, Globe, ListChecks, MonitorPlay, PanelRightOpen } from "lucide-react";
import {
  attachFile,
  cancelAskResponse,
  cancelChild,
  compactSession,
  getGlobalPermissionMode,
  getRecentWorkspaces,
  getSessionTaskPanelDefaultVisible,
  getShowCompletedProcess,
  getComputerUseEnabled,
  getBrowserUseEnabled,
  getSession,
  findChildSession,
  listSessionChildren,
  listSessionQueue,
  cancelQueuedTask,
  getSessionContextUsage,
  getSessionUsage,
  listEnabledModels,
  listMemories,
  listProjects,
  listSessionWorkspaceFiles,
  openSessionWorkspace,
  pickDirectory,
  pickFile,
  retrySession,
  saveAttachment,
  setSessionModel,
  setSessionAgent,
  setSessionRole,
  listActiveTeams,
  listAgents,
  listExperts,
  setSessionMode,
  setSessionPermissionMode,
  setSessionWorkspace,
  stopSession,
  submitAskResponse,
  submitPermissionDecision,
  submitPlanDecision,
  submitUserMessage,
  subscribeAgentStreamEvents,
  subscribeSessionSuggestions,
} from "../../api";
import type {
  Agent,
  AgentStreamEvent,
  ExpertSummary,
  Team,
  ChildAgentSummary,
  QueuedTask,
  Artifact,
  ContextUsageView,
  EnabledProviderModels,
  FeedRow,
  Message,
  PendingAsk,
  PendingPermission,
  PermissionMode,
  Project,
  RunActivity,
  Session,
  TodoItem,
  UsageTotals,
} from "../../types";
import { Disclosure, MessageFeed, buildPersistedRows } from "../../components/session/MessageFeed";
import {
  MessageFeedNav,
  type MessageFeedNavItem,
} from "../../components/session/MessageFeedNav";
import { extractAttachments } from "../../lib/attachments";
import { Composer, type ComposerHandle } from "../../components/session/Composer";
import { SessionMonitorPanel } from "../../components/session/SessionMonitorPanel";
import { WorkspaceTab } from "../../components/session/WorkspaceTab";
import { ArtifactPreviewDrawer } from "../../components/session/ArtifactPreviewDrawer";
import { SessionAskCard } from "../../components/session/SessionAskCard";
import { SessionPermissionCard } from "../../components/session/SessionPermissionCard";
import { SessionPlanCard } from "../../components/session/SessionPlanCard";
import { useSession } from "../../components/session/SessionProvider";
import { renameSession } from "../../api";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { Tooltip } from "../../components/ui/Tooltip";
import {
  SLASH_COMMANDS,
  parseSlashCommand,
  type ParsedCommand,
} from "./slashCommands";
import { toolActivityLabel } from "../../components/session/toolNarrative";
import { Button } from "../../components/ui";
import { ComputerPanel } from "../../components/session/computer/ComputerPanel";
import { BrowserPanel } from "../../components/session/browser/BrowserPanel";
import {
  SessionSidePanel,
  type SidePanelTab,
  type SidePanelTabDef,
} from "../../components/session/SessionSidePanel";

type SessionLoadStatus = "idle" | "loading" | "ready" | "missing" | "error";
const SESSION_FEED_STICKY_BOTTOM_PX = 80;

// 取路径的最后一段（目录名）：去掉结尾斜杠后取最后一段；空串返回空。
function baseName(p: string): string {
  const t = p.replace(/[/\\]+$/, "");
  const i = Math.max(t.lastIndexOf("/"), t.lastIndexOf("\\"));
  return i >= 0 ? t.slice(i + 1) : t;
}

function parseEpochSeconds(value?: string | null): number | null {
  if (!value) return null;
  const seconds = Number(value);
  if (Number.isFinite(seconds) && seconds > 0) return seconds * 1000;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function activityFromSession(session: Session): RunActivity | null {
  // 停泊等专家：父虽不在 RunRegistry，但仍是「委派进行中」——保持忙态，消除假死。
  if (session.session.awaitingSubagent) {
    return {
      label: "正在等待专家处理…",
      startedAt: parseEpochSeconds(session.session.runStartedAt) ?? Date.now(),
    };
  }
  if (!session.isRunning) return null;
  return {
    label: "正在思考",
    startedAt: parseEpochSeconds(session.session.runStartedAt) ?? Date.now(),
  };
}

/** 父会话是否处于「委派专家处理中」的等待态（用于 busy/composer 禁用）。 */
function isAwaitingSubagent(session: Session | null): boolean {
  return !!session?.session.awaitingSubagent;
}

// 把每题答案格式化为与后端 format_ask_answers 一致的文本，供回答后乐观插入 feed
// （run_finished 用 DB 重建时同构替换，无闪烁）。
function formatAskAnswers(ask: PendingAsk, answers: string[][]): string {
  let out = "用户已回答：";
  ask.questions.forEach((q, i) => {
    const label = q.header.trim() ? q.header : q.question;
    const vals = answers[i] ?? [];
    const answer = vals.length > 0 ? vals.join("、") : "（未回答）";
    out += `\n${i + 1}. ${label}：${answer}`;
  });
  return out;
}

// feed 行的稳定 key：按 kind + id 索引，供事件 find-or-create-upsert。
function rowKey(r: FeedRow): string {
  if (r.kind === "assistant") return "a:" + r.id;
  if (r.kind === "tool") return "t:" + r.id;
  return "u:" + r.id;
}

function streamEventKey(e: AgentStreamEvent): string | null {
  if (e.sequence > 0) {
    return JSON.stringify([
      e.sessionId,
      e.messageId,
      e.kind,
      e.toolCallId ?? "",
      e.sequence,
    ]);
  }
  // sequence=0 但有稳定身份的非流式事件（如 model_retrying「第 X/N 次重试」）：
  // 用 kind + messageId + text 去重，防双监听/重投递（StrictMode/HMR）导致重复行。
  // messageId 每次模型调用唯一 → 不同 run 的重试不误并；text 含次数 → 同一 run 各次不误并。
  if (e.kind === "model_retrying") {
    return JSON.stringify([e.sessionId, e.messageId, e.kind, e.text ?? ""]);
  }
  return null;
}

function isFeedNearBottom(el: HTMLDivElement): boolean {
  return (
    el.scrollHeight - el.scrollTop - el.clientHeight <=
    SESSION_FEED_STICKY_BOTTOM_PX
  );
}

/** 功能未开启时，对应 tab 内的引导占位：提示去设置开启。 */
function FeatureOffHint({ name }: { name: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
      <span className="text-sm font-medium text-foreground">「{name}」未开启</span>
      <span className="text-xs text-foreground-muted">可在「设置」中开启后，在此面板使用。</span>
    </div>
  );
}

export function SessionPage() {
  const {
    currentSessionId: sessionId,
    refreshSessions,
    requestNewSession,
    openSession,
    enterDraftWithProject,
  } = useSession();
  const messages = useMessages();
  const notify = useNotifications();
  // detail 仅用于 session.id 与初次加载的持久 messages（reload 用）。
  const [detail, setDetail] = useState<Session | null>(null);
  const [sessionLoadStatus, setSessionLoadStatus] =
    useState<SessionLoadStatus>("idle");
  const [sessionLoadError, setSessionLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // L0 乐观停止：点「停止」后立即置真——STOP 键切「停止中」、冻结流式增量渲染，
  // 不等后端撞检查点。以 run_finished 为权威对账回收（见事件处理）。
  const [stopping, setStopping] = useState(false);
  // 压缩进行中（仅供压缩按钮转圈；与 busy 区分，避免任何 run 都让按钮转圈）。
  const [compacting, setCompacting] = useState(false);
  // pending 控制权限卡显示，与 busy 解耦：busy 仅表示命令进行中。
  const [pending, setPending] = useState<PendingPermission | null>(null);
  // 子代理（child）权限请求：冒泡到父会话，可操作确认；决定提交给该 child 会话并续跑它。
  const [childPending, setChildPending] = useState<{
    childId: string;
    agentLabel?: string;
    pending: PendingPermission;
  } | null>(null);
  // 子代理（child）提问请求：冒泡到父会话作答；答案提交给该 child 会话并续跑它。
  const [childAsk, setChildAsk] = useState<{
    childId: string;
    agentLabel?: string;
    ask: PendingAsk;
  } | null>(null);
  // ask 控制 Ask 卡显示（与 pending 互斥，引擎一次只暂停一种）。
  const [ask, setAsk] = useState<PendingAsk | null>(null);
  // plan 控制计划卡显示（plan 模式 propose_plan 暂停时非空，与 pending/ask 互斥）。
  const [plan, setPlan] = useState<Session["pendingPlan"] | null>(null);
  // todos：当前会话的待办清单，随 todos_updated 事件实时更新。
  const [todos, setTodos] = useState<TodoItem[]>([]);
  // 全局权限模式（用于在会话覆盖为 null 时展示继承值）。
  const [globalPermMode, setGlobalPermMode] = useState<PermissionMode>("manual");
  // 最近使用的工作目录（全局），供 composer 下拉子菜单展示。
  const [recents, setRecents] = useState<string[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  // 产物列表（随 artifacts_updated 事件实时更新）+ 当前预览的产物。
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [previewArtifact, setPreviewArtifact] = useState<Artifact | null>(null);
  // 工作空间文件列表（相对路径）+ 加载/错误态；由 workspace tab 使用，随 tab 激活 / artifacts_updated 重拉。
  const [wsFiles, setWsFiles] = useState<string[]>([]);
  const [wsFilesLoading, setWsFilesLoading] = useState(false);
  const [wsFilesError, setWsFilesError] = useState<string | null>(null);
  const [collapsedMonitorSessionId, setCollapsedMonitorSessionId] = useState<string | null>(null);
  // 一轮结束后的快捷建议（后台生成，事件推送）；发新消息/切会话/新 run 时清空。
  const [suggestions, setSuggestions] = useState<string[]>([]);
  // 当前 run 在 Composer 中展示的运行阶段；startedAt 在 run_started 时固定，后续只更新 label。
  const [runActivity, setRunActivity] = useState<RunActivity | null>(null);
  // 启用模型分组（按厂商），供 Composer 模型下拉。
  const [modelGroups, setModelGroups] = useState<EnabledProviderModels[]>([]);
  // 可选团队 + 散装 agent（Composer 角色槽）；启动加载一次，窗口聚焦刷新。
  const [teams, setTeams] = useState<Team[]>([]);
  const [roleExperts, setRoleExperts] = useState<ExpertSummary[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  // 当前会话上下文窗口占用（供 Composer 的 context meter）；切会话清空，载入/每轮结束后刷新。
  const [contextUsage, setContextUsage] = useState<ContextUsageView | null>(
    null,
  );
  // 当前会话累计 token 用量（供 Composer 的累计 chip）；与 contextUsage 同步刷新。
  const [sessionUsage, setSessionUsage] = useState<UsageTotals | null>(null);
  const [showCompletedProcess, setShowCompletedProcess] = useState(true);
  // 「桌面操作」功能总开关（启动加载一次）：决定桌面 tab 内是否提供进入入口。
  const [computerUseEnabled, setComputerUseEnabled] = useState(false);
  // 「浏览器操作」功能总开关（全局设置）：决定浏览器 tab 内是否提供进入入口。
  const [browserUseEnabled, setBrowserUseEnabled] = useState(false);
  // 右侧任务面板当前 tab：监控 / 浏览器 / 桌面（原三块独立侧栏合并为一块，按 tab 切换）。
  const [sidePanelTab, setSidePanelTab] = useState<SidePanelTab>("monitor");
  // 自动切换去重：记最近一次因工具活动自动切到的工具行 id，避免重复切 / 打开历史会话误切。
  const autoSwitchedRowRef = useRef<string | null>(null);
  // 产物按「轮」分组：用 artifact.messageId 在消息时间线上往前找最近的 user 消息作轮根，
  // 供 feed 在每轮末尾汇总「本轮产物」。映射不到轮根的（如刷新前临时态）暂不计入。
  const artifactsByRound = useMemo(() => {
    const msgs = detail?.messages ?? [];
    const idIndex = new Map<string, number>();
    msgs.forEach((m, i) => idIndex.set(m.id, i));
    const map = new Map<string, Artifact[]>();
    for (const a of artifacts) {
      let root: string | undefined;
      const idx = a.messageId ? idIndex.get(a.messageId) : undefined;
      if (idx !== undefined) {
        for (let i = idx; i >= 0; i--) {
          if (msgs[i].role === "user") {
            root = msgs[i].id;
            break;
          }
        }
      }
      if (!root) continue;
      const arr = map.get(root);
      if (arr) arr.push(a);
      else map.set(root, [a]);
    }
    return map;
  }, [detail?.messages, artifacts]);
  // 显示源：事件增量构建的一个数组，就是渲染输入；完成时不被持久数据整体覆盖。
  // list 保持到达顺序、map 供按 rowKey find-or-create-upsert。
  const composerRef = useRef<ComposerHandle | null>(null);
  const feedRef = useRef<{ list: FeedRow[]; map: Map<string, FeedRow> }>({
    list: [],
    map: new Map(),
  });
  const [feedVersion, bump] = useState(0);
  const feedScrollRef = useRef<HTMLDivElement | null>(null);
  const feedPinnedToBottomRef = useRef(true);
  const lastAutoScrolledSessionIdRef = useRef<string | null>(null);
  // 订阅 effect 的依赖为 []，闭包里的 sessionId 会是初值（陈旧）。
  // 用 ref 持有当前 sessionId，事件处理里据此过滤跨会话流（事件带 sessionId）。
  const sessionIdRef = useRef<string | null>(sessionId);
  sessionIdRef.current = sessionId;
  // 事件回调里读最新 stopping（闭包捕获旧值会漏判），跟随 state 刷新。
  const stoppingRef = useRef(false);
  stoppingRef.current = stopping;
  // 当前正在流式输出的 assistant messageId：用于让该行思考块进入「思考中」态。
  const streamingIdRef = useRef<string | null>(null);
  const processedStreamEventKeysRef = useRef<Set<string>>(new Set());
  // dispatch tool_call id → child 会话 id（从 child 事件的 parentToolCallId/sessionId 累积）。
  const childByCallRef = useRef<Map<string, string>>(new Map());
  // 当前会话的专家（child 子运行）列表 + 状态（右侧面板展示）。
  const [childAgents, setChildAgents] = useState<ChildAgentSummary[]>([]);
  // T70：当前会话任务队列（排队中的用户消息），Composer 上方排队条展示。
  const [queuedTasks, setQueuedTasks] = useState<QueuedTask[]>([]);
  // agent name → 展示名 映射（来自已解析的 child 列表）：让 feed 派发卡与抽屉标题显示「珀西」而非 image-creator。
  const agentDisplayNames = useMemo(() => {
    const m: Record<string, string> = {};
    for (const c of childAgents) {
      if (c.displayName && c.displayName.trim()) m[c.expertName] = c.displayName;
    }
    return m;
  }, [childAgents]);
  // 侧边导航项：每条用户消息一项（id + 截断文字）。必须在任何条件 return 之前调用（Hooks 规则）。
  const feedNavItems = useMemo<MessageFeedNavItem[]>(() => {
    const clip = (s: string, n: number) =>
      s.length > n ? s.slice(0, n) + "…" : s;
    const items: MessageFeedNavItem[] = [];
    let current: { id: string; title: string; reply: string } | null = null;
    const flush = () => {
      if (!current) return;
      items.push({
        ...current,
        artifacts: (artifactsByRound.get(current.id) ?? []).filter(
          (a) => a.kind !== "working",
        ),
      });
    };
    for (const r of feedRef.current.list) {
      if (r.kind === "user") {
        flush();
        const { body } = extractAttachments(r.content);
        const firstLine =
          body
            .split(/\r\n|\r|\n/)
            .map((l) => l.trim())
            .find(Boolean) ?? "";
        current = {
          id: r.id,
          title: clip(firstLine, 60) || "（空消息）",
          reply: "",
        };
      } else if (
        r.kind === "assistant" &&
        current &&
        r.content.trim().length > 0
      ) {
        // 该轮最后一条有内容的 assistant = 最终回复；折叠空白后截断，供卡片 line-clamp 预览。
        current.reply = clip(r.content.trim().replace(/\s+/g, " "), 200);
      }
    }
    flush();
    return items;
  }, [feedVersion, artifactsByRound]);
  // 各 child 子运行的「当前步骤」（childSessionId → 短句，如「正在搜索网页」），由 child 实时事件累积，
  // 供专家面板在运行中行展示其正在做什么（仅 running 行显示）。
  const [childSteps, setChildSteps] = useState<Record<string, string>>({});
  // 当前并行运行中的专家数（供活动条决策：>1 显示稳定的「N 个专家并行处理中」，避免逐事件切名闪烁）。
  const runningChildCountRef = useRef(0);
  runningChildCountRef.current = childAgents.filter(
    (c) => c.status === "running",
  ).length;

  useLayoutEffect(() => {
    const el = feedScrollRef.current;
    if (!el || !detail) return;
    const sessionChanged = lastAutoScrolledSessionIdRef.current !== detail.session.id;
    if (!sessionChanged && !feedPinnedToBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
    feedPinnedToBottomRef.current = true;
    lastAutoScrolledSessionIdRef.current = detail.session.id;
  }, [detail?.session.id, feedVersion]);

  function handleFeedScroll() {
    const el = feedScrollRef.current;
    if (!el) return;
    feedPinnedToBottomRef.current = isFeedNearBottom(el);
  }

  // 刷新 context meter + 累计用量 chip：取最近一轮真实用量与会话累计；切到别的会话则丢弃结果。
  const refreshContextUsage = (sid: string) => {
    void getSessionContextUsage(sid)
      .then((u) => {
        if (sid === sessionIdRef.current) setContextUsage(u);
      })
      .catch((err) => console.error(err));
    void getSessionUsage(sid)
      .then((u) => {
        if (sid === sessionIdRef.current) setSessionUsage(u);
      })
      .catch((err) => console.error(err));
  };

  // 手动压缩较早历史（/compact 命令与 Composer 压缩按钮共用）。
  const doCompact = async () => {
    if (!detail || busy || compacting) return;
    setCompacting(true);
    setBusy(true);
    try {
      await compactSession(detail.session.id);
      feedRef.current.list.push({
        kind: "divider",
        id: "compact-" + Date.now(),
        content: "已压缩较早的对话历史（模型上下文已精简）。",
      });
      bump((n) => n + 1);
      refreshContextUsage(detail.session.id);
      notify.success("已压缩较早的对话历史");
    } catch (err) {
      console.error(err);
      notify.error("压缩对话历史失败");
    } finally {
      setBusy(false);
      setCompacting(false);
    }
  };

  // reload：用持久 messages 重建整个 feed（仅初次加载/重开 app 调用）。
  const rebuildFeed = (messages: Message[], showCompletedProcess = true) => {
    const rows = buildPersistedRows(messages, showCompletedProcess);
    feedRef.current = {
      list: rows,
      map: new Map(rows.map((r) => [rowKey(r), r])),
    };
  };

  // 按 key find-or-create + 原地 mutate + bump 触发重渲染。
  const upsertRow = (
    key: string,
    make: () => FeedRow,
    update: (r: FeedRow) => void,
  ) => {
    const m = feedRef.current.map;
    let r = m.get(key);
    if (!r) {
      r = make();
      m.set(key, r);
      feedRef.current.list.push(r);
    }
    update(r);
    bump((n) => n + 1);
  };

  // 依赖 sessionId：切换会话时完整重置（清 feed/pending/ask/busy），再用新 detail 重建。
  // 切换会话不串：旧会话的 live 行、暂停卡片一律清空，避免跨会话残留。
  useEffect(() => {
    if (sessionId == null) {
      setDetail(null);
      setSessionLoadStatus("idle");
      setSessionLoadError(null);
      feedRef.current = { list: [], map: new Map() };
      processedStreamEventKeysRef.current.clear();
      setPending(null);
      setAsk(null);
      setPlan(null);
      setBusy(false);
      setStopping(false);
      setTodos([]);
      setSuggestions([]);
      setRunActivity(null);
      setCollapsedMonitorSessionId(null);
      bump((n) => n + 1);
      return;
    }
    let cancelled = false;
    // 先就地重置，避免上一个会话内容闪现。
    feedRef.current = { list: [], map: new Map() };
    processedStreamEventKeysRef.current.clear();
    setPending(null);
    setAsk(null);
    setPlan(null);
    setBusy(false);
    setStopping(false);
    setDetail(null);
    setSessionLoadStatus("loading");
    setSessionLoadError(null);
    setTodos([]);
    setArtifacts([]);
    setChildAgents([]);
    setQueuedTasks([]);
    setChildSteps({});
    childByCallRef.current.clear();
    setSuggestions([]);
    setRunActivity(null);
    setContextUsage(null);
    bump((n) => n + 1);
    Promise.all([
      getSession(sessionId),
      getShowCompletedProcess(),
      getSessionTaskPanelDefaultVisible(),
    ])
      .then(([d, completedProcessVisible, showTaskPanelDefault]) => {
        if (cancelled) return;
        setShowCompletedProcess(completedProcessVisible);
        setCollapsedMonitorSessionId(showTaskPanelDefault ? null : sessionId);
        if (d === null) {
          setDetail(null);
          setSessionLoadStatus("missing");
          setPending(null);
          setAsk(null);
          setPlan(null);
          setBusy(false);
          setTodos([]);
          setArtifacts([]);
          setSuggestions([]);
          setRunActivity(null);
          return;
        }
        setDetail(d);
        setSessionLoadStatus("ready");
        rebuildFeed(d.messages, completedProcessVisible);
        setPending(d.pendingPermission ?? null);
        setAsk(d.pendingAsk ?? null);
        setPlan(d.pendingPlan ?? null);
        setTodos(d.todos ?? []);
        setArtifacts(d.artifacts ?? []);
        void listSessionChildren(sessionId).then(setChildAgents).catch(() => {});
        void listSessionQueue(sessionId).then(setQueuedTasks).catch(() => setQueuedTasks([]));
        setSuggestions(d.session.lastSuggestions ?? []);
        setBusy(d.isRunning || isAwaitingSubagent(d));
        setRunActivity(activityFromSession(d));
        refreshContextUsage(sessionId);
        bump((n) => n + 1);
      })
      .catch((err) => {
        if (cancelled) return;
        console.error(err);
        setDetail(null);
        setSessionLoadStatus("error");
        setSessionLoadError(String(err));
        setBusy(false);
        setRunActivity(null);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    // 异步监听器清理：StrictMode(dev) 下本 effect 跑两次，cleanup 在 subscribe 的 .then resolve
    // 之前执行——必须用 cancelled 标记，待 resolve 后若已 cancelled 则立即退订，否则两个监听器并存
    // 会让每个流式 delta 被处理两次(思考/答案重复输出)。
    let un: (() => void) | undefined;
    let cancelled = false;
    subscribeAgentStreamEvents((e: AgentStreamEvent) => {
      // child 子运行事件：记下 dispatch tool_call → child 会话 id 的映射（供「打开专家」侧栏定位）。
      // 在跨会话守卫之前，因为 child 事件的 sessionId 是 child、不等于当前会话。
      if (e.parentSessionId === sessionIdRef.current && e.parentToolCallId) {
        childByCallRef.current.set(e.parentToolCallId, e.sessionId);
        // 子代理实时动作：回显到父会话活动条（消除假死），并按 child 记下「当前步骤」供专家面板展示。
        const member = e.expertName ? `专家「${e.expertName}」` : "专家";
        let step: string | null = null;
        if (e.kind === "tool_call") step = toolActivityLabel(e.toolName, e.status);
        else if (e.kind === "message_delta") step = "正在回复";
        else if (e.kind === "thinking_delta") step = "正在思考";
        if (step) {
          const s = step;
          const child = e.sessionId;
          setChildSteps((m) => (m[child] === s ? m : { ...m, [child]: s }));
          // 多专家并行时活动条用稳定聚合文案，避免逐事件在不同专家名/动作间快速切换（呼吸灯闪烁）；
          // 仅单个专家在跑时才显示其具体步骤。逐专家实时步骤改到右侧面板各行查看。
          const runningCount = runningChildCountRef.current;
          const label =
            runningCount > 1
              ? `${runningCount} 个专家并行处理中`
              : `${member} · ${s}`;
          setRunActivity((cur) =>
            cur && cur.label === label
              ? cur
              : { label, startedAt: cur?.startedAt ?? Date.now() },
          );
        }
      }
      // 专家列表/状态刷新：任意 run 起止（含 child run 与父续跑）时重取（低频，状态最终一致）。
      if (
        (e.kind === "run_started" || e.kind === "run_finished") &&
        sessionIdRef.current
      ) {
        void listSessionChildren(sessionIdRef.current)
          .then(setChildAgents)
          .catch(() => {});
      }
      // 跨会话守卫：只处理当前会话的事件，避免切换后旧会话的迟到事件串入。
      if (e.sessionId !== sessionIdRef.current) return;
      // L0 乐观停止：已点停止后，冻结流式增量渲染（避免「已点停止还在吐字」）。
      // feed 由随后的 run_finished 用 DB 重建对账，不丢数据。
      if (
        stoppingRef.current &&
        (e.kind === "thinking_delta" ||
          e.kind === "message_delta" ||
          (e.kind === "tool_call" && e.status === "generating"))
      ) {
        return;
      }
      const eventKey = streamEventKey(e);
      if (eventKey) {
        if (processedStreamEventKeysRef.current.has(eventKey)) return;
        processedStreamEventKeysRef.current.add(eventKey);
      }
      if (e.kind === "thinking_delta") {
        setRunActivity((current) =>
          current ? { ...current, label: "正在思考" } : current,
        );
        upsertRow(
          "a:" + e.messageId,
          () => ({
            kind: "assistant",
            id: e.messageId,
            reasoning: "",
            content: "",
          }),
          (r) => {
            if (r.kind === "assistant")
              r.reasoning = (r.reasoning ?? "") + (e.text ?? "");
          },
        );
        streamingIdRef.current = e.messageId;
      } else if (e.kind === "message_delta") {
        setRunActivity((current) =>
          current ? { ...current, label: "正在生成回复" } : current,
        );
        upsertRow(
          "a:" + e.messageId,
          () => ({
            kind: "assistant",
            id: e.messageId,
            reasoning: "",
            content: "",
          }),
          (r) => {
            if (r.kind === "assistant") r.content += e.text ?? "";
          },
        );
        streamingIdRef.current = e.messageId;
      } else if (e.kind === "tool_call") {
        // 生成期（status=generating，模型在流式产出参数）与执行期（running）区分显示。
        const callStatus = e.status === "generating" ? "generating" : "running";
        setRunActivity((current) => ({
          label: toolActivityLabel(e.toolName, callStatus),
          startedAt: current?.startedAt ?? parseEpochSeconds(e.createdAt) ?? Date.now(),
        }));
        upsertRow(
          "t:" + (e.toolCallId ?? e.messageId),
          () => ({
            kind: "tool",
            id: e.toolCallId ?? e.messageId,
            toolCallId: e.toolCallId,
            toolName: e.toolName ?? "工具",
            input: e.text ?? "",
            output: null,
            startedAt: parseEpochSeconds(e.createdAt) ?? Date.now(),
            status: callStatus,
          }),
          (r) => {
            if (r.kind === "tool") {
              r.input = e.text ?? r.input;
              r.toolName = e.toolName ?? r.toolName;
              r.startedAt = r.startedAt ?? parseEpochSeconds(e.createdAt) ?? Date.now();
              r.status = callStatus;
            }
          },
        );
      } else if (e.kind === "tool_result") {
        const r = feedRef.current.map.get("t:" + (e.toolCallId ?? e.messageId));
        if (r && r.kind === "tool") {
          r.output = e.text ?? "";
          r.status = e.status === "failed" ? "failed" : "done";
          r.finishedAt = parseEpochSeconds(e.createdAt) ?? Date.now();
          r.startedAt = r.startedAt ?? r.finishedAt;
        }
        bump((n) => n + 1);
      } else if (e.kind === "context_compacted") {
        // 引擎自动压缩了较早历史：feed 出一行提示 + 刷新 context meter（轮内发生，不等轮末）。
        if (e.sessionId === sessionIdRef.current) {
          feedRef.current.list.push({
            kind: "divider",
            id: "auto-compact-" + Date.now(),
            content: e.text ?? "已自动压缩较早历史，上下文已精简。",
          });
          bump((n) => n + 1);
          refreshContextUsage(e.sessionId);
        }
      } else if (e.kind === "todos_updated") {
        setTodos(e.todos ?? []);
      } else if (e.kind === "artifacts_updated") {
        setArtifacts(e.artifacts ?? []);
      } else if (e.kind === "queued_tasks_updated") {
        // T70：队列变化（入队/排空/取消）→ 刷新排队条。
        void listSessionQueue(sessionIdRef.current!)
          .then(setQueuedTasks)
          .catch(() => {});
      } else if (e.kind === "message_failed") {
        // 模型调用失败：推一条 error 行即时反馈（run_finished 后会用 DB 重建，换成持久化那条）。
        streamingIdRef.current = null;
        setRunActivity(null);
        if (e.sessionId === sessionIdRef.current && e.text) {
          feedRef.current.list.push({
            kind: "error",
            id: "error-" + (e.messageId || Date.now()),
            content: e.text,
          });
        }
        bump((n) => n + 1);
      } else if (e.kind === "model_retrying") {
        // 自动重试中：临时显示一行（非持久；下次 run_finished 重建 feed 时消失）。
        setRunActivity((current) =>
          current ? { ...current, label: "正在重试模型调用" } : current,
        );
        if (e.sessionId === sessionIdRef.current && e.text) {
          feedRef.current.list.push({
            kind: "divider",
            id: "retrying-" + Date.now(),
            content: e.text,
          });
          bump((n) => n + 1);
        }
      } else if (e.kind === "message_completed") {
        // 只 bump，不覆盖、不重建：feed 已由事件增量建好。
        streamingIdRef.current = null;
        bump((n) => n + 1);
      } else if (
        e.kind === "run_started" ||
        e.kind === "run_finished" ||
        e.kind === "permission_required" ||
        e.kind === "ask_required" ||
        e.kind === "plan_required"
      ) {
        // 新一轮开始：清掉上一轮的快捷建议。
        if (e.kind === "run_started" && e.sessionId === sessionIdRef.current) {
          setSuggestions([]);
          setRunActivity({
            label: "正在思考",
            startedAt: parseEpochSeconds(e.createdAt) ?? Date.now(),
          });
        }
        if (e.kind === "run_finished" && e.status !== "parked") {
          // parked（派发专家后停泊）不清活动条——父仍在「委派进行中」，由下面 refetch 据
          // awaitingSubagent 设等待态，消除假死。
          setRunActivity(null);
        }
        // 子代理权限请求冒泡：child 的 permission_required（parentSessionId=当前会话）→ 取该 child 的
        // pending 在父会话弹可操作确认卡；决定提交给 child 会话由后端续跑 child（见 decideChild）。
        if (
          e.kind === "permission_required" &&
          e.parentSessionId &&
          e.parentSessionId === sessionIdRef.current
        ) {
          const childId = e.sessionId;
          const agentLabel = e.expertName ?? undefined;
          void getSession(childId).then((d) => {
            if (d?.pendingPermission) {
              setChildPending({ childId, agentLabel, pending: d.pendingPermission });
            }
          });
        }
        // 子代理提问冒泡：child 的 ask_required（parentSessionId=当前会话）→ 取其 pendingAsk 在父弹答题卡。
        if (
          e.kind === "ask_required" &&
          e.parentSessionId &&
          e.parentSessionId === sessionIdRef.current
        ) {
          const childId = e.sessionId;
          const agentLabel = e.expertName ?? undefined;
          void getSession(childId).then((d) => {
            if (d?.pendingAsk) {
              setChildAsk({ childId, agentLabel, ask: d.pendingAsk });
            }
          });
        }
        // child 运行结束：清掉其权限/提问冒泡卡（已批准续跑 / 已回答 / 取消 / 失败）。
        if (e.kind === "run_finished" && e.sessionId) {
          setChildPending((cur) => (cur && cur.childId === e.sessionId ? null : cur));
          setChildAsk((cur) => (cur && cur.childId === e.sessionId ? null : cur));
        }
        // L0 对账：本会话 run 结束 → 清乐观停止态（cancelled/done/failed 皆然）。
        if (e.kind === "run_finished" && e.sessionId === sessionIdRef.current) {
          setStopping(false);
        }
        // 控制事件：以 getSessionDetail 为单一事实源，重新同步 pending/todos/busy。
        // run_finished 时顺带用 DB 重建 feed，弥合刷新窗口丢失的 delta。
        const sid = e.sessionId;
        // parked（派发专家后停泊）不重建 feed：DB 里此时还没有 dispatch 的 tool 结果消息，
        // 重建会让「专家运行中」卡片消失。保留 live feed，等 child 完成回填后再随父续跑重建。
        const rebuild = e.kind === "run_finished" && e.status !== "parked";
        void getSession(sid).then((d) => {
          if (!d || sid !== sessionIdRef.current) return;
          setPending(d.pendingPermission ?? null);
          setAsk(d.pendingAsk ?? null);
          setPlan(d.pendingPlan ?? null);
          setTodos(d.todos ?? []);
          setArtifacts(d.artifacts ?? []);
          // 委派进行中（awaitingSubagent）也算忙，保持「等待专家处理」态、禁用 composer。
          setBusy(d.isRunning || isAwaitingSubagent(d));
          setRunActivity(activityFromSession(d));
          if (rebuild) {
            // run_finished：用最新 DB 重建 feed，并刷新 detail（含 messages，
            // 供按轮分组产物时把 artifact.messageId 映射到轮根 user 消息）。
            setDetail(d);
            rebuildFeed(d.messages, showCompletedProcess);
            streamingIdRef.current = null;
            // 本轮结束，token_usage 已落新行 → 刷新 context meter。
            refreshContextUsage(sid);
          }
          bump((n) => n + 1);
        });
      }
    }).then((u) => {
      if (cancelled) {
        u(); // effect 已清理但监听器现在才注册成功 → 立即退订，避免重复监听。
      } else {
        un = u;
      }
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  // 斜杠命令分发：均为前端动作，不走 submitUserInput。
  const runSlashCommand = async (parsed: ParsedCommand) => {
    try {
      switch (parsed.name) {
        case "/new":
          requestNewSession();
          break;
        case "/clear": {
          // 仅清当前视图（feed/todos），不删 DB；切走再回会从 DB 恢复。
          feedRef.current = { list: [], map: new Map() };
          setTodos([]);
          bump((n) => n + 1);
          break;
        }
        case "/rename": {
          if (!detail) break;
          const cur = detail.session.title;
          const name =
            parsed.args.join(" ").trim() ||
            (await messages.prompt({
              title: "重命名会话",
              message: "输入新的会话名称",
              defaultValue: cur,
              confirmText: "保存",
            })) ||
            "";
          if (name.trim()) {
            await renameSession(detail.session.id, name.trim());
            refreshSessions();
          }
          break;
        }
        case "/stop": {
          if (!detail) break;
          setStopping(true); // 乐观：立即进入「停止中」
          await stopSession(detail.session.id);
          break;
        }
        case "/memory": {
          const mems = await listMemories();
          const content = mems.length
            ? "## 长期记忆\n" + mems.map((m) => `- ${m.content}`).join("\n")
            : "（暂无长期记忆）";
          feedRef.current.list.push({
            kind: "assistant",
            id: "memory-" + Date.now(),
            content,
          });
          bump((n) => n + 1);
          break;
        }
        case "/compact": {
          await doCompact();
          break;
        }
        case "/plan": {
          await togglePlan();
          break;
        }
        case "/help": {
          const content = SLASH_COMMANDS.map(
            (c) => `- \`${c.usage}\` — ${c.description}`,
          ).join("\n");
          feedRef.current.list.push({
            kind: "assistant",
            id: "help-" + Date.now(),
            content,
          });
          bump((n) => n + 1);
          break;
        }
        default:
          break;
      }
    } catch (err) {
      console.error(err);
      notify.error(`命令 ${parsed.name} 执行失败`);
    }
  };

  const onSubmit = async (text: string): Promise<void> => {
    // 已知斜杠命令：拦截分发，不发消息。
    const parsed = parseSlashCommand(text);
    if (parsed?.command) {
      await runSlashCommand(parsed);
      return;
    }
    if (!detail) return;
    // T70：会话忙时后端会把消息入队（不进 feed、不改运行态）；空闲时才是即时发送。
    const wasRunning = busy;
    setSuggestions([]); // 发新消息即清掉上一轮建议。
    const uid = `tmp-${Date.now()}`;
    if (!wasRunning) {
      setBusy(true);
      setRunActivity({ label: "正在思考", startedAt: Date.now() });
      // 乐观追加用户行到 feed（不 setDetail 整体替换）。
      upsertRow(
        "u:" + uid,
        () => ({ kind: "user", id: uid, content: text }),
        () => {},
      );
    }
    try {
      const { session: next, queued } = await submitUserMessage(detail.session.id, text);
      if (queued) {
        // 后端入队（不论前端先前 wasRunning 以为忙不忙）：撤掉可能的乐观气泡——消息在排队条里、
        // 不在 feed。修复点：原先仅凭前端 wasRunning 决定是否显示气泡，但前后端「忙」态可能不一致
        //（队列排空边界/重连那一拍），导致乐观气泡成为与排队条并存的「孤儿已送达消息」。现按后端
        // 真实 queued 结果对账。wasRunning 为 true 时本就没插气泡，filter 为 no-op。
        const optimisticKey = "u:" + uid;
        feedRef.current.list = feedRef.current.list.filter((row) => rowKey(row) !== optimisticKey);
        feedRef.current.map.delete(optimisticKey);
        void listSessionQueue(detail.session.id)
          .then(setQueuedTasks)
          .catch(() => {});
        if (!wasRunning) {
          // 前端先前误判空闲、已乐观置「正在思考」；校正为在飞 run 的真实运行态。
          setBusy(next.isRunning);
          setRunActivity(activityFromSession(next));
        }
        // wasRunning 时运行态本就正确，保留 live 活动标签不动。
      } else {
        // 即时起跑：保留乐观气泡（run_finished 重建时同构替换），同步运行态。
        setBusy(next.isRunning);
        setRunActivity(activityFromSession(next));
      }
    } catch (err) {
      console.error(err);
      if (!wasRunning) {
        const optimisticKey = "u:" + uid;
        feedRef.current.list = feedRef.current.list.filter((row) => rowKey(row) !== optimisticKey);
        feedRef.current.map.delete(optimisticKey);
        setBusy(false);
        setRunActivity(null);
      }
      notify.error("发送失败：" + String(err));
      throw err;
    } finally {
      streamingIdRef.current = null; // 兜底：命令结束清思考中态。
      bump((n) => n + 1);
      refreshSessions(); // 刷新列表标题/排序（首条消息可能生成标题、updated_at 变化）。
    }
  };

  // T70：取消一个排队中的任务项（不影响在飞队头）。
  const onCancelQueued = async (itemId: string) => {
    if (!detail) return;
    try {
      const next = await cancelQueuedTask(detail.session.id, itemId);
      setQueuedTasks(next);
    } catch (err) {
      notify.error("取消失败：" + String(err));
    }
  };

  // 权限决定：允许/拒绝 → 引擎续跑，续跑期间新事件经现有订阅增量进 feed。
  // 若返回还有下一个 pending 则更新卡片，否则关卡片。
  // 把指定工具行就地标记为完成：ask_user 回答 / 拒绝权限 走命令层落结果、不经引擎 emit
  // tool_result，故该工具行（如「正在向用户提问…」）会卡在 running，需前端补更新为 done。
  const markToolDone = (toolCallId: string, output: string) => {
    const r = feedRef.current.map.get("t:" + toolCallId);
    if (r && r.kind === "tool") {
      r.status = "done";
      if (output) r.output = output;
      r.finishedAt = Date.now();
      r.startedAt = r.startedAt ?? r.finishedAt;
    }
  };

  const decide = async (approved: boolean) => {
    if (!detail || !pending) return;
    const current = pending;
    setPending(null); // 立即关卡片（决定已做出，续跑可能耗时，不让卡片滞留）。
    // 拒绝：命令层落「用户拒绝」结果、不发 tool_result，前端补标该工具行完成。
    // 批准：续跑会重新执行该工具并 emit tool_result，由事件更新，无需在此标记。
    if (!approved) markToolDone(current.toolCallId, "用户拒绝了该操作。");
    bump((n) => n + 1);
    setBusy(true);
    setRunActivity({ label: "正在思考", startedAt: Date.now() });
    try {
      const next = await submitPermissionDecision(
        detail.session.id,
        current.toolCallId,
        approved,
      );
      setBusy(next.isRunning);
      setRunActivity(activityFromSession(next));
    } catch (err) {
      console.error(err);
      setRunActivity(null);
    } finally {
      streamingIdRef.current = null; // 兜底：命令结束清思考中态。
      bump((n) => n + 1);
      refreshSessions();
    }
  };

  // 子代理权限决定：提交给 child 会话（后端按 subagent 续跑 child，完成后回填父并续跑父）。
  const decideChild = async (approved: boolean) => {
    const current = childPending;
    if (!current) return;
    setChildPending(null); // 立即关卡片。
    try {
      await submitPermissionDecision(current.childId, current.pending.toolCallId, approved);
    } catch (err) {
      notify.notify({ tone: "error", title: "提交失败", message: String(err) });
    }
  };

  // 子代理提问回答：提交给 child 会话（后端按 subagent 续跑该 child）。
  const answerChildAsk = async (answers: string[][]) => {
    const current = childAsk;
    if (!current) return;
    setChildAsk(null);
    try {
      await submitAskResponse(current.childId, current.ask.toolCallId, answers);
    } catch (err) {
      notify.notify({ tone: "error", title: "提交失败", message: String(err) });
    }
  };

  // ask_user 回答：点击即关卡片（对齐 5a 修复），续跑后更新两态。
  const answerAsk = async (answers: string[][]) => {
    if (!detail || !ask) return;
    const current = ask;
    setAsk(null);
    // ask_user 行不会收到 tool_result 事件，前端补标完成。
    markToolDone(current.toolCallId, "已回答");
    // 乐观插入「你的回答」气泡，立即可追溯；run_finished 用 DB 重建时同构替换。
    feedRef.current.list.push({
      kind: "askAnswer",
      id: "ask-ans-" + current.toolCallId,
      content: formatAskAnswers(current, answers),
    });
    bump((n) => n + 1);
    setBusy(true);
    setRunActivity({ label: "正在思考", startedAt: Date.now() });
    try {
      const next = await submitAskResponse(
        detail.session.id,
        current.toolCallId,
        answers,
      );
      setBusy(next.isRunning);
      setRunActivity(activityFromSession(next));
    } catch (err) {
      console.error(err);
      setRunActivity(null);
    } finally {
      streamingIdRef.current = null;
      bump((n) => n + 1);
      refreshSessions();
    }
  };

  // 取消回答：立即关卡片、停止本轮（后端落「已取消」结果解析掉 pending、不续跑），
  // 再用返回的最新会话重建 feed（含取消分隔线）。
  const cancelAsk = async () => {
    if (!detail || !ask) return;
    const current = ask;
    setAsk(null);
    markToolDone(current.toolCallId, "已取消");
    setBusy(false);
    setRunActivity(null);
    streamingIdRef.current = null;
    bump((n) => n + 1);
    try {
      const next = await cancelAskResponse(detail.session.id, current.toolCallId);
      setDetail(next);
      rebuildFeed(next.messages, showCompletedProcess);
      setPending(next.pendingPermission ?? null);
      setAsk(next.pendingAsk ?? null);
      setPlan(next.pendingPlan ?? null);
      setBusy(next.isRunning);
    } catch (err) {
      console.error(err);
      notify.error("取消回答失败：" + String(err));
    } finally {
      bump((n) => n + 1);
      refreshSessions();
    }
  };

  // 计划决定：批准执行 / 提交修改意见 → 引擎续跑。
  // 批准：续跑会执行计划（模式回 normal）；评论：模型修订后会再 propose_plan，卡片重弹。
  const decidePlan = async (approved: boolean, comment?: string) => {
    if (!detail || !plan) return;
    const current = plan; // 闭包捕获，setPlan(null) 后仍可读 toolCallId。
    setPlan(null); // 立即关卡片（决定已做出，续跑可能耗时）。
    // propose_plan 行不会再收到 tool_result，前端补标该工具行完成。
    markToolDone(current.toolCallId, approved ? "[已批准]" : "[已评论]");
    bump((n) => n + 1);
    setBusy(true);
    setRunActivity({ label: "正在思考", startedAt: Date.now() });
    try {
      const next = await submitPlanDecision(
        detail.session.id,
        current.toolCallId,
        approved,
        comment,
      );
      setBusy(next.isRunning);
      setRunActivity(activityFromSession(next));
      // 批准后引擎执行计划、模式回 normal；本地更新使 Composer/状态反映。
      if (approved) {
        setDetail((d) =>
          d ? { ...d, session: { ...d.session, mode: "normal" } } : d,
        );
      }
    } catch (err) {
      console.error(err);
      setRunActivity(null);
      notify.error("提交计划决定失败");
    } finally {
      streamingIdRef.current = null; // 兜底：命令结束清思考中态。
      bump((n) => n + 1);
      refreshSessions();
    }
  };

  // 切换会话权限模式：null 表示跟随全局默认。
  const switchPermissionMode = async (mode: PermissionMode | null) => {
    if (!detail) return;
    try {
      const next = await setSessionPermissionMode(detail.session.id, mode);
      setDetail(next);
    } catch (err) {
      console.error(err);
      notify.error("切换权限模式失败");
    }
  };

  // 选择工作目录：仅首次发送前可用；成功后用返回的最新会话更新 detail。
  const refreshRecents = () =>
    getRecentWorkspaces().then(setRecents).catch(console.error);

  // 进入会话时加载最近使用的目录（全局列表）。
  useEffect(() => {
    refreshRecents();
  }, [sessionId]);

  // 加载全局权限模式（用于继承态展示）。
  useEffect(() => {
    getGlobalPermissionMode().then(setGlobalPermMode).catch(console.error);
  }, []);

  // 订阅一轮结束后的快捷建议（仅当前会话）。
  useEffect(() => {
    let un: (() => void) | undefined;
    let cancelled = false;
    subscribeSessionSuggestions((payload) => {
      console.log(
        "[suggest] 收到事件",
        payload,
        "当前会话=",
        sessionIdRef.current,
      );
      if (payload.sessionId === sessionIdRef.current) {
        setSuggestions(payload.suggestions);
      }
    }).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  // 加载启用模型分组（供 Composer 模型下拉）；窗口聚焦时刷新，使设置页改动即时生效。
  useEffect(() => {
    const reload = () =>
      listEnabledModels().then(setModelGroups).catch(console.error);
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);

  // 加载项目列表，供 Composer 工作上下文下拉选择项目。
  useEffect(() => {
    const reload = () =>
      listProjects().then(setProjects).catch(console.error);
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);

  // 「桌面操作」功能总开关（全局设置）：决定是否在会话头提供入口；窗口聚焦时刷新使设置即时生效。
  useEffect(() => {
    const reload = () =>
      getComputerUseEnabled()
        .then(setComputerUseEnabled)
        .catch(console.error);
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);

  // 本会话是否出现过浏览器 / 桌面工具活动：即便功能开关关着，只要确有活动也渲染真实面板（而非引导）。
  const sidePanelActivity = useMemo(() => {
    let browser = false;
    let computer = false;
    for (const r of feedRef.current.list) {
      if (r.kind !== "tool") continue;
      if (r.toolName === "browser") browser = true;
      else if (r.toolName === "computer") computer = true;
    }
    return { browser, computer };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [feedVersion]);

  // feed 出现「浏览器 / 桌面」工具活动时自动切到对应 tab：仅当最近一条工具行是该工具、会话运行中、
  // 且该行未自动切过时触发——打开历史会话不误切；用户手动切走后，下一次新活动才会再切回。
  useEffect(() => {
    const list = feedRef.current.list;
    let last: { id: string; tool: SidePanelTab } | null = null;
    for (let i = list.length - 1; i >= 0; i--) {
      const r = list[i];
      if (r.kind !== "tool") continue;
      if (r.toolName === "browser") last = { id: r.id, tool: "browser" };
      else if (r.toolName === "computer") last = { id: r.id, tool: "computer" };
      break; // 只看最近一条工具行：当前正在操作的工具才决定切哪个 tab
    }
    if (!last) return;
    if (autoSwitchedRowRef.current === last.id) return;
    autoSwitchedRowRef.current = last.id;
    if (busy) setSidePanelTab(last.tool);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [feedVersion, busy]);

  // 「浏览器操作」功能总开关（全局设置）：决定是否在会话头提供入口；窗口聚焦时刷新使设置即时生效。
  useEffect(() => {
    const reload = () =>
      getBrowserUseEnabled()
        .then(setBrowserUseEnabled)
        .catch(console.error);
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);

  // 加载可选团队 + 散装 agent（供 Composer 角色槽）；窗口聚焦时刷新。
  useEffect(() => {
    const reload = () => {
      listActiveTeams().then(setTeams).catch(console.error);
      listExperts().then(setRoleExperts).catch(console.error);
      listAgents().then(setAgents).catch(console.error);
    };
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);

  // 选择会话角色（kind 空串 = 自由模式）；本地更新 detail 使下拉即时反映。
  const pickRole = async (kind: string, id: string) => {
    if (!detail) return;
    await setSessionRole(detail.session.id, kind, id);
    setDetail((d) =>
      d
        ? {
            ...d,
            session: {
              ...d.session,
              roleKind: (kind || null) as "expert" | "team" | null,
              roleId: id || null,
            },
          }
        : d,
    );
  };

  const pickAgent = async (agentId: string) => {
    if (!detail) return;
    await setSessionAgent(detail.session.id, agentId || null);
    setDetail((d) =>
      d ? { ...d, session: { ...d.session, agentId: agentId || null } } : d,
    );
  };

  // 切换计划模式（与 /plan 命令等价）；本地更新 mode，不向消息流追加提示。
  const togglePlan = async () => {
    if (!detail) return;
    const cur = detail.session.mode === "plan" ? "normal" : "plan";
    await setSessionMode(detail.session.id, cur);
    setDetail((d) => (d ? { ...d, session: { ...d.session, mode: cur } } : d));
    refreshSessions();
  };

  // 选择会话模型（null 表示用全局默认）；本地更新 detail 使下拉即时反映。
  const pickModel = async (modelId: string | null) => {
    if (!detail) return;
    try {
      await setSessionModel(detail.session.id, modelId);
      setDetail((prev) =>
        prev
          ? { ...prev, session: { ...prev.session, selectedModelId: modelId } }
          : prev,
      );
    } catch (err) {
      console.error(err);
      notify.error("设置模型失败：" + String(err));
    }
  };

  const pickWorkspace = async () => {
    if (!detail) return;
    try {
      const dir = await pickDirectory();
      if (!dir) return;
      if (detail.session.agentId) await setSessionAgent(detail.session.id, null);
      const next = await setSessionWorkspace(detail.session.id, dir);
      setDetail({ ...next, session: { ...next.session, agentId: null } });
      refreshRecents();
      notify.success("已设置工作目录");
    } catch (err) {
      console.error(err);
      notify.error("设置工作目录失败：" + String(err));
    }
  };

  const refreshWorkspaceFiles = useCallback(async () => {
    const sid = sessionIdRef.current;
    if (!sid) return;
    setWsFilesLoading(true);
    setWsFilesError(null);
    try {
      const list = await listSessionWorkspaceFiles(sid);
      setWsFiles(list);
    } catch (err) {
      console.error(err);
      setWsFilesError(String(err));
    } finally {
      setWsFilesLoading(false);
    }
  }, []);

  // 工作空间 tab 自动刷新：激活或运行态变化时拉一次；运行中每 3s 轮询一次
  //（捕捉 agent 写入但未登记为产物的文件），运行结束（busy: true→false）再刷新一次拿到最终结果。
  useEffect(() => {
    if (sidePanelTab !== "workspace") return;
    void refreshWorkspaceFiles();
    if (!busy) return;
    const timer = setInterval(() => void refreshWorkspaceFiles(), 3000);
    return () => clearInterval(timer);
  }, [sidePanelTab, busy, refreshWorkspaceFiles]);

  // 产物更新（新文件落盘）时，若正停留在工作空间 tab 则重拉。
  useEffect(() => {
    if (sidePanelTab === "workspace") void refreshWorkspaceFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [artifacts]);

  const handleOpenWorkspace = async () => {
    if (!detail) return;
    try {
      await openSessionWorkspace(detail.session.id);
    } catch (err) {
      console.error(err);
      notify.error("打开工作目录失败：" + String(err));
    }
  };

  // 点击工作空间文件 → 右侧预览抽屉。命中已登记产物则沿用其元数据，否则用路径合成一个轻量产物
  //（预览抽屉只依赖 path：以 read_artifact 沙箱读取该会话工作目录内文件）。
  const handlePreviewWorkspaceFile = (relPath: string) => {
    const existing = artifacts.find((a) => a.path === relPath);
    setPreviewArtifact(
      existing ?? { path: relPath, title: relPath, kind: "", createdAt: "" },
    );
  };

  // 添加附件：弹文件选择器 → 纳入会话工作目录 → 返回可被 agent 访问的相对路径。
  const onAttachFile = async (): Promise<string | null> => {
    if (!detail) return null;
    const id = detail.session.id;
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

  // 粘贴/拖拽进来的文件或图片：读字节 → 写入工作目录 attachments/ → 返回相对路径。
  const onPasteFile = async (file: File): Promise<string | null> => {
    if (!detail) return null;
    const id = detail.session.id;
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

  // 从「最近使用的目录」子菜单选择：直接以该路径设置工作目录。
  const pickRecent = async (path: string) => {
    if (!detail) return;
    try {
      if (detail.session.agentId) await setSessionAgent(detail.session.id, null);
      const next = await setSessionWorkspace(detail.session.id, path);
      setDetail({ ...next, session: { ...next.session, agentId: null } });
      refreshRecents();
      notify.success("已设置工作目录");
    } catch (err) {
      console.error(err);
      notify.error("设置工作目录失败：" + String(err));
    }
  };

  if (sessionId == null)
    return (
      <div className="grid h-full place-items-center p-6 text-foreground-muted">
        选择或新建会话
      </div>
    );
  if (sessionLoadStatus === "loading")
    return (
      <div className="grid h-full place-items-center p-6 text-sm text-foreground-muted">
        加载中…
      </div>
    );
  if (sessionLoadStatus === "missing")
    return (
      <div className="grid h-full place-items-center p-6">
        <div className="flex w-full max-w-[360px] flex-col items-center text-center">
          <div className="text-base font-semibold text-foreground">
            会话不存在
          </div>
          <div className="mt-2 text-sm leading-6 text-foreground-muted">
            这个会话可能已被删除，或当前列表还指向旧记录。
          </div>
          <div className="mt-5 flex items-center gap-2">
            {/* <button
              type="button"
              className="rounded-md border border-border-subtle bg-card px-3 py-2 text-sm font-medium text-foreground-secondary transition hover:bg-accent hover:text-foreground"
              onClick={() => refreshSessions()}
            >
              刷新列表
            </button> */}
            <Button tone="primary" onClick={() => requestNewSession()}>
              <CirclePlus className="mr-1" aria-hidden="true" />
              新建会话
            </Button>
          </div>
        </div>
      </div>
    );
  if (sessionLoadStatus === "error")
    return (
      <div className="grid h-full place-items-center p-6">
        <div className="flex w-full max-w-[360px] flex-col items-center text-center">
          <div className="text-base font-semibold text-foreground">
            会话加载失败
          </div>
          <div className="mt-2 break-words text-sm leading-6 text-foreground-muted">
            {sessionLoadError || "请刷新会话列表后重试。"}
          </div>
          <button
            type="button"
            className="mt-5 rounded-md border border-border-subtle bg-card px-3 py-2 text-sm font-medium text-foreground-secondary transition hover:bg-accent hover:text-foreground"
            onClick={() => refreshSessions()}
          >
            刷新列表
          </button>
        </div>
      </div>
    );
  if (!detail)
    return (
      <div className="grid h-full place-items-center p-6 text-sm text-foreground-muted">
        加载中…
      </div>
    );

  // 工作目录展示：显式选过 → 显示目录名（非全路径）；未选 → 「默认工作目录」/composer 不显示路径。
  const selectedDir = detail.session.workingDir?.trim() || "";
  const wsName = selectedDir ? baseName(selectedDir) : undefined;
  const wsLabel = wsName ?? "默认工作目录";
  const sessionAgentId = detail.session.agentId
    ? (agents.find((a) => a.id === detail.session.agentId)?.id ?? null)
    : null;
  const monitorCollapsed = collapsedMonitorSessionId === detail.session.id;
  // 子代理（child）会话：只读查看，不提供输入框；底部用占位条 + 「返回主会话」入口。
  const isSubagentSession = detail.session.origin === "subagent";
  const parentSessionId = detail.session.parentSessionId ?? null;
  // 工具动作流的「运行中」信号：会话忙且不处于权限/反问/计划等待态。
  const running = busy && !pending && !ask && !plan;
  // 右侧任务面板 tab：监控常驻；浏览器/桌面仅主会话提供（子代理会话不适用）。
  const sideTabs: SidePanelTabDef[] = [
    { key: "monitor", label: "监控", icon: <ListChecks className="h-[14px] w-[14px]" aria-hidden="true" /> },
    { key: "workspace", label: "工作空间", icon: <FolderTree className="h-[14px] w-[14px]" aria-hidden="true" /> },
  ];
  if (!isSubagentSession) {
    sideTabs.push(
      { key: "browser", label: "浏览器", icon: <Globe className="h-[14px] w-[14px]" aria-hidden="true" /> },
      { key: "computer", label: "桌面", icon: <MonitorPlay className="h-[14px] w-[14px]" aria-hidden="true" /> },
    );
  }

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-row ">
      <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
        <div className="session-header flex min-w-0 items-center justify-between gap-3 px-4 py-4 text-sm font-semibold border-b border-border">
          <div className="min-w-0">
            {detail?.session.title || "会话"}
            {/* <span className="block truncate text-xs font-normal text-foreground-muted">
              {detail?.session.id}
            </span> */}
          </div>
          {/* 桌面/浏览器面板入口已并入右侧任务面板的 tab 条（见 SessionSidePanel），此处不再放独立 toggle。 */}
          {monitorCollapsed && (
            <Tooltip content="展开任务面板">
              <button
                type="button"
                aria-label="展开任务面板"
                className="absolute right-3 grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-foreground"
                onClick={() => setCollapsedMonitorSessionId(null)}
              >
                <PanelRightOpen className="h-[14px] w-[14px]" aria-hidden="true" />
              </button>
            </Tooltip>
          )}
        </div>
        
        <div className="session-body flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
            <div
              ref={feedScrollRef}
              onScroll={handleFeedScroll}
              className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden"
            >
              <div className="min-w-0 max-w-full px-2 pt-2">
                <MessageFeed
                sessionId={detail.session.id}
                onAddQuote={(text) => composerRef.current?.addQuote(text)}
                rows={feedRef.current.list}
                streamingId={streamingIdRef.current}
                thinking={busy && !pending && !ask && !plan}
                artifactsByRound={artifactsByRound}
                onOpenArtifact={setPreviewArtifact}
                resolvedWorkingDir={detail.resolvedWorkingDir}
                retryDisabled={busy}
                agentDisplayNames={agentDisplayNames}
                onDispatchAgentClick={(toolCallId, expertName) => {
                  // 点派发卡 → 进入该子代理会话（完整 SessionPage，可面包屑返回主会话）。
                  void expertName;
                  const live = childByCallRef.current.get(toolCallId);
                  if (live) {
                    openSession(live);
                    return;
                  }
                  void findChildSession(detail.session.id, toolCallId).then(
                    (childId) => {
                      if (childId) openSession(childId);
                      else notify.error("没找到该专家的子会话（可能尚未启动）");
                    },
                  );
                }}
                onRetry={() => {
                  if (!detail || busy) return;
                  setBusy(true);
                  setRunActivity({ label: "正在思考", startedAt: Date.now() });
                  void retrySession(detail.session.id)
                    .then((next) => {
                      setBusy(next.isRunning);
                      setRunActivity(activityFromSession(next));
                    })
                    .catch((err) => {
                      console.error(err);
                      notify.error("重试失败：" + String(err));
                      setBusy(false);
                      setRunActivity(null);
                    });
                }}
              />
            </div>
          </div>
            <MessageFeedNav
              scrollRef={feedScrollRef}
              items={feedNavItems}
              onOpenArtifact={setPreviewArtifact}
            />
          </div>
          {plan && (
            <SessionPlanCard plan={plan} busy={busy} onDecide={decidePlan} />
          )}
          <div className="session-composer-dock relative shrink-0">
            <div className="session-transient-stack pointer-events-none absolute inset-x-0 bottom-full z-20 flex flex-col">
              {pending && (
                <div className="pointer-events-auto">
                  <SessionPermissionCard
                    pending={pending}
                    busy={busy}
                    onDecide={decide}
                  />
                </div>
              )}
              {childPending && (
                <div className="pointer-events-auto">
                  <SessionPermissionCard
                    pending={childPending.pending}
                    busy={false}
                    onDecide={(approved) => void decideChild(approved)}
                    agentLabel={childPending.agentLabel}
                  />
                </div>
              )}
              {ask && (
                <div className="pointer-events-auto">
                  <SessionAskCard
                    ask={ask}
                    busy={busy}
                    onAnswer={answerAsk}
                    onCancel={() => void cancelAsk()}
                  />
                </div>
              )}
              {childAsk && (
                <div className="pointer-events-auto">
                  <SessionAskCard
                    ask={childAsk.ask}
                    busy={false}
                    onAnswer={(answers) => void answerChildAsk(answers)}
                    agentLabel={childAsk.agentLabel}
                  />
                </div>
              )}
            </div>
            {isSubagentSession ? (
              <div className="flex items-center justify-center px-4 pb-4 pt-1">
                <div className="flex w-full max-w-[760px] items-center justify-center rounded-2xl border border-border-subtle bg-surface px-4 py-3 text-[13px] text-foreground-muted">
                  {parentSessionId && (
                    <button
                      type="button"
                      onClick={() => openSession(parentSessionId)}
                      className="ml-3 inline-flex shrink-0 items-center gap-1 rounded-full border border-border bg-background px-3 py-1.5 text-[13px] font-medium text-foreground transition hover:bg-accent"
                    >
                      <CornerDownLeft className="h-3.5 w-3.5" aria-hidden="true" /> 返回主会话继续对话
                    </button>
                  )}
                </div>
              </div>
            ) : (
            <Composer
              sessionId={detail.session.id}
              ref={composerRef}
              queuedTasks={queuedTasks}
              onCancelQueued={onCancelQueued}
              // T70：忙（busy）不再禁用输入——允许 Enter 把新消息入队；仅 pending/ask/plan 这类
              // 需用户先决定的交互态才硬禁用。running 仍据 busy 控制 STOP 按钮。
              disabled={pending !== null || ask !== null || plan !== null}
              onSubmit={onSubmit}
              running={busy && !pending && !ask}
              stopping={stopping}
              onStop={() => {
                if (!detail) return;
                setStopping(true); // 乐观：立即进入「停止中」，不等后端撞检查点
                stopSession(detail.session.id).catch(console.error);
              }}
              workspaceName={wsName}
              workspaceLocked={!!detail?.session.projectId}
              workspacePath={detail?.resolvedWorkingDir}
              projects={projects}
              selectedProjectId={detail?.session.projectId ?? null}
              onPickProject={(projectId) => enterDraftWithProject(projectId)}
              agents={agents}
              selectedAgentId={sessionAgentId}
              onPickAgent={(id) => void pickAgent(id)}
              onPickWorkspace={pickWorkspace}
              recentWorkspaces={recents}
              onPickRecent={pickRecent}
              modelGroups={modelGroups}
              selectedModelId={detail?.session.selectedModelId ?? null}
              onPickModel={(id) => void pickModel(id)}
              teams={teams}
              roleExperts={roleExperts}
              roleKind={detail?.session.roleKind ?? ""}
              roleId={detail?.session.roleId ?? ""}
              onPickRole={(detail?.session.projectId || sessionAgentId) ? undefined : (k, i) => void pickRole(k, i)}
              planMode={detail?.session.mode === "plan"}
              onTogglePlan={() => void togglePlan()}
              permissionMode={detail?.session.permissionMode ?? null}
              globalPermMode={globalPermMode}
              onChangePermission={(m) => void switchPermissionMode(m)}
              onAttachFile={onAttachFile}
              onPasteFile={onPasteFile}
              suggestions={suggestions}
              onClearSuggestions={() => setSuggestions([])}
              runActivity={runActivity}
              contextUsage={contextUsage}
              sessionUsage={sessionUsage}
              onCompact={() => void doCompact()}
              compacting={compacting}
            />
            )}
          </div>
        </div>
      </div>
      {/* 右侧任务面板：监控 / 浏览器 / 桌面合并为一块，按 tab 切换；操作浏览器/桌面时自动切到对应 tab。 */}
      <div
        aria-hidden={monitorCollapsed}
        className={`h-full min-h-0 shrink-0 overflow-hidden transition-[width,opacity] duration-150 ${
          monitorCollapsed ? "pointer-events-none w-0 opacity-0" : "w-[320px] opacity-100"
        }`}
      >
        <SessionSidePanel
          tab={sidePanelTab}
          tabs={sideTabs}
          onTab={setSidePanelTab}
          onCollapse={() => setCollapsedMonitorSessionId(detail.session.id)}
        >
          {sidePanelTab === "monitor" && (
            <SessionMonitorPanel
              embedded
              todos={todos}
              childAgents={childAgents}
              childSteps={childSteps}
              onOpenChildAgent={(id) => openSession(id)}
              onCancelChildAgent={(id) => {
                void cancelChild(id)
                  .then(() => {
                    const sid = sessionIdRef.current;
                    if (sid) void listSessionChildren(sid).then(setChildAgents);
                  })
                  .catch((err) => notify.notify({ tone: "error", title: "取消失败", message: String(err) }));
              }}
              onCollapse={() => setCollapsedMonitorSessionId(detail.session.id)}
              sessionId={detail.session.id}
              projectId={isSubagentSession ? undefined : (detail?.session.projectId ?? undefined)}
              teamId={isSubagentSession ? undefined : (detail?.session.roleKind === "team" ? (detail?.session.roleId ?? undefined) : undefined)}
            />
          )}
          {sidePanelTab === "workspace" && (
            <WorkspaceTab
              workspaceLabel={wsLabel}
              workspacePath={detail?.resolvedWorkingDir}
              files={wsFiles}
              truncated={wsFiles.length >= 200}
              loading={wsFilesLoading}
              error={wsFilesError}
              artifacts={artifacts}
              onOpenDir={handleOpenWorkspace}
              onPreviewFile={handlePreviewWorkspaceFile}
              onRefresh={() => void refreshWorkspaceFiles()}
            />
          )}
          {sidePanelTab === "browser" && !isSubagentSession &&
            (browserUseEnabled || sidePanelActivity.browser ? (
              <BrowserPanel
                embedded
                sessionId={detail.session.id}
                rows={feedRef.current.list}
                feedVersion={feedVersion}
                running={running}
              />
            ) : (
              <FeatureOffHint name="浏览器操作" />
            ))}
          {sidePanelTab === "computer" && !isSubagentSession &&
            (computerUseEnabled || sidePanelActivity.computer ? (
              <ComputerPanel
                embedded
                sessionId={detail.session.id}
                rows={feedRef.current.list}
                feedVersion={feedVersion}
                running={running}
              />
            ) : (
              <FeatureOffHint name="桌面操作" />
            ))}
        </SessionSidePanel>
      </div>
      <ArtifactPreviewDrawer
        sessionId={detail.session.id}
        artifact={previewArtifact}
        resolvedWorkingDir={detail.resolvedWorkingDir}
        onClose={() => setPreviewArtifact(null)}
      />
    </div>
  );
}

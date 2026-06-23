/** 会话级权限模式（与 Rust PermissionMode 对齐）。 */
export type PermissionMode = "manual" | "auto" | "full";

/** Provider 连通性检查结果（与 Rust ProviderCheckResult 对齐，camelCase）。 */
export interface ProviderCheckResult {
  status: string;
  detail: string;
  checkedAt: string;
}

/** 厂商去敏投影（与 Rust ProviderView 对齐）。 */
export interface Provider {
  id: string;
  name: string;
  baseUrl: string;
  hasSecret: boolean;
  secretHint: string | null;
  enabled: boolean;
  lastCheck: ProviderCheckResult | null;
  sortOrder: number;
}

/** 厂商写入输入（与 Rust ProviderInput 对齐）。apiKey：null 保持，""清除，非空设置。 */
export interface ProviderInput {
  id: string | null;
  name: string;
  baseUrl: string;
  apiKey: string | null;
  enabled: boolean;
}

/** 模型去敏投影（与 Rust ModelView 对齐）。 */
export interface ModelEntry {
  id: string;
  providerId: string;
  model: string;
  displayName: string | null;
  enabled: boolean;
  isDefault: boolean;
  sortOrder: number;
  /** 上下文窗口上限（token）覆盖；null 表示用内置查表。 */
  contextLimit: number | null;
}

/** 模型写入输入（与 Rust ModelInput 对齐）。 */
export interface ModelInput {
  id: string | null;
  providerId: string;
  model: string;
  displayName: string | null;
  enabled: boolean;
  /** 上下文窗口上限（token）覆盖；null/省略表示沿用内置查表。 */
  contextLimit?: number | null;
}

/** Composer 的「启用厂商 → 启用模型」分组（与 Rust EnabledProviderModels 对齐）。 */
export interface EnabledProviderModels {
  providerId: string;
  providerName: string;
  models: ModelEntry[];
}

/** 会话（与 Rust Session 对齐，camelCase）。 */
export interface SessionInfo {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  pinned?: boolean;
  groupId?: string | null;
  /** 会话模式："normal" | "plan"（plan 模式下 agent 先只读调研、提交计划待批准）。 */
  mode?: string;
  /** 用户为该会话显式选择的工作目录；缺省（null/未定义）表示用默认目录。 */
  workingDir?: string | null;
  /** 该会话的权限模式；null/未定义表示跟随全局配置。 */
  permissionMode?: PermissionMode | null;
  /** 会话选中的模型 id；缺省表示用全局默认。 */
  selectedModelId?: string | null;
  /** 会话来源："user"（默认）| "scheduled"（定时任务）| 预留 "im" 等。SessionManager 仅展示 user。 */
  origin?: string;
  /** 是否草稿会话（隐藏在「草稿」区，不进任务列表）。 */
  isDraft?: boolean;
  /** 草稿暂存内容（Composer 序列化串）；非草稿为空。 */
  draftContent?: string;
  /** 上一轮结束生成的快捷建议（持久化，供 reload/切会话回显）。 */
  lastSuggestions?: string[];
  /** 该会话当前是否有 run 在后台运行。 */
  isRunning?: boolean;
  /** 当前 run 的开始时间（Unix epoch 秒字符串）。 */
  runStartedAt?: string | null;
  /** 父 run 停泊态：非空 = 正在等待该 child（专家）子运行完成（委派进行中）。 */
  awaitingSubagent?: string | null;
  /** T59：所属项目 id（项目会话及其 child 非空）；普通会话为 null。 */
  projectId?: string | null;
  /** 所属持久智能体 id；普通会话/项目会话为 null。 */
  agentId?: string | null;
  /** 运行角色类型：expert/team；空/缺省 = 自由模式。 */
  roleKind?: "expert" | "team" | null;
  /** 运行角色 id：kind="expert" 时为专家 id；kind="team" 时为 team id。 */
  roleId?: string | null;
  /** 父会话 id：origin="subagent"（子代理运行）时非空，供「返回主会话」。 */
  parentSessionId?: string | null;
}

/** 会话分组（与 Rust SessionGroup 对齐，camelCase；后端自动配色）。 */
export interface SessionGroup {
  id: string;
  label: string;
  colorKey: string;
  createdAt: string;
  builtIn?: boolean;
  sortOrder?: number;
}

/** 消息（与 Rust Message 对齐，camelCase）。 */
export interface Message {
  id: string;
  sessionId: string;
  // "compaction"：压缩分隔提示（渲染成分隔线）；"error"：模型调用失败提示（渲染成错误块）；
  // "stopped"：手动停止标记（渲染成分隔线）。三者都不是真实对话消息。
  role: "user" | "assistant" | "tool" | "compaction" | "error" | "stopped";
  content: string;
  reasoning?: string;
  toolCallsJson?: string;
  toolCallId?: string;
  toolName?: string;
  /** tool 消息的执行状态："done" | "failed"（持久化，供 reload 显示失败态）。 */
  toolStatus?: string;
  createdAt: string;
}

/** 待确认的风险工具调用（引擎暂停时非空，与 Rust PendingPermission 对齐，camelCase）。 */
export interface PendingPermission {
  /** 该暂停归属的会话 id（顶层会话或某子运行的子会话）。 */
  sessionId: string;
  toolCallId: string;
  toolName: string;
  input: string;
}

/** ask_user 单个问题（与 Rust AskQuestion 对齐，camelCase）。 */
export interface AskQuestion {
  header: string;
  question: string;
  multiSelect: boolean;
  options: string[];
}

/** 待回答的模型提问（与 Rust PendingAsk 对齐）。一次可含多题，前端分页作答。 */
export interface PendingAsk {
  /** 该暂停归属的会话 id（顶层会话或某子运行的子会话）。 */
  sessionId: string;
  toolCallId: string;
  questions: AskQuestion[];
}

/** 待批准的计划（plan 模式下引擎调 propose_plan 暂停时非空，与 Rust PendingPlan 对齐，camelCase）。 */
export interface PendingPlan {
  /** 该暂停归属的会话 id（顶层会话或某子运行的子会话）。 */
  sessionId: string;
  toolCallId: string;
  title: string;
  summary?: string;
  planMarkdown: string;
  riskLevel?: string;
}

/** Todo 待办项（与 Rust TodoItem 对齐，camelCase）。 */
/** 一个产物：Agent 登记的文件（与 Rust Artifact 对齐，camelCase）。 */
export interface Artifact {
  path: string;
  title: string;
  /** 分类：final（最终交付文件）| working（脚本/中间文件）。 */
  kind: string;
  messageId?: string | null;
  toolCallId?: string | null;
  createdAt: string;
}

export interface TodoItem {
  id: number;
  content: string;
  status: string; // "pending" | "in_progress" | "completed"
}

/** 会话详情：会话头 + 消息列表 + 可选的待确认权限 + 可选的待回答提问 + 当前待办清单。 */
export interface Session {
  session: SessionInfo;
  messages: Message[];
  pendingPermission?: PendingPermission;
  pendingAsk?: PendingAsk;
  pendingPlan?: PendingPlan;
  todos?: TodoItem[];
  /** 解析后的实际工作目录绝对路径（后端回填，始终有值）。 */
  resolvedWorkingDir?: string;
  /** 已登记产物列表。 */
  artifacts?: Artifact[];
  /** 该会话当前是否有 run 在后台运行（与 Rust Session.is_running 对齐）。 */
  isRunning: boolean;
}

/** 流式期间按事件增量构建的 live 项（按 id 索引 find-or-create-upsert）。 */
export type LiveItem =
  | { kind: "assistant"; id: string; reasoning: string; content: string }
  | {
      kind: "tool";
      id: string;
      toolName: string;
      input: string;
      output: string | null;
      startedAt?: number;
      finishedAt?: number;
      // generating：模型正在流式生成该工具调用的参数（区别于 running=实际执行）。
      status: "generating" | "running" | "done" | "failed";
    };

/** SimpleFeed 统一渲染的行（持久 + live 同构）。 */
export type FeedRow =
  | { kind: "user"; id: string; content: string }
  | { kind: "assistant"; id: string; reasoning?: string; content: string }
  | { kind: "divider"; id: string; content: string }
  | { kind: "error"; id: string; content: string }
  // 用户对 ask_user 反问的回答（持久化自 ask_user 工具结果），独立展示便于追溯。
  | { kind: "askAnswer"; id: string; content: string }
  | {
      kind: "tool";
      id: string;
      /** 工具调用 id（持久化行 id=消息 id，与 toolCallId 不同；dispatch 卡片定位 child 用此）。 */
      toolCallId?: string;
      toolName: string;
      input: string;
      output: string | null;
      startedAt?: number;
      finishedAt?: number;
      // generating：模型正在流式生成该工具调用的参数（区别于 running=实际执行）。
      status: "generating" | "running" | "done" | "failed";
    };

/** 技能来源（与 Rust SkillSource 对齐）。 */
export type SkillSource = "builtin" | "user";

/** 技能列表项（与 Rust SkillSummary 对齐，camelCase）。 */
export interface Skill {
  id: string;
  source: SkillSource;
  name: string;
  description: string;
  enabled: boolean;
  installedAt: string;
  /** 归属插件 id；null = 散装技能。 */
  pluginId: string | null;
  /** 是否对用户可见/可调（false 为内部知识库技能）。 */
  userInvocable: boolean;
  /** mention 菜单输入提示。 */
  argumentHint: string | null;
  /** 「我的」用户自定义分组 id（未分组为 null）。 */
  groupId?: string | null;
}

/** 伴随体（agent 实例，与 Rust AgentRecord 对齐 camelCase）：软复制 expert 指令 + 引用其技能 + 私有记忆 + 跨会话身份。 */
export interface Agent {
  id: string;
  name: string;
  /** 软复制自源 expert 的指令（= SOUL 可演化人格，可编辑）。 */
  instructions: string;
  /** IDENTITY 稳定锚（名字/角色/硬边界）；只人工编辑，不被自我演化改动。存量为空。 */
  identity: string;
  /** T73：是否允许自我演化（反思写回人格）。默认 false。 */
  evolutionEnabled: boolean;
  /** T73：上次触发反思的 epoch 秒；null=从未。 */
  lastReflectionAt?: number | null;
  tools: string[];
  modelTier: string;
  /** 源 expert 名（技能引用键 + 溯源）；null = 仅全局技能池。 */
  sourceExpertId?: string | null;
  /** 专属工作目录（持久智能体）：绑定该智能体的会话未显式设目录时默认用它。null/空 = 会话级默认目录。 */
  workingDir?: string | null;
  displayName?: string | null;
  profession?: string | null;
  avatar?: string | null;
  color?: string | null;
  enabled: boolean;
  groupId?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** SOUL 版本（T73，与 Rust SoulVersion 对齐）：活跃 / 待批准 / 历史归档。 */
export interface SoulVersion {
  id: string;
  agentId: string;
  soul: string;
  /** "active" | "pending" | "archived" */
  status: "active" | "pending" | "archived";
  summary: string;
  /** "seed" | "reflection" | "manual" */
  source: string;
  createdAt: string;
}

/** 项目（与 Rust Project 对齐）。 */
export interface Project {
  id: string;
  name: string;
  description: string;
  workspaceDir?: string | null;
  /** 成员任务 run 权限模式：manual|auto|full（默认 manual）。 */
  permissionMode: "manual" | "auto" | "full";
  /** 项目章程/PM 指令：运行时合成为 lead 人格。 */
  instructions: string;
  createdAt: string;
  updatedAt: string;
}

/** 「我的」用户自定义分组（与 Rust Group 对齐）。 */
export interface Group {
  id: string;
  /** "agent" | "team"。 */
  kind: string;
  name: string;
  sort: number;
  createdAt: string;
}

/** 技能目录文件项。 */
export interface SkillFile {
  relPath: string;
  isDir: boolean;
}

/** 技能详情（元数据 + SKILL.md 原文 + 文件列表）。 */
export interface SkillDetail {
  skill: Skill;
  skillMd: string;
  files: SkillFile[];
}

/** 单文件预览结果。 */
export interface SkillFilePreview {
  kind: "markdown" | "text" | "image" | "binary";
  text: string | null;
  dataUrl: string | null;
  name: string;
}

/** 流式事件（与 Rust AgentStreamEvent 对齐，camelCase）。 */
export interface AgentStreamEvent {
  kind: string;
  sessionId: string;
  messageId: string;
  sequence: number;
  text?: string;
  status?: string;
  toolName?: string;
  toolLabel?: string;
  toolCallId?: string;
  todos?: TodoItem[];
  /** 整组产物（仅 artifacts_updated 事件携带）。 */
  artifacts?: Artifact[];
  /** 子运行来源标记（仅 child 子运行事件携带）：前端据此把 child 事件路由到专家面板。 */
  parentSessionId?: string;
  parentToolCallId?: string;
  expertName?: string;
  createdAt: string;
}

/** T70：会话任务队列项（与 Rust session::task_queue::SessionTaskItem 对齐，camelCase）。 */
export interface QueuedTask {
  itemId: string;
  kind: "user_message" | "agent_task";
  payload: string;
  toolCallId?: string;
  parentSessionId?: string;
  status: "running" | "queued";
  enqueuedAt: string;
}

export interface RunActivity {
  label: string;
  startedAt: number;
}

/** 用量范围（与后端 get_usage_analytics range 对齐）。 */
export type UsageRange = "all" | "30d" | "7d";

export interface UsageTotals {
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
  calls: number;
}

export interface UsageDateBucket {
  date: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
  calls: number;
}

export interface UsageModelRow {
  provider: string;
  model: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
  calls: number;
}

export interface UsageSessionRow {
  sessionId: string;
  title: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
  calls: number;
}

export interface UsageProjectRow {
  projectId: string;
  name: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
  calls: number;
}

export interface UsageAgentRow {
  agentId: string;
  name: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
  calls: number;
}

/** 单条消息的用量（会话→消息二层展开；与 Rust UsageMessageRow 对齐）。 */
export interface UsageMessageRow {
  messageId: string;
  snippet: string;
  role: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
  ts: string;
}

export interface UsageHourBucket {
  hour: number;
  total: number;
  calls: number;
}

export interface UsageDateModel {
  date: string;
  model: string;
  total: number;
}

export interface UsageCallRow {
  ts: string;
  provider: string;
  model: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheCreate: number;
  total: number;
}

/** 模型调用日志筛选条件（与 Rust CallLogFilter 对齐，camelCase）。 */
export interface CallLogFilter {
  sessionId?: string;
  model?: string;
  provider?: string;
  usageType?: string;
  status?: string;
  since?: number;
  until?: number;
  search?: string;
  limit?: number;
  offset?: number;
}

/** 调用日志列表行（摘要，不含完整 payload）。 */
export interface CallLogRow {
  id: string;
  createdAt: string;
  sessionId?: string | null;
  usageType: string;
  provider: string;
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreateTokens: number;
  latencyMs: number;
  status: string;
  truncated: boolean;
}

/** 调用日志明细（含完整请求/响应 payload）。 */
export interface CallLogDetail extends CallLogRow {
  messageId?: string | null;
  parentSessionId?: string | null;
  parentToolCallId?: string | null;
  expertName?: string | null;
  requestJson: string;
  responseText?: string | null;
  responseToolCallsJson?: string | null;
  reasoningText?: string | null;
  finishReason?: string | null;
  errorMessage?: string | null;
  errorClass?: string | null;
  httpStatus?: number | null;
  requestBytes: number;
}

export interface CallLogStats {
  count: number;
  bytes: number;
}

/** 用量分析聚合（与 Rust UsageAnalyticsView 对齐，camelCase）。 */
export interface UsageAnalyticsView {
  totals: UsageTotals;
  byDate: UsageDateBucket[];
  byModel: UsageModelRow[];
  bySession: UsageSessionRow[];
  byProject: UsageProjectRow[];
  byAgent: UsageAgentRow[];
  byHour: UsageHourBucket[];
  byDateModel: UsageDateModel[];
  recentCalls: UsageCallRow[];
  recentCacheCalls: UsageCallRow[];
  sessions: number;
  messages: number;
  generatedAt: string;
}

/** 单会话上下文窗口占用（与 Rust ContextUsageView 对齐，camelCase）。 */
export interface ContextUsageView {
  usedTokens: number;
  maxTokens: number;
  percent: number;
  model: string;
}

// ─── 定时任务 ────────────────────────────────────────────────────────────────

export type ScheduleInput =
  | {
      kind: "preset";
      preset: "interval" | "daily" | "weekly";
      time: string; // "HH:MM"
      weekdays: number[]; // 1=Mon..7=Sun
      every?: { value: number; unit: "minutes" | "hours" };
    }
  | { kind: "cron"; expr: string };

export interface ScheduledTask {
  id: string;
  name: string;
  prompt: string;
  scheduleSpec: string;
  scheduleDisplay?: string | null;
  workingDir?: string | null;
  projectId?: string | null;
  agentId?: string | null;
  roleKind?: "expert" | "team" | null;
  roleId?: string | null;
  /** 会话权限模式（manual/auto/full）；null=继承全局默认。 */
  permissionMode?: PermissionMode | null;
  /** 运行使用的模型 id；null=全局默认。 */
  modelId?: string | null;
  enabled: boolean;
  nextRunAt?: number | null;
  lastRunAt?: number | null;
  createdAt: number;
  updatedAt: number;
  executionCount: number;
  lastStatus?: string | null;
}

export interface TaskExecution {
  id: string;
  taskId: string;
  taskName: string;
  sessionId: string;
  status: string; // running|completed|needs_attention|failed|skipped
  trigger: string; // schedule|catchup|manual
  startedAt: number;
  finishedAt?: number | null;
  error?: string | null;
  /** 该次运行所属 session 的标题；session 被删则为 null（ScheduledTaskSessions 据此显示并过滤）。 */
  sessionTitle?: string | null;
}

export interface ScheduledTaskInput {
  name: string;
  prompt: string;
  schedule: ScheduleInput;
  scheduleDisplay?: string | null;
  workingDir?: string | null;
  projectId?: string | null;
  agentId?: string | null;
  roleKind?: "expert" | "team" | null;
  roleId?: string | null;
  /** 会话权限模式（manual/auto/full）；省略时后端默认 full。 */
  permissionMode?: PermissionMode | null;
  /** 运行模型 id；省略=全局默认。 */
  modelId?: string | null;
}

export interface ScheduledTaskEvent {
  taskId: string;
  executionId: string;
  status: string;
}

/** 会话级权限模式（与 Rust PermissionMode 对齐）。 */
export type PermissionMode = "manual" | "auto" | "full";

/** Provider 调用协议（与 Rust Protocol 对齐；序列化为字符串经 invoke 传输）。 */
export type ProviderProtocol = "openai" | "anthropic";

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
  protocol: ProviderProtocol;
}

/** 厂商写入输入（与 Rust ProviderInput 对齐）。apiKey：null 保持，""清除，非空设置。 */
export interface ProviderInput {
  id: string | null;
  name: string;
  baseUrl: string;
  apiKey: string | null;
  enabled: boolean;
  protocol: ProviderProtocol;
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
  /** vision 能力覆盖（原始值，供设置页编辑/回写）；null=跟随内置查表。 */
  supportsVision: boolean | null;
  /** 已解析的 vision 能力（覆盖 ∨ 内置查表）；Composer 据此判定是否提示降级。 */
  visionCapable: boolean;
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
  /** vision 能力覆盖；null/省略表示沿用内置查表。 */
  supportsVision?: boolean | null;
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

/** 团队来源（与 Rust TeamSource 对齐）。 */
export type TeamSource = "user" | "imported" | "builtin";

/** 团队成员/lead 对 agent 的引用（与 Rust TeamMember 对齐，camelCase）。 */
export interface TeamMember {
  pluginId: string;
  teamId: string;
  name: string;
  /** "lead" | "member"。 */
  role: string;
  displayName?: string | null;
  profession?: string | null;
  avatar?: string | null;
}

/** 团队列表项（与 Rust TeamSummary 对齐）。 */
export interface Team {
  id: string;
  source: TeamSource;
  name: string;
  displayName: string;
  description: string;
  avatar?: string | null;
  category?: string | null;
  enabled: boolean;
  installedAt: string;
  memberCount: number;
  /** 来自广场目录（「加入我的」的副本带；其余 null）。 */
  catalogId?: string | null;
  /** 「我的」用户自定义分组 id（未分组为 null）。 */
  groupId?: string | null;
}

/** 团队详情（与 Rust TeamDetail 对齐）。 */
export interface TeamDetail {
  team: Team;
  lead?: ExpertSummary | null;
  members: ExpertSummary[];
  quickPrompts: string[];
  /** 该团队的私有技能（owner=team id，含未启用）。 */
  skills: Skill[];
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

/** 长期记忆条目（与 Rust Memory 对齐，camelCase）。 */
export interface Memory {
  id: string;
  content: string;
  createdAt: string;
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
  /**
   * 限定名（T108 §6）：plugin 提供的公开技能 = `plugin:name`；散装与私有技能为 null（用裸名）。
   * 展示与调用都该用它——装两个都带同名技能的 plugin 时，裸名无从区分。
   */
  qualifiedName: string | null;
  /** 是否对用户可见/可调（false 为内部知识库技能）。 */
  userInvocable: boolean;
  /** mention 菜单输入提示。 */
  argumentHint: string | null;
  /** 「我的」用户自定义分组 id（未分组为 null）。 */
  groupId?: string | null;
}

/** 插件来源（与 Rust PluginSource 对齐）。 */
export type PluginSource = "builtin" | "user";

/** 插件列表项（与 Rust PluginSummary 对齐，camelCase）。 */
export interface Plugin {
  id: string;
  source: PluginSource;
  name: string;
  displayName: string;
  version: string;
  description: string;
  descriptionZh: string | null;
  category: string | null;
  customizedFrom: string | null;
  enabled: boolean;
  installedAt: string;
  skillCount: number;
}

/** 插件详情（能力包元数据 + 其下技能 + 提供的专家）。 */
export interface PluginDetail {
  plugin: Plugin;
  skills: Skill[];
  /** 该插件提供的专家。 */
  agents: ExpertSummary[];
  /** 该插件提供的 MCP server（名称/传输/目标/连接状态）。 */
  mcpServers: PluginMcpSummary[];
  /** 该插件声明的 hooks（事件/匹配/命令）。 */
  hooks: PluginHookSummary[];
  /** 作者（从 plugin.json 解析；缺失为 null）。 */
  author: string | null;
  /** 主页 URL。 */
  homepage: string | null;
  /** 仓库 URL。 */
  repository: string | null;
  /** 许可证。 */
  license: string | null;
  /** 关键词。 */
  keywords: string[];
}

/** 插件提供的 MCP server 展示摘要（与 Rust PluginMcpSummary 对齐）。 */
export interface PluginMcpSummary {
  name: string;
  /** stdio | http */
  transport: string;
  /** stdio 的 command 或 http 的 url */
  target: string;
  /** disconnected | connecting | connected | failed | unauthorized */
  state: string;
}

/** 插件声明的 hook 展示摘要（与 Rust PluginHookSummary 对齐）。 */
export interface PluginHookSummary {
  /** PreToolUse | PostToolUse | SessionStart | Stop */
  event: string;
  /** 工具名匹配（空=匹配全部；仅 Pre/PostToolUse 有意义）。 */
  matcher: string | null;
  /** 命令（展示用，前端截断）。 */
  command: string;
}

/** 套件内专家摘要（与 Rust ExpertSummary 对齐，仅取展示所需字段）。 */
export interface ExpertSummary {
  id: string;
  /** 来源（与 Rust ExpertSource 对齐）。 */
  source: "builtin" | "user" | "plugin";
  name: string;
  description: string;
  /** 工具白名单。 */
  tools: string[];
  /** 模型档位："main" | "aux"。 */
  modelTier: string;
  role: string;
  /** owner：plugin 提供则非空。 */
  pluginId: string;
  /** owner：team 私有则非空。owner = pluginId XOR teamId。 */
  teamId: string;
  displayName?: string | null;
  profession?: string | null;
  avatar?: string | null;
  enabled: boolean;
  /** 来自广场目录（「加入我的」的副本带；其余 null）。 */
  catalogId?: string | null;
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

/** 项目成员（引用 agent；与 Rust ProjectMember 对齐）。 */
export interface ProjectMember {
  id: string;
  projectId: string;
  expertName: string;
  roleLabel?: string | null;
  responsibilities?: string | null;
  isCoordinator: boolean;
  sort: number;
  displayName?: string | null;
  avatar?: string | null;
}

/** 项目级任务看板投影：一次成员 child 运行（与 Rust ProjectChildRun 对齐）。 */
export interface ProjectChildRun {
  sessionId: string;
  threadId: string;
  threadTitle: string;
  expertName: string;
  displayName?: string | null;
  task: string;
  status: "running" | "blocked" | "done" | "failed" | "cancelled";
  artifactCount: number;
}

/** 项目级产物投影（与 Rust ProjectArtifact 对齐）。 */
export interface ProjectArtifact {
  path: string;
  title: string;
  sessionId: string;
  expertName: string;
  displayName?: string | null;
  task: string;
}

/** T61 任务台账项（与 Rust ProjectTask 对齐）。 */
export interface ProjectTask {
  id: string;
  threadSessionId: string;
  projectId?: string | null;
  /** 父任务 id：空=主任务（本轮基调）；非空=该主任务下的子任务。 */
  parentTaskId?: string | null;
  /** 主任务锚定的用户消息 id（本轮请求）。 */
  roundMessageId?: string | null;
  title: string;
  assignee?: string | null;
  status: "pending" | "in_progress" | "done" | "failed" | "cancelled";
  runSessionId?: string | null;
  sort: number;
  createdAt: string;
  updatedAt: string;
}

/** 项目运行时真实可用的专属技能，带来源归属。 */
export interface ProjectSkill {
  skill: Skill;
  sourceKind: "team" | "expert";
  sourceId: string;
  sourceName: string;
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

/** 专家详情（与 Rust ExpertDetail 对齐）：摘要 + 角色设定正文。 */
export interface ExpertDetail {
  agent: ExpertSummary;
  systemPrompt: string;
  /** 用户引导语（使用该专家的提示词列表）。 */
  quickPrompts: string[];
  /** 该专家的私有技能（owner=agent name，含未启用）。 */
  skills: Skill[];
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

/** 专家（child 子运行）摘要（与 Rust ChildAgentSummary 对齐）。 */
export interface ChildAgentSummary {
  sessionId: string;
  expertName: string;
  task: string;
  /** running | paused | done | failed */
  status: string;
  createdAt: string;
  /** 轮次键：同一轮 fan-out 的专家共享此值（产出其 dispatch 调用的 assistant 消息 id）。 */
  roundId: string;
  /** 可选展示身份（来自专家定义；缺省回退 expertName）。 */
  displayName?: string | null;
  profession?: string | null;
  avatar?: string | null;
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

/** 作用域（项目/智能体）用量详情（与 Rust ScopedUsageView 对齐）。 */
export interface ScopedUsageView {
  totals: UsageTotals;
  bySession: UsageSessionRow[];
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

/** 知识库（资料库集合），对齐 Rust KnowledgeBase（camelCase）。 */
export interface KnowledgeBase {
  id: string;
  name: string;
  description: string | null;
  icon: string | null;
  createdAt: string;
  updatedAt: string | null;
  /** 该库内资料数（列表查询填充；create/update 返回 0）。 */
  docCount: number;
}

/** 资料库内的一篇资料，对齐 Rust Document。 */
export interface KnowledgeDocument {
  id: string;
  kbId: string;
  title: string;
  sourceType: string; // text | paste
  sourceRef: string | null;
  status: string; // pending | parsing | ready | error
  error: string | null;
  charSize: number;
  createdAt: string;
}

/** 检索命中的资料片段，对齐 Rust RetrievedChunk。 */
export interface KnowledgeHit {
  chunkId: string;
  docId: string;
  docTitle: string;
  headingPath: string;
  content: string;
  score: number;
}

/**
 * 装载入口（install_plugin_from_path）的结果。三体系分立（T108）：包的类型由**清单文件名**
 * 判定 —— `plugin.json`（标准，一切公开）/ `expert.json`（专家 + 其私有技能）/
 * `team.json`（编排 + 其私有成员与技能），分发到三条各自的装载器。
 */
export type InstalledExtension =
  | ({ kind: "plugin" } & Plugin)
  | ({ kind: "team" } & Team)
  | ({ kind: "expert" } & ExpertSummary);

/* ============================ 市场（T109）============================
 *
 * **四个各自独立的市场**：插件 / 技能 / 专家 / 团队。它们的条目字段不一样 ——
 * 技能有下载量和上游仓库，专家有私有技能数，团队有主理人和成员，插件的内容是异质的。
 * 所以这里**不共用一个通用条目类型**：之前那个通用详情里塞着七个字段，
 * 一个技能会把其中六个填成空数组。
 */

/** 市场货架。仅用于市场页的 Tab 与安装后的落地提示 —— 不是一个「通用条目」的判别式。 */
export type MarketShelf = "plugin" | "skill" | "expert" | "team";

/** 一页市场条目。`total` 是该货架的**总数**（技能货架 7 万+），不是本页条数。 */
export interface MarketPage<T> {
  items: T[];
  total: number;
}

/** 技能市场（SkillHub）的一个技能。 */
export interface SkillMarketItem {
  /** SkillHub 的 slug —— 安装与「已安装」比对都用它。 */
  slug: string;
  displayName: string;
  version: string;
  description: string;
  /** 下载量（已收成人话，如 "18.2 万"）。空串 = 不显示。 */
  downloads: string;
  installed: boolean;
}

/**
 * SkillHub 的技能分类。**只有技能货架有分类** —— 它是 SkillHub 自己的分类体系，
 * 插件/专家/团队没有这回事，所以分类行只在技能页出现。
 */
export interface SkillCategory {
  key: string;
  name: string;
}

export interface SkillMarketDetail {
  slug: string;
  displayName: string;
  version: string;
  description: string;
  author: string | null;
  /** 上游仓库地址。 */
  homepage: string | null;
  installed: boolean;
}

/** 专家市场（silicon 官方）的一个专家包。 */
export interface ExpertMarketItem {
  name: string;
  displayName: string;
  version: string;
  description: string;
  /** 自带技能数。这些技能是**私有的**（只在选中该专家时载入）。 */
  skillCount: number;
  installed: boolean;
}

export interface ExpertMarketDetail {
  name: string;
  displayName: string;
  version: string;
  description: string;
  skills: string[];
  installed: boolean;
}

/** 团队市场（silicon 官方）的一个团队包。 */
export interface TeamMarketItem {
  name: string;
  displayName: string;
  version: string;
  description: string;
  memberCount: number;
  installed: boolean;
}

export interface TeamMarketDetail {
  name: string;
  displayName: string;
  version: string;
  description: string;
  lead: string | null;
  members: string[];
  installed: boolean;
}

/** 插件市场（标准 plugin 生态）的一个插件包。 */
export interface PluginMarketItem {
  name: string;
  displayName: string;
  version: string;
  description: string;
  /** 能力概览标签（如 "3 技能"、"1 MCP"）—— 插件内容异质，故用标签而非单一计数。 */
  provides: string[];
  installed: boolean;
}

export interface PluginMarketDetail {
  name: string;
  displayName: string;
  version: string;
  description: string;
  skills: string[];
  agents: string[];
  mcpServers: string[];
  commands: string[];
  hooks: number;
  author: string | null;
  homepage: string | null;
  installed: boolean;
}

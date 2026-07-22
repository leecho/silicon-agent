import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import type {
  AgentStreamEvent,
  ExpertDetail,
  Agent,
  SoulVersion,
  ExpertSummary,
  ChildAgentSummary,
  EnabledProviderModels,
  Memory,
  ModelEntry,
  ModelInput,
  Provider,
  ProviderCheckResult,
  ProviderInput,
  SessionInfo,
  Session,
  SessionGroup,
  QueuedTask,
  Skill,
  SkillDetail,
  SkillFilePreview,
  InstalledExtension,
  Plugin,
  PluginDetail,
  Team,
  TeamDetail,
  TeamMember,
  Group,
  Project,
  ProjectMember,
  ProjectSkill,
  ProjectChildRun,
  ProjectArtifact,
  ProjectTask,
  MarketPage,
  SkillCategory,
  SkillMarketItem,
  SkillMarketDetail,
  ExpertMarketItem,
  ExpertMarketDetail,
  TeamMarketItem,
  TeamMarketDetail,
  PluginMarketItem,
  PluginMarketDetail,
  ContextUsageView,
  PermissionMode,
  ScopedUsageView,
  UsageAnalyticsView,
  UsageMessageRow,
  UsageRange,
  UsageTotals,
  CallLogFilter,
  CallLogRow,
  CallLogDetail,
  CallLogStats,
  KnowledgeBase,
  KnowledgeDocument,
  KnowledgeHit,
} from "./types";

export interface AppHealth {
  ok: boolean;
  dbReady: boolean;
  version: string;
}

export type AppPlatform = "macos" | "windows" | "linux" | "unknown";

function normalizeAppPlatform(platform: string): AppPlatform {
  return platform === "macos" || platform === "windows" || platform === "linux" ? platform : "unknown";
}

export async function getAppPlatform(): Promise<AppPlatform> {
  return normalizeAppPlatform(await invoke<string>("app_platform"));
}

export async function appHealth(): Promise<AppHealth> {
  return await invoke<AppHealth>("app_health");
}

/** 内置工具 name→中文标签映射（单一真相源 = 后端 Tool::label()）。前端叙事据此展示。 */
export async function getToolLabels(): Promise<Record<string, string>> {
  return await invoke<Record<string, string>>("get_tool_labels");
}

export interface TrayOpenPayload {
  id: string;
}

export async function refreshTrayMenu(): Promise<void> {
  await invoke<void>("refresh_tray_menu");
}

async function refreshTrayMenuAfterMutation(): Promise<void> {
  try {
    await refreshTrayMenu();
  } catch (err) {
    console.debug("refresh tray menu failed", err);
  }
}

export async function subscribeTrayNewTask(handler: () => void): Promise<() => void> {
  return await listen("tray_new_task", () => handler());
}

export async function subscribeTrayOpenProject(
  handler: (payload: TrayOpenPayload) => void,
): Promise<() => void> {
  return await listen<TrayOpenPayload>("tray_open_project", (event) => handler(event.payload));
}

export async function subscribeTrayOpenAgent(
  handler: (payload: TrayOpenPayload) => void,
): Promise<() => void> {
  return await listen<TrayOpenPayload>("tray_open_agent", (event) => handler(event.payload));
}

export async function subscribeTrayOpenSession(
  handler: (payload: TrayOpenPayload) => void,
): Promise<() => void> {
  return await listen<TrayOpenPayload>("tray_open_session", (event) => handler(event.payload));
}

/** 列出全部厂商（去敏）。 */
export async function listProviders(): Promise<Provider[]> {
  return await invoke<Provider[]>("list_providers");
}

/** 新增/更新厂商，返回去敏投影。 */
export async function upsertProvider(input: ProviderInput): Promise<Provider> {
  return await invoke<Provider>("upsert_provider", { input });
}

/** 删除厂商（级联模型 + 密钥）。 */
export async function deleteProvider(id: string): Promise<void> {
  await invoke<void>("delete_provider", { id });
}

/** 启用/停用厂商。 */
export async function setProviderEnabled(
  id: string,
  enabled: boolean,
): Promise<void> {
  await invoke<void>("set_provider_enabled", { id, enabled });
}

/** 连通测试（GET /models），返回检查结果。 */
export async function testProvider(id: string): Promise<ProviderCheckResult> {
  return await invoke<ProviderCheckResult>("test_provider", { id });
}

/** 自动拉取厂商可用模型名列表。 */
export async function fetchProviderModels(id: string): Promise<string[]> {
  return await invoke<string[]>("fetch_provider_models", { id });
}

/** 列出某厂商下全部模型。 */
export async function listProviderModels(
  providerId: string,
): Promise<ModelEntry[]> {
  return await invoke<ModelEntry[]>("list_provider_models", { providerId });
}

/** 新增/更新模型。 */
export async function upsertProviderModel(
  input: ModelInput,
): Promise<ModelEntry> {
  return await invoke<ModelEntry>("upsert_provider_model", { input });
}

/** 删除模型。 */
export async function deleteProviderModel(id: string): Promise<void> {
  await invoke<void>("delete_provider_model", { id });
}

/** 启用/停用模型。 */
export async function setModelEnabled(
  id: string,
  enabled: boolean,
): Promise<void> {
  await invoke<void>("set_model_enabled", { id, enabled });
}

/** 设全局默认模型。 */
export async function setDefaultModel(id: string): Promise<void> {
  await invoke<void>("set_default_model", { id });
}

/** 设/清全局 fallback 模型（null 清除）。 */
export async function setFallbackModel(modelId: string | null): Promise<void> {
  await invoke<void>("set_fallback_model", { modelId });
}

/** 读取全局 fallback 模型 id。 */
export async function getFallbackModel(): Promise<string | null> {
  return await invoke<string | null>("get_fallback_model");
}

/** Composer：启用厂商下的启用模型，按厂商分组。 */
export async function listEnabledModels(): Promise<EnabledProviderModels[]> {
  return await invoke<EnabledProviderModels[]>("list_enabled_models");
}

/** 设置会话选中的模型（null 用全局默认）。 */
export async function setSessionModel(
  sessionId: string,
  modelId: string | null,
): Promise<void> {
  await invoke<void>("set_session_model", { sessionId, modelId });
}

/** 设/清会话运行角色（kind 空串 = 自由模式；否则 kind∈{"expert","team"} + id）。 */
export async function setSessionRole(
  sessionId: string,
  kind: string,
  id: string,
): Promise<void> {
  await invoke<void>("set_session_role", { sessionId, kind, id });
}

/** 设/清会话所属持久智能体。 */
export async function setSessionAgent(
  sessionId: string,
  agentId: string | null,
): Promise<void> {
  await invoke<void>("set_session_agent", { sessionId, agentId });
}

/** 列出启用团队（供 Composer 角色槽）。 */
export async function listActiveTeams(): Promise<Team[]> {
  return await invoke<Team[]>("list_active_teams");
}

/** 列出全部团队（团队页）。 */
export async function listTeams(): Promise<Team[]> {
  return await invoke<Team[]>("list_teams");
}

/** 团队详情（元数据 + 解析后的 lead/成员 + 开场引导语）。 */
export async function getTeamDetail(id: string): Promise<TeamDetail> {
  return await invoke<TeamDetail>("team_detail", { id });
}

/** 新建用户团队（lead 可空；members 为对 agent 的引用）。 */
export async function createTeam(
  name: string,
  displayName: string,
  description: string | null,
  lead: TeamMember | null,
  members: TeamMember[],
): Promise<Team> {
  return await invoke<Team>("create_team", {
    name,
    displayName,
    description,
    lead,
    members,
  });
}

/** 切换团队启用状态。 */
export async function toggleTeam(id: string, enabled: boolean): Promise<Team> {
  return await invoke<Team>("toggle_team", { id, enabled });
}

/** 删除用户团队（级联其私有组件）。 */
export async function deleteTeam(id: string): Promise<void> {
  await invoke<void>("delete_team", { id });
}

/** 列出可作团队成员/角色的启用 agent（散装 + plugin 提供）。 */
export async function listExperts(): Promise<ExpertSummary[]> {
  return await invoke<ExpertSummary[]>("list_experts");
}

/** 从本地目录导入团队结构的包（原生/codebuddy/Claude 方言）→ 落成 imported 团队。 */
export async function importTeamFromPath(path: string): Promise<Team> {
  return await invoke<Team>("import_team_from_path", { path });
}

/** 从本地目录导入「带技能的专家」expert 包（原生/codebuddy/Claude 方言）→ 散装 agent + 其 skill 作该 agent 私有。 */
export async function importExpertFromPath(path: string): Promise<ExpertSummary> {
  return await invoke<ExpertSummary>("import_expert_from_path", { path });
}

/** 列出散装 agent（含未启用），供「专家」管理页。 */
export async function listStandaloneExperts(): Promise<ExpertSummary[]> {
  return await invoke<ExpertSummary[]>("list_standalone_experts");
}

/** 列出「扩展 → 专家」Tab 的全部 agent：自建 + 插件提供（含未启用，排除团队私有）。 */
export async function listManageableExperts(): Promise<ExpertSummary[]> {
  return await invoke<ExpertSummary[]>("list_manageable_experts");
}

/** 新建散装 agent。 */
export async function createExpert(input: {
  name: string;
  description: string;
  systemPrompt: string;
  tools: string[];
  modelTier: string;
  displayName?: string | null;
  profession?: string | null;
  avatar?: string | null;
  quickPrompts?: string[];
}): Promise<ExpertSummary> {
  return await invoke<ExpertSummary>("create_expert", input);
}

/** 切换 agent 启用状态。 */
export async function toggleExpert(id: string, enabled: boolean): Promise<ExpertSummary> {
  return await invoke<ExpertSummary>("toggle_expert", { id, enabled });
}

/** 删除散装 user agent（内置仅可禁用）。 */
export async function deleteExpert(id: string): Promise<void> {
  await invoke<void>("delete_expert", { id });
}

/** 专家详情（摘要 + 角色设定正文）。 */
export async function getExpertDetail(id: string): Promise<ExpertDetail> {
  return await invoke<ExpertDetail>("expert_detail", { id });
}

// ── 伴随体（agent 实例）CRUD（T69）──
/** 列出全部伴随体。 */
export async function listAgents(): Promise<Agent[]> {
  return await invoke<Agent[]>("list_agents");
}
/** 由源 expert 播种创建一个伴随体（软复制其指令、引用其技能）。name 由后端从显示名派生，前端只传显示名。 */
export async function createAgent(sourceExpert: string, displayName: string): Promise<Agent> {
  const agent = await invoke<Agent>("create_agent", { sourceExpert, displayName });
  await refreshTrayMenuAfterMutation();
  return agent;
}
/** 伴随体详情。 */
export async function agentDetail(id: string): Promise<Agent> {
  return await invoke<Agent>("agent_detail", { id });
}
/** 智能体直接激活的顶层会话。 */
export async function listAgentSessions(agentId: string): Promise<SessionInfo[]> {
  return await invoke<SessionInfo[]>("list_agent_sessions", { agentId });
}
/** 智能体维度任务台账。 */
export async function listAgentTasks(agentId: string): Promise<ProjectTask[]> {
  return await invoke<ProjectTask[]>("list_agent_tasks", { agentId });
}
/** 智能体维度产物列表。 */
export async function listAgentArtifacts(agentId: string): Promise<ProjectArtifact[]> {
  return await invoke<ProjectArtifact[]>("list_agent_artifacts", { agentId });
}
/** 智能体运行时可用的专属技能。 */
export async function listAgentSkills(agentId: string): Promise<ProjectSkill[]> {
  return await invoke<ProjectSkill[]>("list_agent_skills", { agentId });
}
/** 打开智能体专属工作目录。 */
export async function openAgentWorkspace(agentId: string): Promise<void> {
  await invoke<void>("open_agent_workspace", { agentId });
}
/** 保存（编辑）伴随体：回传整条记录。 */
export async function updateAgent(record: Agent): Promise<Agent> {
  const agent = await invoke<Agent>("update_agent", { record });
  await refreshTrayMenuAfterMutation();
  return agent;
}
/** 切换启用状态。 */
export async function toggleAgent(id: string, enabled: boolean): Promise<void> {
  await invoke<void>("toggle_agent", { id, enabled });
  await refreshTrayMenuAfterMutation();
}
/** 删除伴随体（级联其私有记忆）。 */
export async function deleteAgent(id: string): Promise<void> {
  await invoke<void>("delete_agent", { id });
  await refreshTrayMenuAfterMutation();
}

// ── 自我演化（T73）──
/** 设置「允许自我演化」开关。 */
export async function setEvolutionEnabled(id: string, enabled: boolean): Promise<void> {
  await invoke<void>("set_evolution_enabled", { id, enabled });
}
/** 列出某伴随体的 SOUL 版本史（新在前）。 */
export async function listSoulVersions(id: string): Promise<SoulVersion[]> {
  return await invoke<SoulVersion[]>("list_soul_versions", { id });
}
/** 批准一个待批准的 SOUL 提案：设为活跃并同步人格。 */
export async function approveSoulProposal(id: string, versionId: string): Promise<void> {
  await invoke<void>("approve_soul_proposal", { id, versionId });
}
/** 拒绝一个待批准的 SOUL 提案。 */
export async function rejectSoulProposal(versionId: string): Promise<void> {
  await invoke<void>("reject_soul_proposal", { versionId });
}
/** 回滚到某历史 SOUL 版本。 */
export async function rollbackSoulVersion(id: string, versionId: string): Promise<void> {
  await invoke<void>("rollback_soul_version", { id, versionId });
}

/* ===================== 市场（T109）：四个市场，各一组命令 =====================
 *
 * 每个市场的条目类型不同，故不共用「通用市场 API」。
 * 安装也各走各的安装器 —— 从哪个货架点的安装，类型本来就是已知的，无需再探。
 */

/**
 * 浏览技能市场（SkillHub）。
 *
 * **分页与搜索都在服务端**：SkillHub 有 7 万+ 技能，不可能拉全再前端过滤。
 */
export async function browseSkillMarket(
  page: number,
  pageSize: number,
  keyword?: string,
  category?: string,
): Promise<MarketPage<SkillMarketItem>> {
  return await invoke<MarketPage<SkillMarketItem>>("browse_skill_market", {
    page,
    pageSize,
    keyword: keyword?.trim() || null,
    category: category || null,
  });
}

/** SkillHub 的技能分类（只有技能货架有分类）。 */
export async function listSkillCategories(): Promise<SkillCategory[]> {
  return await invoke<SkillCategory[]>("list_skill_categories");
}

export async function skillMarketDetail(slug: string): Promise<SkillMarketDetail> {
  return await invoke<SkillMarketDetail>("skill_market_detail", { slug });
}

/** 技能正文（SKILL.md 原文，markdown）。安装前预览用。 */
export async function skillMarketPreview(slug: string): Promise<string> {
  return await invoke<string>("skill_market_preview", { slug });
}

/** 装一个技能：下载 zip → 解压 → 落进「技能」页。 */
export async function installSkillFromMarket(slug: string): Promise<Skill> {
  return await invoke<Skill>("install_skill_from_market", { slug });
}

/** 浏览专家市场（silicon 官方）。 */
export async function browseExpertMarket(
  page: number,
  pageSize: number,
  keyword?: string,
): Promise<MarketPage<ExpertMarketItem>> {
  return await invoke<MarketPage<ExpertMarketItem>>("browse_expert_market", {
    page,
    pageSize,
    keyword: keyword?.trim() || null,
  });
}

export async function expertMarketDetail(name: string): Promise<ExpertMarketDetail> {
  return await invoke<ExpertMarketDetail>("expert_market_detail", { name });
}

/** 装一个专家包：落进「专家」页，其自带技能为该专家**私有**。 */
export async function installExpertFromMarket(name: string): Promise<ExpertSummary> {
  return await invoke<ExpertSummary>("install_expert_from_market", { name });
}

/** 浏览团队市场（silicon 官方）。 */
export async function browseTeamMarket(
  page: number,
  pageSize: number,
  keyword?: string,
): Promise<MarketPage<TeamMarketItem>> {
  return await invoke<MarketPage<TeamMarketItem>>("browse_team_market", {
    page,
    pageSize,
    keyword: keyword?.trim() || null,
  });
}

export async function teamMarketDetail(name: string): Promise<TeamMarketDetail> {
  return await invoke<TeamMarketDetail>("team_market_detail", { name });
}

/** 装一个团队包：落进「团队」页，其成员与技能为该团队**私有**。 */
export async function installTeamFromMarket(name: string): Promise<Team> {
  return await invoke<Team>("install_team_from_market", { name });
}

/** 浏览插件市场（标准 plugin 生态）。 */
export async function browsePluginMarket(
  page: number,
  pageSize: number,
  keyword?: string,
): Promise<MarketPage<PluginMarketItem>> {
  return await invoke<MarketPage<PluginMarketItem>>("browse_plugin_market", {
    page,
    pageSize,
    keyword: keyword?.trim() || null,
  });
}

export async function pluginMarketDetail(name: string): Promise<PluginMarketDetail> {
  return await invoke<PluginMarketDetail>("plugin_market_detail", { name });
}

/** 装一个插件：落进「插件」页，其技能与专家全局公开。 */
export async function installPluginFromMarket(name: string): Promise<Plugin> {
  return await invoke<Plugin>("install_plugin_from_market", { name });
}

/** 列出某类型（agent|team）的「我的」分组。 */
export async function listGroups(kind: "agent" | "team" | "skill"): Promise<Group[]> {
  return await invoke<Group[]>("list_groups", { kind });
}

/** 新建分组。 */
export async function createGroup(kind: "agent" | "team" | "skill", name: string): Promise<Group> {
  return await invoke<Group>("create_group", { kind, name });
}

/** 重命名分组。 */
export async function renameGroup(id: string, name: string): Promise<void> {
  await invoke<void>("rename_group", { id, name });
}

/** 删除分组（组内项归零）。 */
export async function deleteGroup(id: string, kind: "agent" | "team" | "skill"): Promise<void> {
  await invoke<void>("delete_group", { id, kind });
}

/** 把专家移入分组（null=移出）。 */
export async function setExpertGroup(expertId: string, groupId: string | null): Promise<void> {
  await invoke<void>("set_expert_group", { expertId, groupId });
}

/** 把团队移入分组（null=移出）。 */
export async function setTeamGroup(teamId: string, groupId: string | null): Promise<void> {
  await invoke<void>("set_team_group", { teamId, groupId });
}

/** 把技能移入分组（null=移出）。 */
export async function setSkillGroup(skillId: string, groupId: string | null): Promise<void> {
  await invoke<void>("set_skill_group", { skillId, groupId });
}

/** 取消单个子代理（运行中/等待中）：停其运行并把已取消结果回填父。 */
export async function cancelChild(childId: string): Promise<void> {
  await invoke<void>("cancel_child", { childId });
}

/** 取默认会话详情（不存在时由后端创建）。 */
export async function getDefaultSession(): Promise<Session> {
  return await invoke<Session>("get_default_session");
}

/** 新建会话，isDraft=true 建草稿会话。 */
export async function createSession(isDraft = false): Promise<SessionInfo> {
  const session = await invoke<SessionInfo>("create_session", { isDraft });
  if (!isDraft) await refreshTrayMenuAfterMutation();
  return session;
}

/** 保存草稿内容（防抖调用）。 */
export async function setDraftContent(
  sessionId: string,
  content: string,
): Promise<void> {
  await invoke<void>("set_draft_content", { sessionId, content });
}

/** 清理空草稿（启动时调用一次），返回删除条数。 */
export async function cleanupEmptyDrafts(): Promise<number> {
  return await invoke<number>("cleanup_empty_drafts");
}

/** 列出全部会话（按后端排序，通常 updated_at 倒序）。 */
export async function listSessions(): Promise<SessionInfo[]> {
  return await invoke<SessionInfo[]>("list_sessions");
}

/** 删除指定会话。 */
export async function deleteSession(sessionId: string): Promise<void> {
  await invoke<void>("delete_session", { sessionId });
  await refreshTrayMenuAfterMutation();
}

/** 重命名指定会话，返回更新后的会话头。 */
export async function renameSession(
  sessionId: string,
  title: string,
): Promise<SessionInfo> {
  const session = await invoke<SessionInfo>("rename_session", { sessionId, title });
  await refreshTrayMenuAfterMutation();
  return session;
}

/** 置顶 / 取消置顶指定会话。 */
export async function setSessionPinned(
  sessionId: string,
  pinned: boolean,
): Promise<void> {
  await invoke<void>("set_session_pinned", { sessionId, pinned });
}

/** 把会话移入分组（groupId）或移出分组（null）。 */
export async function setSessionGroup(
  sessionId: string,
  groupId: string | null,
): Promise<void> {
  await invoke<void>("set_session_group", { sessionId, groupId });
}

/** 新建会话分组，color 为十六进制色（#RRGGBB）；缺省后端回退默认色。返回创建的分组。 */
export async function createSessionGroup(
  label: string,
  color?: string,
): Promise<SessionGroup> {
  return await invoke<SessionGroup>("create_session_group", { label, color });
}

/** 编辑分组名称与颜色（内建分组不可编辑）。color 为十六进制色（#RRGGBB）。 */
export async function updateSessionGroup(
  id: string,
  label: string,
  color?: string,
): Promise<SessionGroup> {
  return await invoke<SessionGroup>("update_session_group", { id, label, color });
}

/** 列出全部会话分组（按 created_at）。 */
export async function listSessionGroups(): Promise<SessionGroup[]> {
  return await invoke<SessionGroup[]>("list_session_groups");
}

/** 删除指定分组；其专家会归入「最近」。 */
export async function deleteSessionGroup(groupId: string): Promise<void> {
  await invoke<void>("delete_session_group", { groupId });
}

/** 取指定会话详情（会话头 + 消息列表）。 */
export async function getSession(sessionId: string): Promise<Session | null> {
  return await invoke<Session | null>("get_session", { sessionId });
}

/** 按 (父会话, dispatch toolCallId) 找专家（child）会话 id；供「打开专家」在无 live 事件时定位。 */
export async function findChildSession(
  sessionId: string,
  toolCallId: string,
): Promise<string | null> {
  return await invoke<string | null>("find_child_session", {
    sessionId,
    toolCallId,
  });
}

/** 列某会话的专家（child 子运行）+ 状态，供右侧面板展示。 */
export async function listSessionChildren(
  sessionId: string,
): Promise<ChildAgentSummary[]> {
  return await invoke<ChildAgentSummary[]>("list_session_children", {
    sessionId,
  });
}

/** T70：列会话任务队列（含在飞队头 + 排队项）。 */
export async function listSessionQueue(sessionId: string): Promise<QueuedTask[]> {
  return await invoke<QueuedTask[]>("list_session_queue", { sessionId });
}

/** T70：取消一个排队中的任务项，返回取消后的队列。 */
export async function cancelQueuedTask(
  sessionId: string,
  itemId: string,
): Promise<QueuedTask[]> {
  return await invoke<QueuedTask[]>("cancel_queued_task", { sessionId, itemId });
}

/** 提交用户输入，触发引擎单轮处理，返回更新后的会话详情。 */
/** 发送消息结果：当前会话详情 + 本条是「入队」(queued) 还是「即时起跑」。
 * T70：前端据 `queued` 对账乐观气泡——入队则撤掉气泡（消息在排队条里，不在 feed）。 */
export interface SubmitOutcome {
  session: Session;
  queued: boolean;
}

export async function submitUserMessage(
  sessionId: string,
  content: string,
): Promise<SubmitOutcome> {
  const outcome = await invoke<SubmitOutcome>("submit_user_message", {
    sessionId,
    content,
  });
  await refreshTrayMenuAfterMutation();
  return outcome;
}

/** 增强消息：把输入框草稿润色 + 补全为结构清晰、指令明确的提示词，返回改写后的正文。
 * 草稿会话可传空 sessionId（模型走辅助模型 / 全局默认）。 */
export async function enhanceMessage(
  text: string,
  sessionId: string,
): Promise<string> {
  return await invoke<string>("enhance_message", { sessionId, text });
}

/** 提交风险工具的权限决定（允许/拒绝），引擎据此续跑，返回更新后的会话详情。 */
export async function submitPermissionDecision(
  sessionId: string,
  toolCallId: string,
  approved: boolean,
): Promise<Session> {
  const session = await invoke<Session>("submit_permission_decision", {
    sessionId,
    toolCallId,
    approved,
  });
  await refreshTrayMenuAfterMutation();
  return session;
}

/** 设置指定会话的权限模式；null 表示跟随全局配置。返回更新后的会话详情。 */
export async function setSessionPermissionMode(
  sessionId: string,
  mode: "manual" | "auto" | "full" | null,
): Promise<Session> {
  return await invoke<Session>("set_session_permission_mode", { sessionId, mode });
}

/** 读取全局权限模式（"manual" | "auto" | "full"）。 */
export async function getGlobalPermissionMode(): Promise<"manual" | "auto" | "full"> {
  return await invoke<"manual" | "auto" | "full">("get_global_permission_mode");
}

/** 写入全局权限模式。 */
export async function setGlobalPermissionMode(
  mode: "manual" | "auto" | "full",
): Promise<void> {
  await invoke<void>("set_global_permission_mode", { mode });
}

/** 提交 ask_user 的用户回答，引擎据此续跑，返回更新后的会话详情。 */
export async function submitAskResponse(
  sessionId: string,
  toolCallId: string,
  answers: string[][],
): Promise<Session> {
  return await invoke<Session>("submit_ask_response", {
    sessionId,
    toolCallId,
    answers,
  });
}

/** 取消一条待回答的 ask 并停止本轮：落「已取消」结果、不续跑，返回更新后的会话详情。 */
export async function cancelAskResponse(
  sessionId: string,
  toolCallId: string,
): Promise<Session> {
  return await invoke<Session>("cancel_ask_response", {
    sessionId,
    toolCallId,
  });
}

/** 请求停止当前会话的引擎运行；引擎在检查点停止并保留已产出内容。 */
export async function stopSession(sessionId: string): Promise<void> {
  await invoke<void>("stop_session", { sessionId });
}

/** 列出全部长期记忆。 */
export async function listMemories(): Promise<Memory[]> {
  return await invoke<Memory[]>("list_memories");
}

/** 新增一条长期记忆。 */
export async function addMemory(content: string): Promise<Memory> {
  return await invoke<Memory>("add_memory", { content });
}

/** 更新指定长期记忆的内容。 */
export async function updateMemory(id: string, content: string): Promise<void> {
  await invoke<void>("update_memory", { id, content });
}

/** 删除指定长期记忆。 */
export async function deleteMemory(id: string): Promise<void> {
  await invoke<void>("delete_memory", { id });
}

/** 清空全部长期记忆。 */
export async function clearMemories(): Promise<void> {
  await invoke<void>("clear_memories");
}

/** 读取用户画像整段文本（无则空串）。 */
export async function getMemoryProfile(): Promise<string> {
  return await invoke<string>("get_memory_profile");
}

/** 写入/覆盖用户画像（空内容等同清空）。 */
export async function setMemoryProfile(content: string): Promise<void> {
  await invoke<void>("set_memory_profile", { content });
}

/** 置顶/取消置顶一条记忆（置顶进 Tier1，始终注入）。 */
export async function setMemoryPinned(id: string, pinned: boolean): Promise<void> {
  await invoke<void>("set_memory_pinned", { id, pinned });
}

/** 主动整理结果概要。 */
export interface CurationOutcome {
  ran: boolean;
  factsBefore: number;
  factsAfter: number;
  profileUpdated: boolean;
}

/** 主动整理：模型驱动地对事实去重/合并、并抽取/更新用户画像。 */
export async function curateMemories(): Promise<CurationOutcome> {
  return await invoke<CurationOutcome>("curate_memories");
}

/** 记忆作用域：项目层 / 智能体私有层（全局走前面的 listMemories 等）。 */
export type MemoryScopeKind = "project" | "agent";

/** 列出某作用域内的 fact（精确作用域，不并入全局）。 */
export async function listScopedMemories(
  scopeKind: MemoryScopeKind,
  scopeId: string,
): Promise<Memory[]> {
  return await invoke<Memory[]>("list_scoped_memories", { scopeKind, scopeId });
}

/** 统计某作用域内的 fact 条数（首页记忆卡片）。 */
export async function countScopedMemories(
  scopeKind: MemoryScopeKind,
  scopeId: string,
): Promise<number> {
  return await invoke<number>("count_scoped_memories", { scopeKind, scopeId });
}

/** 在某作用域内新增一条 fact。 */
export async function addScopedMemory(
  scopeKind: MemoryScopeKind,
  scopeId: string,
  content: string,
): Promise<Memory> {
  return await invoke<Memory>("add_scoped_memory", { scopeKind, scopeId, content });
}

/** 压缩较早的对话历史以精简模型上下文，返回更新后的会话详情。 */
export async function compactSession(sessionId: string): Promise<Session> {
  return await invoke<Session>("compact_session", { sessionId });
}

/** 设置会话模式（"normal" | "plan"）。 */
export async function setSessionMode(
  sessionId: string,
  mode: string,
): Promise<void> {
  await invoke<void>("set_session_mode", { sessionId, mode });
}

/** 设置会话工作目录（沙箱根）。已发送/运行中会被后端拒绝。返回最新会话详情。 */
export async function setSessionWorkspace(
  sessionId: string,
  path: string,
): Promise<Session> {
  return await invoke<Session>("set_session_workspace", { sessionId, path });
}

/** 列出最近使用过的工作目录（全局，最多 8 个）。 */
export async function getRecentWorkspaces(): Promise<string[]> {
  return await invoke<string[]>("get_recent_workspaces");
}

/** 列出会话工作区文件相对路径，供 Composer @ 自动补全。 */
export async function listSessionWorkspaceFiles(
  sessionId: string,
): Promise<string[]> {
  return await invoke<string[]>("list_session_workspace_files", { sessionId });
}

/** 产物预览内容（与 Rust ArtifactContent 对齐）。 */
export interface ArtifactContent {
  kind: "markdown" | "text" | "pdf" | "html" | "office" | "binary";
  content: string;
}

/** 读取某产物文件内容用于预览（沙箱限定在该 session 工作目录）。 */
export async function readArtifact(
  sessionId: string,
  path: string,
): Promise<ArtifactContent> {
  return await invoke<ArtifactContent>("read_artifact", { sessionId, path });
}

/** 列出项目工作目录内的文件相对路径（工作目录 Tab 文件树用）。 */
export async function listProjectWorkspaceFiles(projectId: string): Promise<string[]> {
  return await invoke<string[]>("list_project_workspace_files", { projectId });
}

/** 读取项目工作目录内某文件用于预览。 */
export async function readProjectWorkspaceFile(projectId: string, path: string): Promise<ArtifactContent> {
  return await invoke<ArtifactContent>("read_project_workspace_file", { projectId, path });
}

/** 列出智能体工作目录内的文件相对路径（工作目录 Tab 文件树用）。 */
export async function listAgentWorkspaceFiles(agentId: string): Promise<string[]> {
  return await invoke<string[]>("list_agent_workspace_files", { agentId });
}

/** 读取智能体工作目录内某文件用于预览。 */
export async function readAgentWorkspaceFile(agentId: string, path: string): Promise<ArtifactContent> {
  return await invoke<ArtifactContent>("read_agent_workspace_file", { agentId, path });
}

/** 用系统默认应用打开项目工作目录内某文件。 */
export async function openProjectWorkspaceFile(projectId: string, path: string): Promise<void> {
  await invoke<void>("open_project_workspace_file", { projectId, path });
}

/** 用系统默认应用打开智能体工作目录内某文件。 */
export async function openAgentWorkspaceFile(agentId: string, path: string): Promise<void> {
  await invoke<void>("open_agent_workspace_file", { agentId, path });
}

/** 弹系统目录选择器，返回所选绝对路径；取消返回 null。 */
export async function pickDirectory(): Promise<string | null> {
  const picked = await openDialog({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

/** 用系统文件管理器打开目录。 */
export async function openWorkspaceDir(path: string): Promise<void> {
  await openPath(path);
}

/** 打开指定会话的受控工作目录。 */
export async function openSessionWorkspace(sessionId: string): Promise<void> {
  await invoke<void>("open_session_workspace", { sessionId });
}

/** 用系统默认应用打开文件。 */
export async function openArtifactFile(
  sessionId: string,
  path: string,
): Promise<void> {
  await invoke<void>("open_artifact_file", { sessionId, path });
}

/** 在系统文件管理器中定位文件。 */
export async function revealArtifactFile(
  sessionId: string,
  path: string,
): Promise<void> {
  await invoke<void>("reveal_artifact_file", { sessionId, path });
}

/** 提交计划决定（批准/评论修改），引擎据此续跑，返回更新后的会话详情。 */
export async function submitPlanDecision(
  sessionId: string,
  toolCallId: string,
  approved: boolean,
  comment?: string,
): Promise<Session> {
  const session = await invoke<Session>("submit_plan_decision", {
    sessionId,
    toolCallId,
    approved,
    comment,
  });
  await refreshTrayMenuAfterMutation();
  return session;
}

/** 列出全部技能（内置 + 用户，按 name 升序）。 */
export async function listSkills(): Promise<Skill[]> {
  return await invoke<Skill[]>("list_skills");
}

/** 列出项目运行时真实可用的专属技能。 */
export async function listProjectSkills(projectId: string): Promise<ProjectSkill[]> {
  return await invoke<ProjectSkill[]>("list_project_skills", { projectId });
}

/** 切换技能启用状态，返回更新后的技能。 */
export async function toggleSkill(
  id: string,
  enabled: boolean,
): Promise<Skill> {
  return await invoke<Skill>("toggle_skill", { id, enabled });
}

/** 从本地路径安装技能（.zip 文件或技能目录），返回安装的技能。 */
export async function installSkillFromPath(path: string): Promise<Skill> {
  return await invoke<Skill>("install_skill_from_path", { path });
}

/** 卸载用户技能（内置不可卸载）。 */
export async function uninstallSkill(id: string): Promise<void> {
  await invoke<void>("uninstall_skill", { id });
}

/** 读取技能详情（元数据 + SKILL.md 原文 + 文件列表）。 */
export async function getSkillDetail(id: string): Promise<SkillDetail> {
  return await invoke<SkillDetail>("get_skill_detail", { id });
}

/** 读取技能目录内单文件用于预览。 */
export async function readSkillFile(
  id: string,
  relPath: string,
): Promise<SkillFilePreview> {
  return await invoke<SkillFilePreview>("read_skill_file", { id, relPath });
}

/** 列出全部插件（按 name 升序，含各自 skill 数）。 */
export async function listPlugins(): Promise<Plugin[]> {
  return await invoke<Plugin[]>("list_plugins");
}

/**
 * 统一装载入口（T106）：装载一个扩展包（目录或 zip）。
 * 后端探包分发——团队结构 → 落成团队（`kind:"team"`，去团队页）；否则 → 装成能力包（`kind:"plugin"`）。
 */
export async function installPluginFromPath(path: string): Promise<InstalledExtension> {
  return await invoke<InstalledExtension>("install_plugin_from_path", { path });
}

/** 切换插件启用状态（其下技能可见性随之级联），返回更新后的插件。 */
export async function togglePlugin(
  id: string,
  enabled: boolean,
): Promise<Plugin> {
  return await invoke<Plugin>("toggle_plugin", { id, enabled });
}

/** 卸载用户插件（内置不可卸载，级联删其技能）。 */
export async function uninstallPlugin(id: string): Promise<void> {
  await invoke<void>("uninstall_plugin", { id });
}

/** 读取插件详情（元数据 + 其下技能列表）。 */
export async function getPluginDetail(id: string): Promise<PluginDetail> {
  return await invoke<PluginDetail>("plugin_detail", { id });
}

/** 弹文件选择器选 .zip 技能包，返回绝对路径；取消返回 null。 */
export async function pickSkillZip(): Promise<string | null> {
  const picked = await openDialog({
    multiple: false,
    directory: false,
    filters: [{ name: "技能压缩包", extensions: ["zip"] }],
  });
  return typeof picked === "string" ? picked : null;
}

/** 弹文件选择器选 .zip 套件包，返回绝对路径；取消返回 null。 */
export async function pickPluginZip(): Promise<string | null> {
  const picked = await openDialog({
    multiple: false,
    directory: false,
    filters: [{ name: "套件压缩包", extensions: ["zip"] }],
  });
  return typeof picked === "string" ? picked : null;
}

/** 弹文件选择器选 .zip 团队包，返回绝对路径；取消返回 null。 */
export async function pickTeamZip(): Promise<string | null> {
  const picked = await openDialog({
    multiple: false,
    directory: false,
    filters: [{ name: "团队压缩包", extensions: ["zip"] }],
  });
  return typeof picked === "string" ? picked : null;
}

/** 弹文件选择器选单个文件，返回绝对路径；取消返回 null。 */
export async function pickFile(): Promise<string | null> {
  const picked = await openDialog({ directory: false, multiple: false });
  return typeof picked === "string" ? picked : null;
}

/** 把外部文件作为附件纳入会话工作目录，返回可被 agent 访问的相对路径。
 *  工作区内文件直接引用、不复制；工作区外文件复制到 attachments/ 下。 */
export async function attachFile(
  sessionId: string,
  srcPath: string,
): Promise<string> {
  return await invoke<string>("attach_file", { sessionId, srcPath });
}

/** 把字节（粘贴/拖拽的文件或图片）写入会话工作目录 attachments/，返回相对路径。 */
export async function saveAttachment(
  sessionId: string,
  fileName: string,
  data: number[],
): Promise<string> {
  return await invoke<string>("save_attachment", { sessionId, fileName, data });
}

/** 读取会话工作目录内某附件的原始字节（供图片预览）。 */
export async function readAttachment(
  sessionId: string,
  relPath: string,
): Promise<number[]> {
  return await invoke<number[]>("read_attachment", { sessionId, relPath });
}

/** 读取用量分析聚合。 */
export async function getUsageAnalytics(
  range: UsageRange,
): Promise<UsageAnalyticsView> {
  return await invoke<UsageAnalyticsView>("get_usage_analytics", { range });
}

/** 读取单会话上下文窗口占用（供 composer 的 context meter 展示）。 */
export async function getSessionContextUsage(
  sessionId: string,
): Promise<ContextUsageView> {
  return await invoke<ContextUsageView>("get_session_context_usage", {
    sessionId,
  });
}

/** 单会话累计 token 用量（供 composer 累计 chip）。 */
export async function getSessionUsage(sessionId: string): Promise<UsageTotals> {
  return await invoke<UsageTotals>("get_session_usage", { sessionId });
}

/** 项目维度用量详情（总计 + 按会话）。 */
export async function getProjectUsage(
  projectId: string,
  range: UsageRange,
): Promise<ScopedUsageView> {
  return await invoke<ScopedUsageView>("get_project_usage", { projectId, range });
}

/** 智能体维度用量详情（总计 + 按会话）。 */
export async function getAgentUsage(
  agentId: string,
  range: UsageRange,
): Promise<ScopedUsageView> {
  return await invoke<ScopedUsageView>("get_agent_usage", { agentId, range });
}

/** 单会话按消息用量（会话→消息二层展开）。 */
export async function getSessionMessageUsage(
  sessionId: string,
): Promise<UsageMessageRow[]> {
  return await invoke<UsageMessageRow[]>("get_session_message_usage", { sessionId });
}

// ── 模型调用日志（T76）──────────────────────────────────────────────────────
export async function getModelCallLogEnabled(): Promise<boolean> {
  return await invoke<boolean>("get_model_call_log_enabled");
}
export async function setModelCallLogEnabled(enabled: boolean): Promise<void> {
  await invoke("set_model_call_log_enabled", { enabled });
}
export async function listModelCalls(filter: CallLogFilter): Promise<CallLogRow[]> {
  return await invoke<CallLogRow[]>("list_model_calls", { filter });
}
export async function getModelCall(id: string): Promise<CallLogDetail | null> {
  return await invoke<CallLogDetail | null>("get_model_call", { id });
}
export async function clearModelCalls(filter?: CallLogFilter): Promise<number> {
  return await invoke<number>("clear_model_calls", { filter: filter ?? null });
}
export async function getModelCallLogStats(): Promise<CallLogStats> {
  return await invoke<CallLogStats>("get_model_call_log_stats");
}

/** 订阅引擎流式事件；返回取消订阅函数。 */
export async function subscribeAgentStreamEvents(
  handler: (event: AgentStreamEvent) => void,
): Promise<() => void> {
  return await listen<AgentStreamEvent>("agent_stream_event", (event) => {
    handler(event.payload);
  });
}

/** 订阅会话元信息更新（如后台生成的标题落库）；返回取消订阅函数。 */
export async function subscribeSessionUpdated(
  handler: () => void,
): Promise<() => void> {
  return await listen("session_updated", () => handler());
}

// ===== 项目协作 =====
export async function listProjects(): Promise<Project[]> {
  return await invoke<Project[]>("list_projects");
}
export async function createProject(
  name: string,
  description?: string,
  instructions?: string,
  workspaceDir?: string,
): Promise<Project> {
  const project = await invoke<Project>("create_project", { name, description, instructions, workspaceDir });
  await refreshTrayMenuAfterMutation();
  return project;
}
/** T59：设项目指令（章程/PM 指令）。 */
export async function setProjectInstructions(projectId: string, instructions: string): Promise<void> {
  await invoke<void>("set_project_instructions", { projectId, instructions });
}
/** T59：更新项目名称与描述。 */
export async function updateProject(id: string, name: string, description?: string): Promise<void> {
  await invoke<void>("update_project", { id, name, description });
  await refreshTrayMenuAfterMutation();
}
/** T59：设项目工作目录。 */
export async function setProjectWorkspace(projectId: string, workspaceDir: string): Promise<void> {
  await invoke<void>("set_project_workspace", { projectId, workspaceDir });
}
export async function getProject(id: string): Promise<Project | null> {
  return await invoke<Project | null>("get_project", { id });
}
export async function deleteProject(id: string): Promise<void> {
  await invoke<void>("delete_project", { id });
  await refreshTrayMenuAfterMutation();
}
export async function listProjectMembers(projectId: string): Promise<ProjectMember[]> {
  return await invoke<ProjectMember[]>("list_project_members", { projectId });
}
export async function addProjectMember(input: {
  projectId: string;
  expertName: string;
  roleLabel?: string | null;
  responsibilities?: string | null;
  isCoordinator?: boolean;
}): Promise<ProjectMember> {
  return await invoke<ProjectMember>("add_project_member", input);
}
export async function removeProjectMember(memberId: string): Promise<void> {
  await invoke<void>("remove_project_member", { memberId });
}
/** 方案C：从团队导入一名成员到项目（复制成「项目私有副本」，与源团队解耦）。 */
export async function importTeamMember(projectId: string, teamId: string, expertName: string): Promise<ProjectMember> {
  return await invoke<ProjectMember>("import_team_member", { projectId, teamId, expertName });
}
/** T59：列项目顶层会话。 */
export async function listProjectThreads(projectId: string): Promise<SessionInfo[]> {
  return await invoke<SessionInfo[]>("list_project_threads", { projectId });
}
/** 项目会话列表（兼容底层历史 command 名）。 */
export async function listProjectSessions(projectId: string): Promise<SessionInfo[]> {
  return await listProjectThreads(projectId);
}
/** 从项目草稿首条消息创建会话，返回 session id。 */
export async function submitProjectDraftMessage(input: {
  projectId: string;
  content: string;
  sourceDraftSessionId?: string | null;
  mode?: string | null;
  permissionMode?: PermissionMode | null;
  selectedModelId?: string | null;
}): Promise<string> {
  const sessionId = await invoke<string>("submit_project_draft_message", input);
  await refreshTrayMenuAfterMutation();
  return sessionId;
}
/** T59：设项目权限模式（manual|auto|full）。 */
export async function setProjectPermissionMode(
  projectId: string,
  mode: "manual" | "auto" | "full",
): Promise<void> {
  await invoke<void>("set_project_permission_mode", { projectId, mode });
}
/** T59：项目级任务看板投影（跨会话聚合成员 child 运行）。 */
export async function listProjectChildRuns(projectId: string): Promise<ProjectChildRun[]> {
  return await invoke<ProjectChildRun[]>("list_project_child_runs", { projectId });
}
/** T59：项目级产物投影（跨会话聚合成员 child 已登记 artifacts）。 */
export async function listProjectArtifacts(projectId: string): Promise<ProjectArtifact[]> {
  return await invoke<ProjectArtifact[]>("list_project_artifacts", { projectId });
}
/** T61：项目级任务台账（跨会话聚合）。 */
export async function listProjectTasks(projectId: string): Promise<ProjectTask[]> {
  return await invoke<ProjectTask[]>("list_project_tasks", { projectId });
}
/** T61：某编排会话的任务台账。 */
export async function listThreadTasks(threadSessionId: string): Promise<ProjectTask[]> {
  return await invoke<ProjectTask[]>("list_thread_tasks", { threadSessionId });
}
export async function openProjectWorkspace(projectId: string): Promise<void> {
  await invoke<void>("open_project_workspace", { projectId });
}

/** 订阅一轮结束后的快捷建议；返回取消订阅函数。 */
export async function subscribeSessionSuggestions(
  handler: (payload: { sessionId: string; suggestions: string[] }) => void,
): Promise<() => void> {
  return await listen<{ sessionId: string; suggestions: string[] }>(
    "session_suggestions",
    (event) => handler(event.payload),
  );
}

/** 读「每轮结束后生成快捷建议」开关。 */
export async function getSuggestionsEnabled(): Promise<boolean> {
  return await invoke<boolean>("get_suggestions_enabled");
}

/** 写「每轮结束后生成快捷建议」开关。 */
export async function setSuggestionsEnabled(enabled: boolean): Promise<void> {
  await invoke<void>("set_suggestions_enabled", { enabled });
}

/** 读自动压缩开关（缺省 = 开）。 */
export async function getAutoCompactEnabled(): Promise<boolean> {
  return await invoke<boolean>("get_auto_compact_enabled");
}

/** 设置自动压缩开关。 */
export async function setAutoCompactEnabled(enabled: boolean): Promise<void> {
  await invoke<void>("set_auto_compact_enabled", { enabled });
}

/** 读自动压缩触发阈值百分比（缺省 = 90）。 */
export async function getAutoCompactThresholdPct(): Promise<number> {
  return await invoke<number>("get_auto_compact_threshold_pct");
}

/** 设置自动压缩触发阈值百分比。 */
export async function setAutoCompactThresholdPct(n: number): Promise<void> {
  await invoke<void>("set_auto_compact_threshold_pct", { n });
}

/** 读已完成轮次的思考与执行过程展示开关（缺省 = 开）。 */
export async function getShowCompletedProcess(): Promise<boolean> {
  return await invoke<boolean>("get_show_completed_process");
}

/** 设置已完成轮次的思考与执行过程展示开关。 */
export async function setShowCompletedProcess(enabled: boolean): Promise<void> {
  await invoke<void>("set_show_completed_process", { enabled });
}

/** 读 SessionPage 是否默认显示任务面板（缺省 = 开）。 */
export async function getSessionTaskPanelDefaultVisible(): Promise<boolean> {
  return await invoke<boolean>("get_session_task_panel_default_visible");
}

/** 设置 SessionPage 是否默认显示任务面板。 */
export async function setSessionTaskPanelDefaultVisible(
  visible: boolean,
): Promise<void> {
  await invoke<void>("set_session_task_panel_default_visible", { visible });
}

/** 手动重试上一轮失败的模型调用。 */
export async function retrySession(sessionId: string): Promise<Session> {
  const session = await invoke<Session>("retry_session", { sessionId });
  await refreshTrayMenuAfterMutation();
  return session;
}

/** 读失败自动重试次数。 */
export async function getAutoRetryMax(): Promise<number> {
  return await invoke<number>("get_auto_retry_max");
}

/** 设失败自动重试次数（0..=5；0=关闭）。 */
export async function setAutoRetryMax(n: number): Promise<void> {
  await invoke<void>("set_auto_retry_max", { n });
}

/** 读单次任务最大模型迭代次数。 */
export async function getMaxIterations(): Promise<number> {
  return await invoke<number>("get_max_iterations");
}

/** 设置单次任务最大模型迭代次数。 */
export async function setMaxIterations(n: number): Promise<void> {
  await invoke<void>("set_max_iterations", { n });
}

/** 读单工具执行超时秒数（全局默认，秒；工具级覆盖优先）。 */
export async function getToolTimeoutSecs(): Promise<number> {
  return await invoke<number>("get_tool_timeout_secs");
}

/** 设单工具执行超时秒数（clamp 1..=1800）。 */
export async function setToolTimeoutSecs(n: number): Promise<void> {
  await invoke<void>("set_tool_timeout_secs", { n });
}

/** 读工具并行执行上限（连续 concurrency_safe 段最多并发数）。 */
export async function getToolParallelism(): Promise<number> {
  return await invoke<number>("get_tool_parallelism");
}

/** 设工具并行执行上限（clamp 1..=32；1=串行）。 */
export async function setToolParallelism(n: number): Promise<void> {
  await invoke<void>("set_tool_parallelism", { n });
}

export type SubagentExecutionMode = "parallel" | "serial";

/** 读子代理执行方式。 */
export async function getSubagentExecutionMode(): Promise<SubagentExecutionMode> {
  return await invoke<SubagentExecutionMode>("get_subagent_execution_mode");
}

/** 设置子代理执行方式。 */
export async function setSubagentExecutionMode(mode: SubagentExecutionMode): Promise<void> {
  await invoke<void>("set_subagent_execution_mode", { mode });
}

/** 读辅助模型 id（标题/建议生成用）；null = 跟随会话模型。 */
export async function getAuxModelId(): Promise<string | null> {
  return await invoke<string | null>("get_aux_model_id");
}

/** 写辅助模型 id；null 表示清除（回退会话模型）。 */
export async function setAuxModelId(modelId: string | null): Promise<void> {
  await invoke<void>("set_aux_model_id", { modelId });
}

// ---------- MCP ----------
export type McpTransportConfig =
  | { type: "stdio"; command: string; args?: string[]; env?: Record<string, string>; cwd?: string | null }
  | { type: "http"; url: string; headers?: Record<string, string> }
  | { type: "sse"; url: string; headers?: Record<string, string> };

/** 凭证内联在 transport（http→headers、stdio→env），不再有独立鉴权配置。 */
export interface McpServerConfig {
  id: string;
  name: string;
  presetId?: string | null;
  pluginId?: string;
  /** OAuth 手填 client_id（JSON 扩展字段 clientId）；null/省略=动态注册。 */
  oauthClientId?: string | null;
  transport: McpTransportConfig;
  autoApprove: boolean;
  enabled: boolean;
}

export interface McpServerStatus {
  serverId: string;
  state: "disconnected" | "connecting" | "connected" | "failed" | "unauthorized";
  error: string | null;
  toolCount: number;
}

export interface McpToolDef {
  name: string;
  description: string;
  inputSchema?: unknown;
}

export async function mcpListServers(): Promise<McpServerConfig[]> {
  return await invoke<McpServerConfig[]>("mcp_list_servers");
}
export async function mcpServerStatuses(): Promise<McpServerStatus[]> {
  return await invoke<McpServerStatus[]>("mcp_server_statuses");
}
export async function mcpUpsertServer(config: McpServerConfig): Promise<McpServerConfig> {
  return await invoke<McpServerConfig>("mcp_upsert_server", { config });
}
/** 导入标准 mcpServers JSON：整组覆盖手动服务，返回导入后的手动服务列表。 */
export async function mcpImportJson(json: string): Promise<McpServerConfig[]> {
  return await invoke<McpServerConfig[]>("mcp_import_json", { json });
}
/** 导出当前手动服务为标准 mcpServers JSON 文本。 */
export async function mcpExportJson(): Promise<string> {
  return await invoke<string>("mcp_export_json");
}
/** 某 server 的工具清单（详情页）。 */
export async function mcpListTools(id: string): Promise<McpToolDef[]> {
  return await invoke<McpToolDef[]>("mcp_list_tools", { id });
}
export async function mcpSetEnabled(id: string, enabled: boolean): Promise<void> {
  await invoke("mcp_set_enabled", { id, enabled });
}
export async function mcpSetAutoApprove(id: string, autoApprove: boolean): Promise<void> {
  await invoke("mcp_set_auto_approve", { id, autoApprove });
}
export async function mcpDeleteServer(id: string): Promise<void> {
  await invoke("mcp_delete_server", { id });
}
export async function mcpTestConnection(config: McpServerConfig): Promise<McpToolDef[]> {
  return await invoke<McpToolDef[]>("mcp_test_connection", { config });
}
/** 断开并重连一个已保存的 server（用户面「重试」）。结果经 mcp_status_event 回流。 */
export async function mcpReconnect(id: string): Promise<void> {
  await invoke("mcp_reconnect", { id });
}
/** 发起 OAuth 授权，返回授权 URL（前端展示/复制）。授权后台完成，结果经 mcp_status_event 回流。 */
export async function mcpOauthAuthorize(id: string): Promise<string> {
  return await invoke<string>("mcp_oauth_authorize", { id });
}
/**
 * 设置/清除某 MCP server 的 OAuth client_id（不支持动态注册 DCR 的服务需手填）。
 * 插件提供的 server 也可以——client_id 是**凭证**，不是包的构成。
 */
export async function mcpSetOauthClientId(
  id: string,
  clientId: string | null,
): Promise<void> {
  await invoke<void>("mcp_set_oauth_client_id", { id, clientId });
}

export async function mcpOauthRevoke(id: string): Promise<void> {
  await invoke("mcp_oauth_revoke", { id });
}

// ── Computer Use ──────────────────────────────────────────────────────────────

/** 读取「桌面操作」功能总开关状态。 */
export async function getComputerUseEnabled(): Promise<boolean> {
  return await invoke<boolean>("get_computer_use_enabled");
}

/** 设置「桌面操作」功能总开关。 */
export async function setComputerUseEnabled(enabled: boolean): Promise<void> {
  await invoke<void>("set_computer_use_enabled", { enabled });
}

// ── 系统权限统一模块（T89）────────────────────────────────────────────────────

export type PermissionKind =
  | "accessibility"
  | "notification"
  | "automation"
  | "calendars"
  | "reminders"
  | "full_disk";
export type PermissionState = "granted" | "denied" | "unknown" | "unsupported";

export interface PermissionRow {
  kind: PermissionKind;
  state: PermissionState;
  canQuery: boolean;
  canRequest: boolean;
  perTarget: boolean;
  needsRelaunch: boolean;
}

export async function permissionStatusAll(): Promise<PermissionRow[]> {
  // 后端 serde 默认 snake_case；Tauri JS 桥可能 camelCase 化，这里两种都兜底。
  const rows = await invoke<Array<Record<string, unknown>>>("permission_status_all");
  return rows.map((r) => ({
    kind: r.kind as PermissionKind,
    state: r.state as PermissionState,
    canQuery: Boolean(r.can_query ?? r.canQuery),
    canRequest: Boolean(r.can_request ?? r.canRequest),
    perTarget: Boolean(r.per_target ?? r.perTarget),
    needsRelaunch: Boolean(r.needs_relaunch ?? r.needsRelaunch),
  }));
}

export async function permissionStatus(kind: PermissionKind): Promise<PermissionState> {
  return await invoke<PermissionState>("permission_status", { kind });
}

export async function permissionRequest(kind: PermissionKind): Promise<PermissionState> {
  return await invoke<PermissionState>("permission_request", { kind });
}

export async function permissionOpenSettings(kind: PermissionKind): Promise<void> {
  await invoke<void>("permission_open_settings", { kind });
}

export async function appRelaunch(): Promise<void> {
  await invoke<void>("app_relaunch");
}

// ── Browser Automation ────────────────────────────────────────────────────────

/** 探测本机浏览器操作就绪状态（"ready" | "not_installed"）。 */
export async function browserStatus(): Promise<string> {
  return await invoke<string>("browser_status");
}

/** 读取「浏览器操作」功能总开关状态。 */
export async function getBrowserUseEnabled(): Promise<boolean> {
  return await invoke<boolean>("get_browser_use_enabled");
}

/** 设置「浏览器操作」功能总开关。 */
export async function setBrowserUseEnabled(enabled: boolean): Promise<void> {
  await invoke<void>("set_browser_use_enabled", { enabled });
}

/** 读取「静默模式」（无头浏览器）开关状态（不弹出可见浏览器窗口）。 */
export async function getBrowserHeadless(): Promise<boolean> {
  return await invoke<boolean>("get_browser_headless");
}

/** 设置「静默模式」（无头浏览器）开关。 */
export async function setBrowserHeadless(enabled: boolean): Promise<void> {
  await invoke<void>("set_browser_headless", { enabled });
}

/** 常驻浏览器当前是否开着（前端据此决定是否还提示登录/打开）。 */
export async function browserIsOpen(): Promise<boolean> {
  return await invoke<boolean>("browser_is_open");
}

/** 用户显式打开浏览器窗口（供先行登录常用网站）。 */
export async function browserOpen(): Promise<void> {
  await invoke<void>("browser_open");
}

/** 浏览器空闲多久（分钟）自动关闭；0=不自动关。 */
export async function getBrowserIdleCloseMin(): Promise<number> {
  return await invoke<number>("get_browser_idle_close_min");
}

/** 设置浏览器空闲自动关闭时长（分钟）；0=不自动关。 */
export async function setBrowserIdleCloseMin(min: number): Promise<void> {
  await invoke<void>("set_browser_idle_close_min", { min });
}

// ===== 知识库（资料库）=====

/** 列出全部资料库。 */
export async function kbList(): Promise<KnowledgeBase[]> {
  return await invoke<KnowledgeBase[]>("kb_list");
}

/** 新建资料库。 */
export async function kbCreate(name: string, description?: string | null): Promise<KnowledgeBase> {
  return await invoke<KnowledgeBase>("kb_create", { name, description: description ?? null, icon: null });
}

/** 重命名/改描述。 */
export async function kbUpdate(id: string, name: string, description?: string | null): Promise<void> {
  await invoke<void>("kb_update", { id, name, description: description ?? null, icon: null });
}

/** 删除资料库（连同资料/索引）。 */
export async function kbDelete(id: string): Promise<void> {
  await invoke<void>("kb_delete", { id });
}

/** 列出某资料库的资料。 */
export async function kbDocumentList(kbId: string): Promise<KnowledgeDocument[]> {
  return await invoke<KnowledgeDocument[]>("kb_document_list", { kbId });
}

/** 取一份资料的原文（查看内容）。 */
export async function kbDocumentText(docId: string): Promise<string> {
  return await invoke<string>("kb_document_text", { docId });
}

/** 富预览一份资料：有原始文件走 markdown/pdf/office 富渲染，否则回退已存文本。 */
export async function kbDocumentPreview(docId: string): Promise<ArtifactContent> {
  return await invoke<ArtifactContent>("kb_document_preview", { docId });
}

/** 添加资料：粘贴文本（body）或选本地文件（filePath，二选一）。 */
export async function kbDocumentAdd(
  kbId: string,
  title: string,
  opts: { body?: string; filePath?: string },
): Promise<KnowledgeDocument> {
  return await invoke<KnowledgeDocument>("kb_document_add", {
    kbId,
    title,
    body: opts.body ?? null,
    filePath: opts.filePath ?? null,
  });
}

/** 从网址添加资料。 */
export async function kbDocumentAddUrl(kbId: string, title: string, url: string): Promise<KnowledgeDocument> {
  return await invoke<KnowledgeDocument>("kb_document_add_url", { kbId, title, url });
}

/** 删除一篇资料。 */
export async function kbDocumentDelete(docId: string): Promise<void> {
  await invoke<void>("kb_document_delete", { docId });
}

/** 在指定资料库内查阅（UI 预览）。 */
export async function kbSearch(kbIds: string[], query: string, topK?: number): Promise<KnowledgeHit[]> {
  return await invoke<KnowledgeHit[]>("kb_search", { kbIds, query, topK: topK ?? 5 });
}

/** 把资料库挂载到会话。 */
export async function kbMount(kbId: string, sessionId: string): Promise<void> {
  await invoke<void>("kb_mount", { kbId, sessionId });
}

/** 取消挂载。 */
export async function kbUnmount(kbId: string, sessionId: string): Promise<void> {
  await invoke<void>("kb_unmount", { kbId, sessionId });
}

/** 当前会话已挂载的资料库 id。 */
export async function kbMountedIds(sessionId: string): Promise<string[]> {
  return await invoke<string[]>("kb_mounted_ids", { sessionId });
}

/** 通用挂载：scopeType ∈ "session" | "agent" | "project"。 */
export async function kbMountScope(kbId: string, scopeType: string, scopeId: string): Promise<void> {
  await invoke<void>("kb_mount_scope", { kbId, scopeType, scopeId });
}

/** 通用卸载。 */
export async function kbUnmountScope(kbId: string, scopeType: string, scopeId: string): Promise<void> {
  await invoke<void>("kb_unmount_scope", { kbId, scopeType, scopeId });
}

/** 某作用域已挂载的资料库 id。 */
export async function kbScopedMountedIds(scopeType: string, scopeId: string): Promise<string[]> {
  return await invoke<string[]>("kb_scoped_mounted_ids", { scopeType, scopeId });
}

/** 读向量检索设置：[启用, 模型id]。 */
export async function kbVectorSettings(): Promise<[boolean, string]> {
  return await invoke<[boolean, string]>("kb_vector_settings");
}

/** 写向量检索设置。 */
export async function kbSetVectorSettings(enabled: boolean, modelId: string): Promise<void> {
  await invoke<void>("kb_set_vector_settings", { enabled, modelId });
}

/** 为某资料库内缺向量的片段批量建立向量索引，返回新建数量。 */
export async function kbBuildVectorIndex(kbId: string): Promise<number> {
  return await invoke<number>("kb_build_vector_index", { kbId });
}

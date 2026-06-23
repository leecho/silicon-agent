import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import type {
  AgentStreamEvent,
  EnabledProviderModels,
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
  ContextUsageView,
  UsageAnalyticsView,
  UsageMessageRow,
  UsageRange,
  UsageTotals,
  CallLogFilter,
  CallLogRow,
  CallLogDetail,
  CallLogStats,
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
export async function submitUserMessage(
  sessionId: string,
  content: string,
): Promise<Session> {
  const session = await invoke<Session>("submit_user_message", {
    sessionId,
    content,
  });
  await refreshTrayMenuAfterMutation();
  return session;
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

/** 弹文件选择器选 .zip 技能包，返回绝对路径；取消返回 null。 */
export async function pickSkillZip(): Promise<string | null> {
  const picked = await openDialog({
    multiple: false,
    directory: false,
    filters: [{ name: "技能压缩包", extensions: ["zip"] }],
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


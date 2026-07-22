import { forwardRef, useEffect, useImperativeHandle, useRef, useState, type ForwardedRef } from "react";
import { ArrowUp, BookMarked, Bot, CircleX, ClipboardList, ImageOff, Layers, Loader2, MessageSquare, Square, Users, Wand2 } from "lucide-react";
import { AddMenu } from "../../pages/session/composer/AddMenu";
import { ModelPicker } from "../../pages/session/composer/ModelPicker";
import { PermissionPicker } from "../../pages/session/composer/PermissionPicker";
import { WorkspacePicker } from "../../pages/session/composer/WorkspacePicker";
import { ContextMeter } from "../../pages/session/composer/ContextMeter";
import { SessionUsageChip } from "../../pages/session/composer/SessionUsageChip";
import { ComposerInput, type ComposerInputHandle } from "./ComposerInput";
import { AttachmentCard, AttachmentImageModal } from "./AttachmentCard";
import { QueuedMessages } from "./QueuedMessages";
import { QuoteChip } from "./QuoteChip";
import { enhanceMessage, kbList, kbMount, kbMountedIds, kbUnmount, listPlugins, listSessionWorkspaceFiles, listSkills } from "../../api";
import {
  attachmentKind,
  basename,
  type Attachment,
} from "../../lib/attachments";
import { Tooltip } from "../ui/Tooltip";
import type {
  Agent,
  ExpertSummary,
  Team,
  ContextUsageView,
  EnabledProviderModels,
  KnowledgeBase,
  PermissionMode,
  Project,
  QueuedTask,
  RunActivity,
  Skill,
  UsageTotals,
} from "../../types";

function formatElapsed(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes === 0) return `${seconds}s`;
  return `${minutes}m ${seconds}s`;
}

function RunActivityInline({ activity }: { activity: RunActivity }) {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [activity.startedAt]);

  return (
    <div className="ui-activity-breathe relative flex items-center gap-1.5 overflow-hidden rounded-lg px-2 py-1 text-xs text-muted-foreground">
      <Loader2 className="relative z-10 h-3.5 w-3.5 shrink-0 animate-spin" aria-hidden="true" />
      <span className="relative z-10 truncate">{activity.label}</span>
      <span className="relative z-10 shrink-0">· {formatElapsed(now - activity.startedAt)}</span>
    </div>
  );
}

export interface ComposerHandle {
  /** 把一段文字作为引用片段加入输入区（划词「添加到对话」调用）。 */
  addQuote: (text: string) => void;
}

export const Composer = forwardRef(function Composer({
  sessionId,
  disabled,
  onSubmit,
  onEnsureSessionId,
  onStop,
  running,
  stopping,
  projects,
  selectedProjectId,
  agents,
  selectedAgentId,
  onPickAgent,
  workspaceName,
  workspacePath,
  onPickProject,
  onPickWorkspace,
  recentWorkspaces,
  onPickRecent,
  modelGroups,
  selectedModelId,
  onPickModel,
  teams,
  roleExperts,
  roleKind,
  roleId,
  onPickRole,
  planMode,
  onTogglePlan,
  permissionMode,
  globalPermMode,
  onChangePermission,
  onAttachFile,
  onPasteFile,
  initialAttachments,
  initialContent,
  onDraftChange,
  suggestions,
  onClearSuggestions,
  runActivity,
  contextUsage,
  sessionUsage,
  onCompact,
  compacting,
  hideWorkspacePicker,
  workspaceLocked,
  queuedTasks,
  onCancelQueued,
}: {
  /** 当前会话 id（读取附件图片预览用）。 */
  sessionId: string;
  disabled: boolean;
  onSubmit: (text: string) => Promise<void>;
  /** 草稿场景：惰性物化草稿为真会话并返回其 id（挂载资料库等需要真 id 时调用）。 */
  onEnsureSessionId?: () => Promise<string | null>;
  onStop?: () => void;
  running?: boolean;
  /** 已点停止、等待后端收口：STOP 键切「停止中」并禁用，避免重复点击与「点了没反应」。 */
  stopping?: boolean;
  projects?: Project[];
  selectedProjectId?: string | null;
  /** 可选持久智能体（绑定后用其专属工作目录 + 私有记忆）。 */
  agents?: Agent[];
  /** 当前会话绑定的持久智能体 id（命中即隐藏角色槽）。 */
  selectedAgentId?: string | null;
  /** 选择持久智能体作为当前会话角色（id="" = 解绑回自由模式）。 */
  onPickAgent?: (agentId: string) => void;
  /** 已选目录的名称（basename）；未选时为 undefined。 */
  workspaceName?: string;
  /** 已选目录的完整路径，仅用于 hover 提示。 */
  workspacePath?: string;
  onPickProject?: (projectId: string) => void;
  onPickWorkspace?: () => void;
  recentWorkspaces?: string[];
  onPickRecent?: (path: string) => void;
  /** 启用模型分组（按厂商），供模型下拉。 */
  modelGroups?: EnabledProviderModels[];
  /** 当前会话选中的模型 id（null/undefined = 用默认）。 */
  selectedModelId?: string | null;
  /** 选择某模型（null 表示用默认）。 */
  onPickModel?: (modelId: string | null) => void;
  /** 可选团队（角色槽下拉）。 */
  teams?: Team[];
  /** 可选散装 agent（角色槽「专家」分组）。 */
  roleExperts?: ExpertSummary[];
  /** 当前运行角色类型（""/"expert"/"team"）。 */
  roleKind?: string | null;
  /** 当前运行角色 id（expert name 或 team id）。 */
  roleId?: string | null;
  /** 选择角色（kind 空串 = 自由模式）。 */
  onPickRole?: (kind: string, id: string) => void;
  /** 是否处于计划模式（session.mode === "plan"）。 */
  planMode?: boolean;
  /** 切换计划模式（与 /plan 等价）。 */
  onTogglePlan?: () => void;
  /** 会话权限模式（null = 继承全局）。 */
  permissionMode?: PermissionMode | null;
  /** 全局权限默认（用于继承态展示）。 */
  globalPermMode?: PermissionMode;
  /** 切换会话权限模式。 */
  onChangePermission?: (mode: PermissionMode | null) => void;
  /** 添加附件：弹选择器+纳入工作目录，返回相对路径（取消返回 null）。 */
  onAttachFile?: () => Promise<string | null>;
  /** 粘贴/拖拽的文件或图片：写入工作目录，返回相对路径（失败返回 null）。 */
  onPasteFile?: (file: File) => Promise<string | null>;
  /** 打开草稿时的初始附件（来自 draft_content 解析）。 */
  initialAttachments?: Attachment[];
  /** 打开草稿时的初始正文（透传给 ComposerInput 注水）。 */
  initialContent?: string;
  /** 内容（正文或附件）变化回调，返回当前完整序列化串，供草稿防抖保存。 */
  onDraftChange?: (serialized: string) => void;
  /** 一轮结束后的快捷建议；展示在输入框上方，点击填入输入框。 */
  suggestions?: string[];
  /** 点击/消费建议后回调（用于清空建议）。 */
  onClearSuggestions?: () => void;
  /** 当前 run 的阶段文案和开始时间；显示在建议下方、输入框上方。 */
  runActivity?: RunActivity | null;
  /** 当前会话上下文窗口占用（null/undefined = 尚未载入，按 0% 展示）。 */
  contextUsage?: ContextUsageView | null;
  /** 当前会话累计 token 用量（null/undefined 或 0 次调用时不展示）。 */
  sessionUsage?: UsageTotals | null;
  /** 手动压缩较早历史（缺省则不显示压缩按钮）。 */
  onCompact?: () => void;
  /** 压缩进行中（禁用按钮 + 转圈）。 */
  compacting?: boolean;
  /** 嵌入式使用（如项目会话）隐藏「工作目录选择」（目录已由项目设定），上下文窗口条仍保留。 */
  hideWorkspacePicker?: boolean;
  /** 锁定工作目录（如关联项目的会话）：展示目录名 + 锁图标，不可更改。 */
  workspaceLocked?: boolean;
  /** T70：当前会话排队消息（塔状堆叠在输入框上方）。 */
  queuedTasks?: QueuedTask[];
  /** T70：取消某条排队消息。 */
  onCancelQueued?: (itemId: string) => void;
}, ref: ForwardedRef<ComposerHandle>) {
  const inputRef = useRef<ComposerInputHandle | null>(null);
  const attachIdRef = useRef(0);
  const [hasText, setHasText] = useState((initialContent ?? "").trim().length > 0);
  const [attachments, setAttachments] = useState<Attachment[]>(
    initialAttachments ?? [],
  );
  const [previewRelPath, setPreviewRelPath] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [quoteFragments, setQuoteFragments] = useState<string[]>([]);
  // 增强消息：进行中态、失败提示。
  const [enhancing, setEnhancing] = useState(false);
  const [enhanceError, setEnhanceError] = useState<string | null>(null);

  useImperativeHandle(
    ref,
    () => ({
      addQuote: (text: string) => {
        const t = text.trim();
        if (t) setQuoteFragments((prev) => [...prev, t]);
      },
    }),
    [],
  );

  // 把当前附件 + 正文序列化为整条待发内容（与提交时一致），供草稿保存。
  const serializeAll = (text: string) =>
    [attachments.map((a) => `⟦@${a.relPath}⟧`).join("\n"), text]
      .filter((s) => s.trim())
      .join("\n\n");

  // 附件变化时上报草稿内容（文本变化在 onContentChange 里上报）。跳过首挂载避免覆盖。
  const draftMounted = useRef(false);
  useEffect(() => {
    if (!draftMounted.current) {
      draftMounted.current = true;
      return;
    }
    onDraftChange?.(serializeAll(inputRef.current?.getText() ?? ""));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachments]);
  // 已启用技能（/ 菜单与 [+] 添加共用）；窗口聚焦时刷新，设置页改动即时生效。
  const [skills, setSkills] = useState<Skill[]>([]);
  const [pluginNameById, setPluginNameById] = useState<Record<string, string>>({});
  const [workspaceFiles, setWorkspaceFiles] = useState<string[]>([]);
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([]);
  const [mountedKbIds, setMountedKbIds] = useState<string[]>([]);
  useEffect(() => {
    const reload = () => {
      void Promise.allSettled([listSkills(), listPlugins()]).then(
        ([skillResult, pluginResult]) => {
          if (skillResult.status === "fulfilled") {
            setSkills(skillResult.value.filter((s) => s.enabled));
          } else {
            console.error(skillResult.reason);
          }

          if (pluginResult.status === "fulfilled") {
            setPluginNameById(
              Object.fromEntries(
                pluginResult.value.map((plugin) => [
                  plugin.id,
                  plugin.displayName || plugin.name || plugin.id,
                ]),
              ),
            );
          } else {
            console.error(pluginResult.reason);
          }
        },
      );
    };
    reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const reload = () => {
      void listSessionWorkspaceFiles(sessionId)
        .then((files) => {
          if (!cancelled) setWorkspaceFiles(files);
        })
        .catch((err) => {
          if (!cancelled) setWorkspaceFiles([]);
          console.error(err);
        });
    };
    reload();
    window.addEventListener("focus", reload);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", reload);
    };
  }, [sessionId, workspacePath]);

  // 资料库列表（供 + 菜单选择）。
  useEffect(() => {
    void kbList()
      .then(setKnowledgeBases)
      .catch(() => setKnowledgeBases([]));
  }, []);

  // 当前会话已挂载的资料库（草稿无 sessionId 时为空，挂载会惰性建会话）。
  useEffect(() => {
    if (!sessionId) {
      setMountedKbIds([]);
      return;
    }
    void kbMountedIds(sessionId)
      .then(setMountedKbIds)
      .catch(() => setMountedKbIds([]));
  }, [sessionId]);

  // 挂/卸资料库到当前会话；草稿时先物化会话再挂。
  const toggleKb = async (kbId: string) => {
    const id = sessionId || (onEnsureSessionId ? await onEnsureSessionId() : null);
    if (!id) return;
    const on = mountedKbIds.includes(kbId);
    const prev = mountedKbIds;
    setMountedKbIds(on ? mountedKbIds.filter((x) => x !== kbId) : [...mountedKbIds, kbId]);
    try {
      if (on) await kbUnmount(kbId, id);
      else await kbMount(kbId, id);
    } catch {
      setMountedKbIds(prev);
    }
  };

  const clearMountedKbs = async () => {
    if (!sessionId || mountedKbIds.length === 0) return;
    const prev = mountedKbIds;
    setMountedKbIds([]);
    try {
      await Promise.all(prev.map((id) => kbUnmount(id, sessionId)));
    } catch {
      setMountedKbIds(prev);
    }
  };

  const addAttachment = (relPath: string) => {
    const name = basename(relPath);
    setAttachments((prev) =>
      // 去重：同一相对路径只保留一张卡片。
      prev.some((a) => a.relPath === relPath)
        ? prev
        : [
            ...prev,
            { id: `att-${attachIdRef.current++}`, relPath, name, kind: attachmentKind(name) },
          ],
    );
  };

  const removeAttachment = (id: string) =>
    setAttachments((prev) => prev.filter((a) => a.id !== id));

  // [+] / 「添加文件」：弹选择器并加入顶部附件区。
  const addFileFromPicker = async () => {
    const rel = await onAttachFile?.();
    if (rel) addAttachment(rel);
  };

  // 粘贴/拖拽的文件或图片：逐个保存并加入附件区。
  const addPastedFiles = (files: File[]) => {
    void (async () => {
      for (const f of files) {
        const rel = await onPasteFile?.(f);
        if (rel) addAttachment(rel);
      }
    })();
  };

  const hasInput = hasText || attachments.length > 0 || quoteFragments.length > 0;
  const canSend = !disabled && !submitting && hasInput;
  // T70：运行中默认显示停止按钮；一旦用户有输入（文本/附件）就切回发送按钮，让其把消息入队。
  const showStop = running && !hasInput;

  // T78：所选模型不支持图像识别 + 含图片附件 → 非阻断提示（图片会以文件名形式发送）。
  const selectedModel = (modelGroups ?? [])
    .flatMap((g) => g.models)
    .find((m) => m.id === selectedModelId);
  const showVisionWarning =
    attachments.some((a) => a.kind === "image") &&
    selectedModel?.visionCapable === false;

  // 真正发送：附件 ⟦@相对路径⟧ + 引用片段 ⟦引用：文本⟧ + 正文，前置拼接；父级确认成功后再清空。
  // 引用以标记承载（与附件同源）：用户消息里渲染成 chip；后端去括号后模型看到「引用：文本」。
  // 文本内的 ⟦⟧ 会破坏标记配对，序列化时剔除。
  const doSubmit = async (text: string) => {
    if (disabled || submitting) return;
    const attachPart = attachments.map((a) => `⟦@${a.relPath}⟧`).join("\n");
    const quotePart = quoteFragments
      .map((q) => `⟦引用：${q.replace(/[⟦⟧]/g, "")}⟧`)
      .join("\n");
    const full = [attachPart, quotePart, text].filter((s) => s.trim()).join("\n\n");
    if (!full.trim()) return;
    setSubmitting(true);
    try {
      await onSubmit(full);
      inputRef.current?.clear();
      setAttachments([]);
      setQuoteFragments([]);
      setHasText(false);
      setEnhanceError(null);
    } finally {
      setSubmitting(false);
    }
  };

  // 增强消息：把当前正文润色 + 补全为清晰提示词，直接替换输入框正文。
  const doEnhance = async () => {
    const cur = inputRef.current?.getText() ?? "";
    if (!cur.trim() || enhancing) return;
    setEnhancing(true);
    setEnhanceError(null);
    try {
      const enhanced = await enhanceMessage(cur, sessionId);
      inputRef.current?.setText(enhanced);
    } catch (e) {
      setEnhanceError(typeof e === "string" ? e : "增强失败，请重试");
    } finally {
      setEnhancing(false);
    }
  };

  return (
    <div className="flex shrink-0 flex-col gap-1.5 p-3">
      {runActivity && <RunActivityInline activity={runActivity} />}
      {/* 一轮结束后的快捷建议：点击填入输入框待编辑 */}
      {suggestions && suggestions.length > 0 && (
        <div className="flex flex-row flex-wrap gap-0.5 pb-1">
          {suggestions.map((s, i) => (
            <Tooltip key={i} content={s}>
              <button
                type="button"
                onClick={() => {
                  inputRef.current?.setText(s);
                  onClearSuggestions?.();
                }}
                className="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] text-foreground-secondary transition hover:bg-accent"
              >
                <MessageSquare className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
                <span className="min-w-0 truncate">{s}</span>
              </button>
            </Tooltip>
          ))}
        </div>
      )}
      {/* T70：排队消息塔——堆叠在输入框正上方（Codex 风格塔状） */}
      {queuedTasks && onCancelQueued && (
        <QueuedMessages tasks={queuedTasks} onCancel={onCancelQueued} />
      )}
      {/* T78：非阻断提示——所选模型不支持图像识别时，图片仅以文件名形式发送 */}
      {showVisionWarning && (
        <div
          role="note"
          className="flex items-center gap-1.5 px-1 pb-0.5 text-[12px] text-foreground-muted"
        >
          <ImageOff className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span>当前模型不支持图像识别，图片将以文件名形式发送</span>
        </div>
      )}
      {/* 增强消息进行中：Loading 提示（输入框同时只读，避免改写覆盖用户新输入） */}
      {enhancing && (
        <div
          role="status"
          className="ui-activity-breathe relative flex items-center gap-1.5 overflow-hidden rounded-lg px-2 py-1 text-[12px] text-muted-foreground"
        >
          <Loader2 className="relative z-10 h-3.5 w-3.5 shrink-0 animate-spin" aria-hidden="true" />
          <span className="relative z-10">正在增强消息…</span>
        </div>
      )}
      {/* 增强消息失败：非阻断提示，原文不变 */}
      {enhanceError && !enhancing && (
        <div
          role="note"
          className="flex items-center gap-1.5 px-1 pb-0.5 text-[12px] text-foreground-muted"
        >
          <Wand2 className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span>{enhanceError}</span>
        </div>
      )}
      {/* 主输入容器：边框圆角，内含 附件区 + 编辑区 + 工具栏 */}
      <div className="relative rounded-xl border border-border bg-background focus-within:border-primary">
        {attachments.length > 0 && (
          <div className="flex flex-wrap gap-2 px-3 pt-3">
            {attachments.map((a) => (
              <AttachmentCard
                key={a.id}
                name={a.name}
                kind={a.kind}
                onRemove={() => removeAttachment(a.id)}
                onOpenImage={
                  a.kind === "image" ? () => setPreviewRelPath(a.relPath) : undefined
                }
              />
            ))}
          </div>
        )}
        {quoteFragments.length > 0 && (
          <div className="flex flex-wrap gap-2 px-3 pt-3">
            <QuoteChip fragments={quoteFragments} onClear={() => setQuoteFragments([])} />
          </div>
        )}
        <ComposerInput
          ref={inputRef}
          disabled={disabled || enhancing}
          skills={skills}
          workspaceFiles={workspaceFiles}
          initialContent={initialContent}
          onSubmit={doSubmit}
          onContentChange={(has) => {
            setHasText(has);
            onDraftChange?.(serializeAll(inputRef.current?.getText() ?? ""));
          }}
          onRequestFile={() => void addFileFromPicker()}
          onPasteFiles={addPastedFiles}
        />
        {/* 框内工具栏 */}
        <div className="flex items-center gap-2 px-2 py-2">
          <AddMenu
            pluginNameById={pluginNameById}
            planMode={planMode}
            skills={skills}
            knowledgeBases={knowledgeBases}
            mountedKbIds={mountedKbIds}
            onToggleKb={(id) => void toggleKb(id)}
            onTogglePlan={onTogglePlan}
            onPickSkill={(s) => inputRef.current?.insertSkill(s)}
            onAddFile={() => void addFileFromPicker()}
            roleValue={onPickRole ? { kind: roleKind ?? "", id: roleId ?? "" } : undefined}
            teams={teams}
            roleExperts={roleExperts}
            onPickRole={onPickRole}
          />
          {mountedKbIds.length > 0 && (
            <Tooltip
              content={
                <span className="block max-w-[240px]">
                  {mountedKbIds
                    .map((id) => knowledgeBases.find((k) => k.id === id)?.name ?? "资料库")
                    .join("、")}
                </span>
              }
            >
              <div className="group/kb-chip flex cursor-default items-center gap-1 rounded-md px-2 py-1.5 text-xs text-foreground-secondary transition hover:bg-accent">
                <button
                  type="button"
                  aria-label="移除资料库"
                  onClick={() => {
                    if (mountedKbIds.length === 1) void toggleKb(mountedKbIds[0]);
                    else void clearMountedKbs();
                  }}
                  className="grid h-4 w-4 cursor-pointer place-items-center rounded-sm text-foreground-muted transition hover:bg-muted hover:text-foreground"
                >
                  <BookMarked className="h-3.5 w-3.5 group-hover/kb-chip:hidden" aria-hidden="true" />
                  <CircleX className="hidden h-3.5 w-3.5 group-hover/kb-chip:block" aria-hidden="true" />
                </button>
                <span className="max-w-[140px] truncate">
                  {mountedKbIds.length === 1
                    ? knowledgeBases.find((k) => k.id === mountedKbIds[0])?.name ?? "资料库"
                    : `${mountedKbIds.length} 知识库`}
                </span>
              </div>
            </Tooltip>
          )}
          {planMode && onTogglePlan && (
            <Tooltip content="计划模式：先只读调研、提交计划等你批准后再执行">
              <div className="group/plan-chip flex cursor-default items-center gap-1 rounded-md px-2 py-1.5 text-xs text-foreground-secondary transition hover:bg-accent">
                <button
                  type="button"
                  aria-label="关闭计划模式"
                  onClick={onTogglePlan}
                  className="grid h-4 w-4 cursor-pointer place-items-center rounded-sm text-foreground-muted transition hover:bg-muted hover:text-foreground"
                >
                  <ClipboardList className="h-3.5 w-3.5 group-hover/plan-chip:hidden" aria-hidden="true" />
                  <CircleX className="hidden h-3.5 w-3.5 group-hover/plan-chip:block" aria-hidden="true" />
                </button>
                <span>计划</span>
              </div>
            </Tooltip>
          )}
          {/* 角色 chip：选了专家/团队时显示，点击复位默认（默认态零占用） */}
          {onPickRole && roleKind && (roleKind === "expert" || roleKind === "team") && (
            <Tooltip content={roleKind === "team" ? "当前团队，点击复位默认" : "当前专家，点击复位默认"}>
              <div className="group/role-chip flex cursor-default items-center gap-1 rounded-md px-2 py-1.5 text-xs text-foreground-secondary transition hover:bg-accent">
                <button
                  type="button"
                  aria-label="复位为默认角色"
                  onClick={() => onPickRole("", "")}
                  className="grid h-4 w-4 cursor-pointer place-items-center rounded-sm text-foreground-muted transition hover:bg-muted hover:text-foreground"
                >
                  {roleKind === "team" ? (
                    <>
                      <Users className="h-3.5 w-3.5 group-hover/role-chip:hidden" aria-hidden="true" />
                      <CircleX className="hidden h-3.5 w-3.5 group-hover/role-chip:block" aria-hidden="true" />
                    </>
                  ) : (
                    <>
                      <Bot className="h-3.5 w-3.5 group-hover/role-chip:hidden" aria-hidden="true" />
                      <CircleX className="hidden h-3.5 w-3.5 group-hover/role-chip:block" aria-hidden="true" />
                    </>
                  )}
                </button>
                <span className="max-w-[120px] truncate">
                  {roleKind === "team"
                    ? (teams ?? []).find((t) => t.id === roleId)?.displayName ?? "团队"
                    : (() => {
                        const ex = (roleExperts ?? []).find((a) => a.name === roleId);
                        return ex?.displayName || ex?.name || "专家";
                      })()}
                </span>
              </div>
            </Tooltip>
          )}
          <div className="flex-1" />
          {/* 增强消息：把草稿润色 + 补全为清晰提示词（右簇，权限左侧，无边框） */}
          <Tooltip content="增强消息：润色并补全为更清晰的提示词">
            <button
              type="button"
              aria-label="增强消息"
              onClick={() => void doEnhance()}
              disabled={disabled || enhancing || !hasText}
              className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-foreground disabled:opacity-40"
            >
              {enhancing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
              ) : (
                <Wand2 className="h-3.5 w-3.5" aria-hidden="true" />
              )}
            </button>
          </Tooltip>
          {onChangePermission && (
            <PermissionPicker
              value={permissionMode ?? null}
              globalDefault={globalPermMode ?? "manual"}
              onChange={onChangePermission}
            />
          )}
          <ModelPicker
            modelGroups={modelGroups ?? []}
            selectedModelId={selectedModelId ?? null}
            onPick={(id) => onPickModel?.(id)}
          />
          {showStop ? (
            <Tooltip content={stopping ? "停止中…" : "停止"}>
              <button
                type="button"
                aria-label={stopping ? "停止中" : "停止"}
                onClick={onStop}
                disabled={stopping}
                className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-destructive/10 text-destructive transition hover:opacity-90 disabled:opacity-60"
              >
                {stopping ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                ) : (
                  <Square className="h-3.5 w-3.5" aria-hidden="true" />
                )}
              </button>
            </Tooltip>
          ) : (
            <Tooltip content={running ? "排队发送" : "发送"}>
              <button
                type="button"
                aria-label={running ? "排队发送" : "发送"}
                onClick={() => void doSubmit(inputRef.current?.getText() ?? "")}
                disabled={!canSend}
                className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-primary text-primary-foreground transition hover:bg-primary/90 disabled:opacity-40"
              >
                <ArrowUp className="h-4 w-4" aria-hidden="true" />
              </button>
            </Tooltip>
          )}
        </div>
      </div>
      {/* 页脚行：工作目录 + 上下文窗口（真实用量）。嵌入式（如项目会话）可隐藏工作目录选择，仍保留上下文条。 */}
      <div className="flex items-center justify-between gap-2 px-1">
        {hideWorkspacePicker ? (
          <span />
        ) : (
          <WorkspacePicker
            projects={projects}
            selectedProjectId={selectedProjectId}
            agents={agents}
            selectedAgentId={selectedAgentId}
            onPickAgent={onPickAgent}
            workspaceName={workspaceName}
            workspacePath={workspacePath}
            onPickProject={onPickProject}
            onPickWorkspace={onPickWorkspace}
            recentWorkspaces={recentWorkspaces}
            onPickRecent={onPickRecent}
            locked={workspaceLocked}
          />
        )}
        <div className="flex items-center gap-2">
          <ContextMeter
            percent={contextUsage?.percent ?? 0}
            usedTokens={contextUsage?.usedTokens}
            maxTokens={contextUsage?.maxTokens}
          />
          <SessionUsageChip usage={sessionUsage} />
          {onCompact && (
            <Tooltip content="压缩较早历史，释放上下文窗口">
            <button
              type="button"
              onClick={onCompact}
              disabled={running || compacting}
              className="flex items-center gap-1 rounded-md px-1.5 py-1 text-xs text-foreground-muted transition hover:bg-accent hover:text-foreground disabled:opacity-40"
            >
              {compacting ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
              ) : (
                <Layers className="h-3.5 w-3.5" aria-hidden="true" />
              )}
              </button>
              </Tooltip>
          )}
        </div>
      </div>
      <AttachmentImageModal
        open={previewRelPath !== null}
        sessionId={sessionId}
        relPath={previewRelPath}
        name={previewRelPath ? basename(previewRelPath) : undefined}
        onClose={() => setPreviewRelPath(null)}
      />
    </div>
  );
});

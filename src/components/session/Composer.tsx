import { useEffect, useRef, useState } from "react";
import { ArrowUp, CircleX, ClipboardList, Layers, Loader2, MessageSquare, Square } from "lucide-react";
import { AddMenu } from "../../pages/session/composer/AddMenu";
import { ModelPicker } from "../../pages/session/composer/ModelPicker";
import { PermissionPicker } from "../../pages/session/composer/PermissionPicker";
import { WorkspacePicker } from "../../pages/session/composer/WorkspacePicker";
import { ContextMeter } from "../../pages/session/composer/ContextMeter";
import { SessionUsageChip } from "../../pages/session/composer/SessionUsageChip";
import { ComposerInput, type ComposerInputHandle } from "./ComposerInput";
import { AttachmentCard, AttachmentImageModal } from "./AttachmentCard";
import { QueuedMessages } from "./QueuedMessages";
import { listSessionWorkspaceFiles, listSkills } from "../../api";
import {
  attachmentKind,
  basename,
  type Attachment,
} from "../../lib/attachments";
import { Tooltip } from "../ui/Tooltip";
import type {
  Agent,
  ContextUsageView,
  EnabledProviderModels,
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

export function Composer({
  sessionId,
  disabled,
  onSubmit,
  onStop,
  running,
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
  onStop?: () => void;
  running?: boolean;
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
}) {
  const inputRef = useRef<ComposerInputHandle | null>(null);
  const attachIdRef = useRef(0);
  const [hasText, setHasText] = useState((initialContent ?? "").trim().length > 0);
  const [attachments, setAttachments] = useState<Attachment[]>(
    initialAttachments ?? [],
  );
  const [previewRelPath, setPreviewRelPath] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

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
  const [workspaceFiles, setWorkspaceFiles] = useState<string[]>([]);
  useEffect(() => {
    const reload = () => {
      void listSkills()
        .then((all) => setSkills(all.filter((s) => s.enabled)))
        .catch(console.error);
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

  const hasInput = hasText || attachments.length > 0;
  const canSend = !disabled && !submitting && hasInput;
  // T70：运行中默认显示停止按钮；一旦用户有输入（文本/附件）就切回发送按钮，让其把消息入队。
  const showStop = running && !hasInput;

  // 真正发送：把附件序列化为前置的 ⟦@相对路径⟧ 行 + 正文；父级确认成功后再清空。
  const doSubmit = async (text: string) => {
    if (disabled || submitting) return;
    const attachPart = attachments.map((a) => `⟦@${a.relPath}⟧`).join("\n");
    const full = [attachPart, text].filter((s) => s.trim()).join("\n\n");
    if (!full.trim()) return;
    setSubmitting(true);
    try {
      await onSubmit(full);
      inputRef.current?.clear();
      setAttachments([]);
      setHasText(false);
    } finally {
      setSubmitting(false);
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
        <ComposerInput
          ref={inputRef}
          disabled={disabled}
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
            planMode={planMode}
            skills={skills}
            onTogglePlan={onTogglePlan}
            onPickSkill={(s) => inputRef.current?.insertSkill(s)}
            onAddFile={() => void addFileFromPicker()}
          />
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
          <div className="flex-1" />
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
            <Tooltip content="停止">
              <button
                type="button"
                aria-label="停止"
                onClick={onStop}
                className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-destructive/10 text-destructive transition hover:opacity-90"
              >
                <Square className="h-3.5 w-3.5" aria-hidden="true" />
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
}

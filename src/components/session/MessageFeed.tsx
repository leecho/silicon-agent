import { useEffect, useState, type ReactNode } from "react";
import { Brain, CircleX, Loader2, MessageSquareReply, RefreshCw } from "lucide-react";
import { renderMessageWithChips } from "./messageChips";
import { AttachmentCard, AttachmentImageModal } from "./AttachmentCard";
import { extractAttachments } from "../../lib/attachments";
import { Tooltip } from "../ui/Tooltip";
import type { Artifact, FeedRow } from "../../types";
import { Disclosure } from "./Disclosure";
import { AssistantAnswer } from "./AssistantAnswer";
import { RoundArtifacts } from "./RoundArtifacts";
import { StepGroup } from "./MessageFeedToolSteps";
import { groupRows } from "./messageFeedRows";

export { Disclosure } from "./Disclosure";
export { buildPersistedRows } from "./messageFeedRows";

const USER_MESSAGE_COLLAPSE_CHAR_LIMIT = 420;
const USER_MESSAGE_COLLAPSE_LINE_LIMIT = 8;

function isLongUserMessage(content: string) {
  return (
    content.length > USER_MESSAGE_COLLAPSE_CHAR_LIMIT ||
    content.split(/\r\n|\r|\n/).length > USER_MESSAGE_COLLAPSE_LINE_LIMIT
  );
}

type AskAnswerRow = {
  question: string;
  answer: string;
  unanswered: boolean;
};

function parseAskAnswerRows(content: string): AskAnswerRow[] {
  const body = content.replace(/^用户已回答：\s*/, "").trim();
  if (!body) return [];

  return body
    .split(/\r\n|\r|\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const match = line.match(/^\d+\.\s*(.+?)[：:]\s*(.*)$/);
      const question = match?.[1]?.trim() || "回答";
      const answer = match?.[2]?.trim() || line;
      return {
        question,
        answer,
        unanswered: answer === "（未回答）" || answer === "(未回答)",
      };
    });
}

export function MessageFeed({
  sessionId,
  rows,
  streamingId,
  artifactsByRound,
  onOpenArtifact,
  resolvedWorkingDir,
  onRetry,
  retryDisabled,
  onDispatchAgentClick,
  agentDisplayNames,
  thinking,
}: {
  /** 当前会话 id（读取附件图片预览用）。 */
  sessionId: string;
  rows: FeedRow[];
  streamingId?: string | null;
  /** 本会话有 run 在跑（与 composer 呼吸灯同源）：在尚无可见流式内容/工具活动时，feed 末尾即时显示「思考中」。 */
  thinking?: boolean;
  // 按「轮根」（该轮首条 user 消息 id）分组的产物；在每轮末尾汇总展示。
  artifactsByRound?: Map<string, Artifact[]>;
  onOpenArtifact?: (a: Artifact) => void;
  resolvedWorkingDir?: string;
  /** 失败错误块「重试」回调；缺省则不显示重试按钮。 */
  onRetry?: () => void;
  /** 重试按钮禁用（运行中）。 */
  retryDisabled?: boolean;
  /** 点击 dispatch_agent 工具行的「打开专家」入口（toolCallId, expertName）。 */
  onDispatchAgentClick?: (toolCallId: string, expertName: string) => void;
  /** agent name → 展示名（把派发卡里的 image-creator 显示成 珀西）。 */
  agentDisplayNames?: Record<string, string>;
}) {
  // 历史消息里点击图片附件 → 预览的相对路径。
  const [previewRelPath, setPreviewRelPath] = useState<string | null>(null);
  const hasActiveStep = rows.some(
    (row) =>
      row.kind === "tool" &&
      row.startedAt !== undefined &&
      row.finishedAt === undefined &&
      (row.status === "running" || row.status === "generating"),
  );
  const [stepNow, setStepNow] = useState(() => Date.now());
  useEffect(() => {
    if (!hasActiveStep) return;
    setStepNow(Date.now());
    const timer = window.setInterval(() => setStepNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [hasActiveStep]);

  const grouped = groupRows(rows);
  const elements: ReactNode[] = [];
  let currentRoot: string | null = null;
  const flushRound = () => {
    if (!currentRoot) return;
    const arts = artifactsByRound?.get(currentRoot)?.filter((a) => a.kind !== "working") ?? [];
    if (arts && arts.length > 0) {
      elements.push(
        <RoundArtifacts
          key={"round-artifacts:" + currentRoot}
          sessionId={sessionId}
          artifacts={arts}
          onOpen={onOpenArtifact}
          resolvedWorkingDir={resolvedWorkingDir}
        />,
      );
    }
  };

  for (const row of grouped) {
    if (row.kind === "user") {
      // 进入新一轮：先收口上一轮的产物汇总，再渲染这条 user 消息。
      flushRound();
      currentRoot = row.id;
      const { attachments, body } = extractAttachments(row.content);
      elements.push(
        <UserMessageBubble
          key={row.id}
          attachments={attachments}
          body={body}
          onOpenImage={setPreviewRelPath}
        />,
      );
    } else if (row.kind === "toolGroup") {
      elements.push(
        <div key={row.id} className="min-w-0 max-w-full">
          <StepGroup
            steps={row.steps}
            now={stepNow}
            onDispatchAgentClick={onDispatchAgentClick}
            agentDisplayNames={agentDisplayNames}
          />
        </div>,
      );
    } else if (row.kind === "divider") {
      // 压缩分隔线：居中淡色，两侧细线。
      elements.push(
        <div
          key={row.id}
          className="my-1 flex items-center gap-3 text-xs text-foreground-muted"
        >
          <span className="h-px flex-1 bg-border" />
          <span className="shrink-0">{row.content}</span>
          <span className="h-px flex-1 bg-border" />
        </div>,
      );
    } else if (row.kind === "askAnswer") {
      elements.push(<AskAnswerSummary key={row.id} content={row.content} />);
    } else if (row.kind === "error") {
      // 模型调用失败：始终可见错误详情，存在回调时展示「重试」按钮。
      elements.push(
        <div key={row.id} className="min-w-0 max-w-full">
          {onRetry && (
            <div>
              <div className="flex items-center gap-1">
                <span className="text-foreground-muted">
                  <CircleX className="h-3.5 w-3.5" aria-hidden="true" />
                </span>
                <span className="text-[13px] text-foreground-secondary">模型调用失败</span>
                <Tooltip content="重试">
                  <button
                    type="button"
                    disabled={retryDisabled}
                    className="inline-flex h-7 items-center gap-1 rounded-md px-1 text-xs font-medium text-foreground-muted transition hover:bg-accent hover:text-foreground"
                    onClick={onRetry}
                  >
                    <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                  </button>
                </Tooltip>
              </div>
              <span className="text-xs text-destructive">{row.content}</span>
            </div>
          )}
        </div>,
      );
    } else {
      // assistant：当前流式行且尚无答案内容 → 思考中（默认展开 + 转动 + 文案切换）。
      const thinking = streamingId === row.id && row.content.length === 0;
      elements.push(
        <div key={row.id} className="min-w-0 max-w-full">
          {row.reasoning && row.reasoning.length > 0 && (
            <Disclosure
              forceOpen={thinking}
              icon={
                thinking ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                ) : (
                  <Brain size={13} aria-hidden="true" />
                )
              }
              label={thinking ? "思考中" : "深度思考"}
            >
              {row.reasoning}
            </Disclosure>
          )}
          {row.content.length > 0 && <AssistantAnswer markdown={row.content} />}
        </div>,
      );
    }
  }
  // 收口最后一轮的产物汇总。
  flushRound();

  // 与 composer 呼吸灯同步：run 在跑，但尚无可见流式内容/运行中工具时，末尾即时显示「思考中」，
  // 消除「composer 已亮、feed 还没出现思考中」的滞后。
  const streamingRow = streamingId ? rows.find((r) => r.id === streamingId) : undefined;
  const streamingVisible =
    !!streamingRow &&
    streamingRow.kind === "assistant" &&
    ((streamingRow.reasoning?.length ?? 0) > 0 || streamingRow.content.length > 0);
  const showThinking = !!thinking && !streamingVisible && !hasActiveStep;

  return (
    <div className="mx-auto flex w-full min-w-0 max-w-full flex-col gap-2 px-4 pb-3 pt-2">
      {elements}
      {showThinking && (
        <div className="flex items-center gap-2 px-1 text-[13px] text-foreground-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          思考中…
        </div>
      )}
      <AttachmentImageModal
        open={previewRelPath !== null}
        sessionId={sessionId}
        relPath={previewRelPath}
        name={previewRelPath ? previewRelPath.split("/").pop() : undefined}
        onClose={() => setPreviewRelPath(null)}
      />
    </div>
  );
}

function AskAnswerSummary({ content }: { content: string }) {
  const rows = parseAskAnswerRows(content);
  const fallback = content.replace(/^用户已回答：\s*/, "").trim() || "（未回答）";

  return (
    <div className="min-w-0 max-w-full">
      <div className="min-w-0 max-w-[78%] rounded-[10px] border border-border-subtle bg-surface px-3 py-2.5 shadow-sm">
        <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium text-foreground-secondary">
          <MessageSquareReply className="h-3.5 w-3.5" aria-hidden="true" />
          问答摘要
        </div>
        {rows.length > 0 ? (
          <div className="overflow-hidden rounded-md border border-border-subtle bg-background/50">
            {rows.map((row, index) => (
              <div
                key={`${row.question}-${index}`}
                className="grid grid-cols-[minmax(96px,0.38fr)_minmax(0,1fr)] gap-3 border-t border-border-subtle px-2.5 py-1.5 first:border-t-0 max-sm:grid-cols-1 max-sm:gap-0.5"
              >
                <p className="text-xs leading-5 text-foreground-muted [overflow-wrap:anywhere]">
                  {row.question}
                </p>
                <p
                  className={`text-sm leading-5 [overflow-wrap:anywhere] ${
                    row.unanswered ? "text-foreground-muted" : "text-foreground"
                  }`}
                >
                  {row.answer}
                </p>
              </div>
            ))}
          </div>
        ) : (
          <p className="whitespace-pre-wrap text-sm leading-5 text-foreground [overflow-wrap:anywhere]">
            {fallback}
          </p>
        )}
      </div>
    </div>
  );
}

function UserMessageBubble({
  attachments,
  body,
  onOpenImage,
}: {
  attachments: ReturnType<typeof extractAttachments>["attachments"];
  body: string;
  onOpenImage: (relPath: string) => void;
}) {
  const trimmedBody = body.trim();
  const collapsible = isLongUserMessage(trimmedBody);
  const [expanded, setExpanded] = useState(false);
  const collapsed = collapsible && !expanded;

  return (
    <div className="flex min-w-0 max-w-full justify-end">
      <div className="min-w-0 max-w-[78%] rounded-[12px] border border-border bg-primary px-4 py-2.5 text-[#ffffff] shadow-sm">
        {attachments.length > 0 && (
          <div className="mb-2 flex flex-wrap gap-2">
            {attachments.map((a, i) => (
              <AttachmentCard
                key={`${a.relPath}-${i}`}
                name={a.name}
                kind={a.kind}
                onOpenImage={a.kind === "image" ? () => onOpenImage(a.relPath) : undefined}
              />
            ))}
          </div>
        )}
        {trimmedBody.length > 0 && (
          <>
            <div className={collapsed ? "max-h-32 overflow-hidden" : undefined}>
              <p className="whitespace-pre-wrap [overflow-wrap:anywhere] text-sm leading-5">
                {renderMessageWithChips(body)}
              </p>
            </div>
            {collapsible && (
              <button
                type="button"
                className="mt-2 text-xs font-medium text-white/70 transition hover:text-white hover:underline"
                onClick={() => setExpanded((current) => !current)}
              >
                {expanded ? "收起" : "查看更多"}
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}

import { CheckCircle2, CircleX, Eye, Loader2 } from "lucide-react";
import type { ReactNode } from "react";
import type { FeedRow } from "../../types";
import { parseDispatchName, toolNarrative } from "./toolNarrative";
import { Disclosure } from "./Disclosure";
import { MarkdownText } from "../ui/MarkdownText";
import type { ProcessItem } from "./messageFeedRows";

function stepIcon(status: "generating" | "running" | "done" | "failed") {
  if (status === "running" || status === "generating")
    return (
      <Loader2
        className="h-3.5 w-3.5 shrink-0 animate-spin text-foreground-muted"
        aria-hidden="true"
      />
    );
  if (status === "failed")
    return (
      <CircleX
        className="h-3.5 w-3.5 shrink-0 text-danger"
        aria-hidden="true"
      />
    );
  return (
    <CheckCircle2
      className="h-3.5 w-3.5 shrink-0 text-success"
      aria-hidden="true"
    />
  );
}

export function formatStepElapsed(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  if (totalSeconds < 1) return "<1s";
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) return seconds > 0 ? `${minutes}m ${seconds}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  return restMinutes > 0 ? `${hours}h ${restMinutes}m` : `${hours}h`;
}

function StepDuration({
  row,
  now,
}: {
  row: Extract<FeedRow, { kind: "tool" }>;
  now: number;
}) {
  if (row.startedAt === undefined) return null;
  const end = row.finishedAt ?? now;
  return (
    <span className="shrink-0 text-foreground-muted">
      · {formatStepElapsed(end - row.startedAt)}
    </span>
  );
}

/** 从 dispatch_agent 工具行的 input 取专家名（用于「打开专家」入口）。流式半截 JSON 也能提取。 */
function dispatchAgentName(input: string): string {
  return parseDispatchName(input);
}

// 归一过程区文本的多余空白：
// ① 去行尾空白 + 把「2+ 连续换行」压成一个空行 + trim；
// ② 折叠「松散列表」——列表项之间的空行会让 markdown 把每项内容包进 <p> 并产生空白
//    文本节点，WebKit 下把 <li> 撑高一大截。去掉「紧跟列表项的空行」令其渲染为紧凑列表
//    （<li>文本</li>，无内层 <p>），消除项内凭空多出的空白。
function tidyProcessText(text: string): string {
  const collapsed = text
    .replace(/[ \t]+\n/g, "\n")
    .replace(/(?:[ \t]*\n){2,}/g, "\n\n")
    .trim();
  const lines = collapsed.split("\n");
  const isListItem = (l: string) => /^\s*(?:[-*+]|\d+[.)])\s+/.test(l);
  const out: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() === "") {
      let j = i + 1;
      while (j < lines.length && lines[j].trim() === "") j++;
      if (j < lines.length && isListItem(lines[j])) continue; // 空行后紧跟列表项 → 丢弃
    }
    out.push(lines[i]);
  }
  return out.join("\n");
}

function StepItem({
  row,
  now = Date.now(),
  onDispatchAgentClick,
  agentDisplayNames,
}: {
  row: Extract<FeedRow, { kind: "tool" }>;
  now?: number;
  onDispatchAgentClick?: (toolCallId: string, expertName: string) => void;
  agentDisplayNames?: Record<string, string>;
}) {
  const isDispatch = row.toolName === "dispatch_agent";
  return (
    <div className="flex min-w-0 max-w-full items-start gap-2 text-[13px]">
      <div className="min-w-0 max-w-full flex-1">
        <Disclosure
          label={
            <span className="flex min-w-0 items-center gap-1">
              <span className="truncate">
                {toolNarrative(row.toolName, row.status, row.input, agentDisplayNames)}
              </span>
              <StepDuration row={row} now={now} />
              {isDispatch && onDispatchAgentClick && (
                <button
                  type="button"
                  className="shrink-0 rounded px-1.5 text-primary hover:underline"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDispatchAgentClick(
                      row.toolCallId ?? row.id,
                      dispatchAgentName(row.input),
                    );
                  }}
                >
                  查看
                </button>
              )}
            </span>
          }
          icon={stepIcon(row.status)}
          mono
        >
          <div className="flex max-h-[320px] min-w-0 max-w-full flex-col gap-1.5 overflow-auto rounded-lg border border-border-subtle bg-surface px-3 py-1 text-[12px] leading-5 text-foreground-muted">
            <div className="min-w-0 max-w-full whitespace-pre-wrap [overflow-wrap:anywhere]">
              {row.input}
            </div>
            <div className="min-w-0 max-w-full whitespace-pre-wrap [overflow-wrap:anywhere]">
              {row.output}
            </div>
          </div>
        </Disclosure>
      </div>
    </div>
  );
}

export function ProcessGroup({
  items,
  now,
  streamingId,
  runActive,
  onDispatchAgentClick,
  agentDisplayNames,
}: {
  items: ProcessItem[];
  now: number;
  /** 当前流式消息 id：过程区含其派生项（思考/旁白）且尚无答案时判定为「运行中」。 */
  streamingId?: string | null;
  /** run 仍活跃且本组是末尾过程组（本轮尚未产出答案，如正等待子专家返回）→ 维持运行态。 */
  runActive?: boolean;
  onDispatchAgentClick?: (toolCallId: string, expertName: string) => void;
  agentDisplayNames?: Record<string, string>;
}) {
  const tools = items.filter(
    (it): it is Extract<FeedRow, { kind: "tool" }> => it.kind === "tool",
  );
  // 运行中动作（取最后一个运行/生成中的工具，用于实时标题）。
  const runningTool = [...tools]
    .reverse()
    .find((t) => t.status === "running" || t.status === "generating");
  // 运行中 = 有运行工具，或过程区仍含流式消息的派生项（思考/旁白尚未收尾为答案），
  // 或 run 仍活跃且本组是末尾（工具已完成但正等待子专家/后续步骤）。
  const live =
    !!runningTool ||
    !!runActive ||
    (streamingId != null && items.some((it) => it.id === streamingId));

  // 时长跨度：最早 startedAt 到最晚 finishedAt；运行中一律用 now 作结束，使时长持续增长。
  const starts = tools
    .map((t) => t.startedAt)
    .filter((v): v is number => v !== undefined);
  const ends = tools.map((t) => (live ? now : t.finishedAt ?? now));
  const span =
    starts.length > 0 ? Math.max(...ends) - Math.min(...starts) : null;
  const spanText = span !== null ? formatStepElapsed(span) : null;

  let title: ReactNode;
  if (runningTool) {
    const narrative = toolNarrative(
      runningTool.toolName,
      runningTool.status,
      runningTool.input,
      agentDisplayNames,
    );
    title = `正在${narrative}…${spanText ? " · " + spanText : ""}`;
  } else if (live && tools.length > 0) {
    // 工具已完成但 run 仍在跑（如等待子专家）：维持「处理中」并显示增长时长。
    title = `处理中…${spanText ? " · " + spanText : ""}`;
  } else if (live) {
    title = "思考中…";
  } else if (tools.length > 0) {
    title = `已处理 ${spanText ? spanText + " · " : ""}${tools.length} 步骤`;
  } else {
    title = "已处理";
  }

  return (
    <Disclosure
      icon={<Eye className="h-3.5 w-3.5" aria-hidden="true" />}
      label={title}
      forceOpen={live}
      flush
    >
      <div className="flex min-w-0 max-w-full flex-col gap-1.5">
        {items.map((it) => {
          // 思考与旁白同样式：过程区内渲染为弱化的 markdown（复用 MarkdownText）。
          if (it.kind === "thinking" || it.kind === "narration") {
            return (
              <div key={it.id + ":" + it.kind} className="min-w-0 max-w-full">
                <MarkdownText
                  value={tidyProcessText(it.text)}
                  muted
                  className="text-[13px] [overflow-wrap:anywhere]"
                />
              </div>
            );
          }
          return (
            <StepItem
              row={it}
              now={now}
              onDispatchAgentClick={onDispatchAgentClick}
              agentDisplayNames={agentDisplayNames}
              key={it.id}
            />
          );
        })}
      </div>
    </Disclosure>
  );
}

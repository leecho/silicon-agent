import { CheckCircle2, CircleX, Eye, Loader2 } from "lucide-react";
import type { FeedRow } from "../../types";
import { parseDispatchName, toolNarrative } from "./toolNarrative";
import { Disclosure } from "./Disclosure";

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
    <li className="flex min-w-0 max-w-full items-start gap-2 text-[13px]">
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
    </li>
  );
}

export function StepGroup({
  steps,
  now,
  onDispatchAgentClick,
  agentDisplayNames,
}: {
  steps: Array<Extract<FeedRow, { kind: "tool" }>>;
  now: number;
  onDispatchAgentClick?: (toolCallId: string, expertName: string) => void;
  agentDisplayNames?: Record<string, string>;
}) {
  const anyRunning = steps.some(
    (s) => s.status === "running" || s.status === "generating",
  );
  return (
    <Disclosure
      icon={<Eye className="h-3.5 w-3.5" aria-hidden="true" />}
      label={`查看 ${steps.length} 个步骤`}
      forceOpen={anyRunning}
    >
      <ol className="flex min-w-0 max-w-full flex-col gap-1.5">
        {steps.map((step) => (
          <StepItem
            row={step}
            now={now}
            onDispatchAgentClick={onDispatchAgentClick}
            agentDisplayNames={agentDisplayNames}
            key={step.id}
          />
        ))}
      </ol>
    </Disclosure>
  );
}

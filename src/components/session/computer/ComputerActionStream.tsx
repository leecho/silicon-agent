import { useMemo } from "react";
import {
  CheckCircle2,
  CircleX,
  Loader2,
  MonitorPlay,
  MousePointerClick,
  Keyboard,
  Type as TypeIcon,
  Eye,
  MoveVertical,
  Hourglass,
} from "lucide-react";
import type { FeedRow } from "../../../types";
import { EmptyState, Skeleton } from "../../ui";
import { computerCopy } from "./copy";

/** 仅消费桌面操作（computer 工具）的步骤行。 */
const COMPUTER_TOOL = "computer";

type ToolRow = Extract<FeedRow, { kind: "tool" }>;

type StepView = {
  id: string;
  verb: string;
  detail: string;
  status: ToolRow["status"];
  at?: number;
};

/**
 * 把 computer 工具行翻译成「用户视角」步骤：只暴露动作（查看屏幕/点击/输入…）与
 * 必要的用户输入文本，绝不暴露元素 id、坐标、屏幕原始内容、工具名或「action」等技术细节。
 */
function toStepView(row: ToolRow): StepView {
  let action = "";
  let typedText = "";
  try {
    const args = JSON.parse(row.input || "{}");
    if (args && typeof args === "object") {
      if (typeof args.action === "string") action = args.action;
      if (typeof args.text === "string") typedText = args.text;
    }
  } catch {
    // 流式半截 JSON：保持空动作，回退到默认「查看屏幕」措辞。
  }

  let verb: string = computerCopy.stepLook;
  switch (action) {
    case "click":
    case "double_click":
      verb = computerCopy.stepClick;
      break;
    case "type":
      verb = computerCopy.stepType;
      break;
    case "key":
      verb = computerCopy.stepKey;
      break;
    case "scroll":
      verb = computerCopy.stepScroll;
      break;
    case "wait":
      verb = computerCopy.stepWait;
      break;
    case "observe":
    default:
      verb = computerCopy.stepLook;
      break;
  }

  // 仅「输入」动作回显用户实际键入的文本（用户视角内容，非技术细节）；其余动作不暴露任何原始输出。
  let detail = "";
  if (action === "type" && typedText) {
    detail = typedText.length > 40 ? typedText.slice(0, 40) + "…" : typedText;
  }

  return { id: row.id, verb, detail, status: row.status, at: row.finishedAt ?? row.startedAt };
}

function stepIcon(verb: string, status: ToolRow["status"]) {
  if (status === "running" || status === "generating")
    return <Loader2 className="h-4 w-4 shrink-0 animate-spin text-foreground-muted" aria-hidden="true" />;
  if (status === "failed")
    return <CircleX className="h-4 w-4 shrink-0 text-danger" aria-hidden="true" />;
  if (verb === computerCopy.stepClick)
    return <MousePointerClick className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === computerCopy.stepType)
    return <TypeIcon className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === computerCopy.stepKey)
    return <Keyboard className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === computerCopy.stepScroll)
    return <MoveVertical className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === computerCopy.stepWait)
    return <Hourglass className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  return <Eye className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
}

function formatAt(at?: number): string {
  if (!at) return "";
  try {
    return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  } catch {
    return "";
  }
}

export function ComputerActionStream({
  rows,
  feedVersion,
  running,
}: {
  /** 当前会话的全部 feed 行（本组件内部按 computer 工具过滤）。 */
  rows: FeedRow[];
  /** feed 重渲染计数器：rows 数组原地 push，靠此值触发步骤过滤重算。 */
  feedVersion: number;
  /** 会话是否处于运行中（用于加载态骨架）。 */
  running: boolean;
}) {
  const steps = useMemo(
    () =>
      rows
        .filter((r): r is ToolRow => r.kind === "tool" && r.toolName === COMPUTER_TOOL)
        .map(toStepView),
    // rows 为原地 mutate 的 ref 稳定数组；feedVersion 变化才代表内容更新，必须入依赖。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rows, feedVersion],
  );

  const hasFailure = steps.some((s) => s.status === "failed");

  // 加载态：运行中但还没有任何步骤（正在准备）→ 骨架屏，保持布局稳定。
  if (running && steps.length === 0) {
    return (
      <div className="flex flex-col gap-3 px-1 py-2">
        <p className="px-2 text-xs text-foreground-muted">{computerCopy.loading}</p>
        <Skeleton lines={3} />
      </div>
    );
  }

  // 空态：未运行且无步骤 → 就绪提示（桌面能力随总开关自动激活，无需手动开启）。
  if (steps.length === 0) {
    return (
      <EmptyState
        icon={<MonitorPlay className="h-6 w-6" aria-hidden="true" />}
        title={computerCopy.emptyTitle}
        description={computerCopy.emptyHint}
      />
    );
  }

  // 有数据：时间线 +（如有失败）内联错误块 + 常驻停止按钮。
  return (
    <div className="flex flex-col gap-3 px-1 py-2">
      <ol className="flex flex-col gap-1.5" aria-label={computerCopy.featureName}>
        {steps.map((s) => (
          <li key={s.id} className="flex items-start gap-2 rounded-lg px-2 py-1.5 text-[13px]">
            {stepIcon(s.verb, s.status)}
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="flex min-w-0 items-center gap-1.5">
                {s.status === "done" && (
                  <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-success" aria-hidden="true" />
                )}
                <span className="shrink-0 font-medium text-foreground">{s.verb}</span>
                {s.detail && (
                  <span className="min-w-0 flex-1 truncate text-foreground-muted">「{s.detail}」</span>
                )}
              </span>
              {formatAt(s.at) && (
                <span className="text-[11px] text-foreground-muted">{formatAt(s.at)}</span>
              )}
            </div>
          </li>
        ))}
      </ol>

      {hasFailure && (
        <div className="mx-2 rounded-lg border border-danger-border bg-danger-subtle px-3 py-2 text-[12px]">
          <p className="font-medium text-danger">{computerCopy.errorTitle}</p>
        </div>
      )}
    </div>
  );
}

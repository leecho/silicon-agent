import { useMemo, type ReactNode } from "react";
import {
  CheckCircle2,
  CircleX,
  Loader2,
  Globe,
  Link as LinkIcon,
  MousePointerClick,
  Pencil,
  ListChecks,
  MoveVertical,
  FileText,
  Hourglass,
  CornerUpLeft,
} from "lucide-react";
import type { FeedRow } from "../../../types";
import { EmptyState, Skeleton } from "../../ui";
import { browserCopy } from "./copy";

/** 仅消费浏览器操作（browser 工具）的步骤行。 */
const BROWSER_TOOL = "browser";

type ToolRow = Extract<FeedRow, { kind: "tool" }>;

type StepView = {
  id: string;
  verb: string;
  detail: string;
  status: ToolRow["status"];
  at?: number;
};

/**
 * 把 browser 工具行翻译成「用户视角」步骤：只暴露动作（打开网页/查看页面/点击…）与
 * 必要的用户可读内容（网址、填写文本），绝不暴露元素编号、选择器、页面原始结构、工具名或「action」等技术细节。
 */
function toStepView(row: ToolRow): StepView {
  let action = "";
  let url = "";
  let typedText = "";
  try {
    const args = JSON.parse(row.input || "{}");
    if (args && typeof args === "object") {
      if (typeof args.action === "string") action = args.action;
      if (typeof args.url === "string") url = args.url;
      if (typeof args.text === "string") typedText = args.text;
    }
  } catch {
    // 流式半截 JSON：保持空动作，回退到默认「查看页面」措辞。
  }

  let verb: string = browserCopy.stepLook;
  switch (action) {
    case "navigate":
      verb = browserCopy.stepOpen;
      break;
    case "click":
    case "double_click":
      verb = browserCopy.stepClick;
      break;
    case "fill":
      verb = browserCopy.stepFill;
      break;
    case "select":
      verb = browserCopy.stepSelect;
      break;
    case "scroll":
      verb = browserCopy.stepScroll;
      break;
    case "extract":
      verb = browserCopy.stepExtract;
      break;
    case "wait":
      verb = browserCopy.stepWait;
      break;
    case "back":
      verb = browserCopy.stepBack;
      break;
    case "observe":
    default:
      verb = browserCopy.stepLook;
      break;
  }

  // 仅「打开网页」回显网址、「填写」回显用户实际键入的文本（均为用户视角内容，非技术细节）；
  // 其余动作不暴露任何页面原始输出、选择器或元素编号。
  let detail = "";
  if (action === "navigate" && url) {
    // 去掉协议 / www 前缀更易读；不再手工截断，由 CSS truncate 按可用宽度收尾（动词永不被挤断）。
    detail = url.replace(/^https?:\/\//, "").replace(/^www\./, "");
  } else if (action === "fill" && typedText) {
    detail = typedText.length > 40 ? typedText.slice(0, 40) + "…" : typedText;
  }

  return { id: row.id, verb, detail, status: row.status, at: row.finishedAt ?? row.startedAt };
}

function stepIcon(verb: string, status: ToolRow["status"]) {
  if (status === "running" || status === "generating")
    return <Loader2 className="h-4 w-4 shrink-0 animate-spin text-foreground-muted" aria-hidden="true" />;
  if (status === "failed")
    return <CircleX className="h-4 w-4 shrink-0 text-danger" aria-hidden="true" />;
  if (verb === browserCopy.stepOpen)
    return <LinkIcon className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === browserCopy.stepClick)
    return <MousePointerClick className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === browserCopy.stepFill)
    return <Pencil className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === browserCopy.stepSelect)
    return <ListChecks className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === browserCopy.stepScroll)
    return <MoveVertical className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === browserCopy.stepExtract)
    return <FileText className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === browserCopy.stepWait)
    return <Hourglass className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  if (verb === browserCopy.stepBack)
    return <CornerUpLeft className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
  return <Globe className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />;
}

function formatAt(at?: number): string {
  if (!at) return "";
  try {
    return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  } catch {
    return "";
  }
}

export function BrowserActionStream({
  rows,
  feedVersion,
  running,
  emptyExtra,
}: {
  /** 当前会话的全部 feed 行（本组件内部按 browser 工具过滤）。 */
  rows: FeedRow[];
  /** feed 重渲染计数器：rows 数组原地 push，靠此值触发步骤过滤重算。 */
  feedVersion: number;
  /** 会话是否处于运行中（用于加载态骨架）。 */
  running: boolean;
  /** 空态下附加内容（如登录提示 + 打开浏览器按钮），与空态文案一同居中显示。 */
  emptyExtra?: ReactNode;
}) {
  const steps = useMemo(
    () =>
      rows
        .filter((r): r is ToolRow => r.kind === "tool" && r.toolName === BROWSER_TOOL)
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
        <p className="px-2 text-xs text-foreground-muted">{browserCopy.loading}</p>
        <Skeleton lines={3} />
      </div>
    );
  }

  // 空态：未运行且无步骤 → 就绪提示（浏览器能力随总开关自动激活，无需手动开启）。
  // 「打开浏览器 + 登录说明」放进 EmptyState 的 action 槽，与图标/标题/说明同属一个居中 hero，
  // 不再是悬在下方的独立描边卡，排版更紧凑友好。
  if (steps.length === 0) {
    return (
      <EmptyState
        icon={<Globe className="h-6 w-6" aria-hidden="true" />}
        title={browserCopy.emptyTitle}
        description={browserCopy.emptyHint}
        action={emptyExtra}
      />
    );
  }

  // 有数据：时间线 +（如有失败）内联错误块。停止统一走会话顶部的全局停止，这里不再放停止按钮。
  return (
    <div className="flex flex-col gap-3 px-1 py-2">
      <ol className="flex flex-col gap-1.5" aria-label={browserCopy.featureName}>
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
          <p className="font-medium text-danger">{browserCopy.errorTitle}</p>
        </div>
      )}
    </div>
  );
}

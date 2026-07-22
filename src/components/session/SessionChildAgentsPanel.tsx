import type { LucideIcon } from "lucide-react";
import {
  Ban,
  Bot,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CirclePause,
  CircleX,
  Loader2,
} from "lucide-react";
import { useState } from "react";
import { avatarEmoji } from "../../lib/avatar";
import type { ChildAgentSummary } from "../../types";

const STATUS: Record<
  string,
  {
    text: string;
    textClassName: string;
    iconClassName: string;
    icon: LucideIcon;
    spinning?: boolean;
  }
> = {
  running: {
    text: "运行中",
    textClassName: "text-primary",
    iconClassName: "text-primary",
    icon: Loader2,
    spinning: true,
  },
  paused: {
    text: "待确认",
    textClassName: "text-warning",
    iconClassName: "text-warning",
    icon: CirclePause,
  },
  done: {
    text: "已完成",
    textClassName: "text-success",
    iconClassName: "text-success",
    icon: CheckCircle2,
  },
  failed: {
    text: "失败",
    textClassName: "text-danger",
    iconClassName: "text-danger",
    icon: CircleX,
  },
  cancelled: {
    text: "已取消",
    textClassName: "text-foreground-muted",
    iconClassName: "text-foreground-muted",
    icon: Ban,
  },
};

/** 按轮次（roundId）把专家分组，组内按 createdAt 升序，轮次按其最新专家时间降序（本轮在前）。 */
function groupByRound(
  members: ChildAgentSummary[],
): { roundId: string; latest: string; members: ChildAgentSummary[] }[] {
  const map = new Map<string, ChildAgentSummary[]>();
  for (const m of members) {
    const arr = map.get(m.roundId);
    if (arr) arr.push(m);
    else map.set(m.roundId, [m]);
  }
  const rounds = Array.from(map.entries()).map(([roundId, ms]) => {
    const sorted = [...ms].sort((a, b) => a.createdAt.localeCompare(b.createdAt));
    const latest = sorted.reduce(
      (acc, m) => (m.createdAt > acc ? m.createdAt : acc),
      sorted[0]?.createdAt ?? "",
    );
    return { roundId, latest, members: sorted };
  });
  rounds.sort((a, b) => b.latest.localeCompare(a.latest));
  return rounds;
}

function MemberRow({
  m,
  step,
  onOpen,
  onCancel,
}: {
  m: ChildAgentSummary;
  step?: string;
  onOpen: (sessionId: string, expertName: string) => void;
  onCancel?: (sessionId: string) => void;
}) {
  const s = STATUS[m.status] ?? {
    text: m.status,
    textClassName: "text-foreground-muted",
    iconClassName: "text-foreground-muted",
    icon: Bot,
  };
  const StatusIcon = s.icon;
  // 运行中：展示该专家「当前在做什么」的实时步骤（替代单纯一个转圈），未知时回退「运行中」。
  const running = m.status === "running";
  const cancellable = running || m.status === "paused";
  const subline = running ? step || "运行中…" : "";
  return (
    <li className="group/row relative">
      <button
        type="button"
        onClick={() => onOpen(m.sessionId, m.expertName)}
        className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent"
      >
        <span className="mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded-md bg-accent text-[13px] text-foreground-secondary">
          {avatarEmoji(m.avatar) ? (
            <span aria-hidden="true">{avatarEmoji(m.avatar)}</span>
          ) : (
            <Bot className="h-3.5 w-3.5" aria-hidden="true" />
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13px] font-medium text-foreground">
            {m.displayName || m.expertName || "专家"}
          </span>
          {m.profession && (
            <span className="block truncate text-[11px] text-foreground-muted">
              {m.profession}
            </span>
          )}
        </span>
        {subline && (
            <span className="mt-0.5 flex min-w-0 items-center gap-1 text-[11px] text-primary">
              <Loader2 className="h-3 w-3 shrink-0 animate-spin" aria-hidden="true" />
              <span className="truncate">{subline}</span>
            </span>
          )}
        {!running && (
          <span
            className={`mt-0.5 flex shrink-0 items-center gap-1 text-[11px] ${s.textClassName}`}
          >
            <StatusIcon className={`h-3.5 w-3.5 ${s.iconClassName}`} aria-hidden="true" />
            {s.text}
          </span>
        )}
      </button>
      {cancellable && onCancel && (
        <button
          type="button"
          title="取消该子代理"
          onClick={(e) => {
            e.stopPropagation();
            onCancel(m.sessionId);
          }}
          className="absolute right-1.5 top-1/2 hidden -translate-y-1/2 items-center rounded-md bg-surface px-1.5 py-0.5 text-[11px] text-foreground-muted shadow-sm transition-colors hover:text-destructive group-hover/row:flex"
        >
          取消
        </button>
      )}
    </li>
  );
}

/**
 * 右侧面板：当前会话的专家（child 子运行）列表 + 状态，按轮次分组。
 * 最新一轮为「本轮」常驻展开；更早的轮次合并折叠到「历史专家」里，默认收起。
 */
export function SessionChildAgentsPanel({
  members,
  steps,
  onOpen,
  onCancel,
}: {
  members: ChildAgentSummary[];
  /** childSessionId → 当前步骤短句（运行中行展示）。 */
  steps?: Record<string, string>;
  onOpen: (sessionId: string, expertName: string) => void;
  /** 取消单个子代理（运行中/等待中可用）；未传则不显示取消。 */
  onCancel?: (sessionId: string) => void;
}) {
  const [historyOpen, setHistoryOpen] = useState(false);
  if (members.length === 0) return null;

  const rounds = groupByRound(members);
  const current = rounds[0];
  const history = rounds.slice(1).flatMap((r) => r.members);
  const currentRunning = current?.members.filter((m) => m.status === "running").length ?? 0;

  return (
    <section className="flex shrink-0 flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-foreground">专家</h3>
        <span className="text-xs text-foreground-muted">{members.length}</span>
      </div>

      {current && (
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2 px-2 text-[11px] font-medium text-foreground-muted">
            <span>本轮 · {current.members.length}</span>
            {currentRunning > 0 && (
              <span className="text-primary">运行中 {currentRunning}</span>
            )}
          </div>
          <ul className="flex flex-col gap-1.5">
            {current.members.map((m) => (
              <MemberRow
                key={m.sessionId}
                m={m}
                step={steps?.[m.sessionId]}
                onOpen={onOpen}
                onCancel={onCancel}
              />
            ))}
          </ul>
        </div>
      )}

      {history.length > 0 && (
        <div className="flex flex-col gap-1">
          <button
            type="button"
            onClick={() => setHistoryOpen((v) => !v)}
            className="flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium text-foreground-muted transition-colors hover:bg-accent"
          >
            {historyOpen ? (
              <ChevronDown className="h-3.5 w-3.5" aria-hidden="true" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5" aria-hidden="true" />
            )}
            历史专家 ({history.length})
          </button>
          {historyOpen && (
            <ul className="flex flex-col gap-1.5">
              {history.map((m) => (
                <MemberRow
                  key={m.sessionId}
                  m={m}
                  step={steps?.[m.sessionId]}
                  onOpen={onOpen}
                />
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}

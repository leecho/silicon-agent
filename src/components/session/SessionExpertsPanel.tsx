import { useEffect, useMemo, useState } from "react";
import type { LucideIcon } from "lucide-react";
import { Ban, Bot, CheckCircle2, CirclePause, CircleX, Loader2, UserRound } from "lucide-react";

import { getTeamDetail, listProjectMembers, listThreadTasks, subscribeAgentStreamEvents } from "../../api";
import { avatarEmoji } from "../../lib/avatar";
import type { ChildAgentSummary } from "../../types";
import { BubbleConfirm } from "../ui/BubbleConfirm";
import { Tooltip } from "../ui/Tooltip";
import { PanelSection } from "./PanelSection";

type ExpertRow = {
  name: string;
  displayName?: string | null;
  profession?: string | null;
  avatar?: string | null;
  isMember: boolean;
};

const STATUS: Record<string, { text: string; className: string; icon: LucideIcon; spin?: boolean }> = {
  running: { text: "执行中…", className: "text-primary", icon: Loader2, spin: true },
  paused: { text: "需确认", className: "text-warning", icon: CirclePause },
  done: { text: "已完成", className: "text-success", icon: CheckCircle2 },
  failed: { text: "失败", className: "text-danger", icon: CircleX },
  cancelled: { text: "已取消", className: "text-foreground-muted", icon: Ban },
};

/**
 * 专家面板：展示本项目/团队成员（带成员图标）+ 本会话参与过的非成员。
 * 名称 · 职业；下方左=当前任务标题，右=执行步骤/需确认/已完成；运行中/待确认者悬浮显示「取消」。
 * 结束后仍保留任务名 + 「已完成/失败」状态；点击进入其运行会话。
 */
export function SessionExpertsPanel({
  threadSessionId,
  projectId,
  teamId,
  childAgents,
  steps,
  onOpen,
  onCancel,
}: {
  threadSessionId: string;
  projectId?: string | null;
  teamId?: string | null;
  childAgents: ChildAgentSummary[];
  steps?: Record<string, string>;
  onOpen: (sessionId: string, expertName: string) => void;
  onCancel?: (sessionId: string) => void;
}) {
  const [members, setMembers] = useState<ExpertRow[]>([]);
  // 运行会话 → 任务台账标题映射：让面板显示「任务标题」而非派发给 agent 的完整消息。
  const [taskByRun, setTaskByRun] = useState<Map<string, string>>(new Map());
  // 正在二次确认取消的运行会话 id（BubbleConfirm 气泡）。
  const [confirmingCancel, setConfirmingCancel] = useState<string | null>(null);
  // 已点确认、等待运行到检查点真正停止的会话 id（乐观显示「取消中…」）。
  const [cancelling, setCancelling] = useState<Set<string>>(new Set());

  useEffect(() => {
    let off = false;
    async function load() {
      const rows: ExpertRow[] = [];
      try {
        if (projectId) {
          const ms = await listProjectMembers(projectId);
          for (const m of ms) rows.push({ name: m.expertName, displayName: m.displayName, profession: m.roleLabel, avatar: m.avatar, isMember: true });
        } else if (teamId) {
          const d = await getTeamDetail(teamId);
          for (const a of d.members) rows.push({ name: a.name, displayName: a.displayName, profession: a.profession, avatar: a.avatar, isMember: true });
        }
      } catch { /* ignore */ }
      if (!off) setMembers(rows);
    }
    void load();
    return () => { off = true; };
  }, [projectId, teamId]);

  useEffect(() => {
    let off = false;
    const reload = () => {
      void listThreadTasks(threadSessionId).then((ts) => {
        if (off) return;
        const m = new Map<string, string>();
        for (const t of ts) if (t.runSessionId) m.set(t.runSessionId, t.title);
        setTaskByRun(m);
      }).catch(() => {});
    };
    reload();
    let un: (() => void) | undefined;
    void subscribeAgentStreamEvents((e) => {
      if (e.kind === "tasks_updated" && e.sessionId === threadSessionId) reload();
      else if (e.kind === "run_started" || e.kind === "run_finished" || e.kind === "tool_result") reload();
    }).then((fn) => (off ? fn() : (un = fn)));
    return () => { off = true; un?.(); };
  }, [threadSessionId]);

  // 成员 + 本会话参与过的非成员（ad-hoc/历史）；后者无成员图标。
  const agents = useMemo(() => {
    const memberNames = new Set(members.map((m) => m.name));
    const extras: ExpertRow[] = [];
    const seen = new Set(memberNames);
    for (const c of childAgents) {
      if (seen.has(c.expertName)) continue;
      seen.add(c.expertName);
      extras.push({ name: c.expertName, displayName: c.displayName, profession: c.profession, avatar: c.avatar, isMember: false });
    }
    return [...members, ...extras];
  }, [members, childAgents]);

  // 每个 agent 的最近一次运行（任意状态，按创建时间降序取首个）。
  const latestRunByName = useMemo(() => {
    const m = new Map<string, ChildAgentSummary>();
    for (const c of childAgents) {
      const prev = m.get(c.expertName);
      if (!prev || c.createdAt > prev.createdAt) m.set(c.expertName, c);
    }
    return m;
  }, [childAgents]);

  const runningCount = useMemo(
    () => agents.filter((a) => latestRunByName.get(a.name)?.status === "running").length,
    [agents, latestRunByName],
  );

  return (
    <PanelSection
      title="专家"
      count={agents.length}
      right={runningCount > 0 ? <span className="text-[11px] text-primary">运行中 {runningCount}</span> : undefined}
    >
      {agents.length === 0 ? (
        <p className="px-2 py-3 text-[12px] text-foreground-muted">还没有专家。</p>
      ) : (
        <ul className="flex flex-col gap-0.5">
          {agents.map((a) => {
            const run = latestRunByName.get(a.name);
            const isCancelling = !!run && cancelling.has(run.sessionId) && (run.status === "running" || run.status === "paused");
            const s = run ? STATUS[run.status] : undefined;
            const cancellable = !!run && !isCancelling && (run.status === "running" || run.status === "paused");
            const taskTitle = run ? (taskByRun.get(run.sessionId) || run.task) : "";
            const rightText = run && s ? (run.status === "running" ? (steps?.[run.sessionId] || s.text) : s.text) : "";
            const RightIcon = s?.icon;
            const confirming = !!run && confirmingCancel === run.sessionId;
            return (
              <li key={a.name} className="group/row relative">
                <div
                  onClick={() => { if (run) onOpen(run.sessionId, a.name); }}
                  className={`flex items-center gap-2 rounded-md px-2 py-1.5 transition-colors ${run ? "cursor-pointer hover:bg-accent" : ""}`}
                >
                  <span className="grid h-7 w-7 shrink-0 place-items-center rounded-md border border-border bg-background text-[14px]">
                    {avatarEmoji(a.avatar) ? <span aria-hidden="true">{avatarEmoji(a.avatar)}</span> : <Bot className="h-3.5 w-3.5 text-foreground-muted" aria-hidden="true" />}
                  </span>
                  <span className="min-w-0 flex-1">
                    {/* 名称 · 职业 + 成员图标 */}
                    <span className="flex items-center gap-1">
                      <span className="shrink-0 max-w-full truncate text-[13px] font-medium text-foreground">{a.displayName || a.name}</span>
                      {a.profession && <span className="min-w-0 truncate text-[11px] text-foreground-muted">· {a.profession}</span>}
                      {a.isMember && (
                        <Tooltip content="项目成员">
                          <span className="inline-flex shrink-0">
                            <UserRound className="h-3 w-3 text-primary" aria-hidden="true" />
                          </span>
                        </Tooltip>
                      )}
                    </span>
                    {/* 任务行：左=当前任务标题；右=状态/步骤；可取消者悬浮换成「取消」 */}
                    {run ? (
                      <span className="flex items-center gap-2 text-[11px]">
                        <Tooltip content={taskTitle || undefined} disabled={!taskTitle}>
                          <span className="min-w-0 flex-1 truncate text-foreground-secondary">{taskTitle || "（无任务）"}</span>
                        </Tooltip>
                        {isCancelling ? (
                          <span className="flex shrink-0 items-center gap-1 text-foreground-muted">
                            <Loader2 className="h-3 w-3 shrink-0 animate-spin" aria-hidden="true" />
                            取消中…
                          </span>
                        ) : (
                          <>
                            {s && (
                              <span className={`flex shrink-0 max-w-[55%] items-center gap-1 ${s.className} ${cancellable ? "group-hover/row:hidden" : ""}`}>
                                {RightIcon && <RightIcon className={`h-3 w-3 shrink-0 ${s.spin ? "animate-spin" : ""}`} aria-hidden="true" />}
                                <span className="truncate">{rightText}</span>
                              </span>
                            )}
                            {cancellable && onCancel && (
                              <button
                                type="button"
                                onClick={(e) => { e.stopPropagation(); setConfirmingCancel(run.sessionId); }}
                                className="hidden shrink-0 items-center gap-1 text-danger transition-colors hover:opacity-80 group-hover/row:flex"
                              >
                                <Ban className="h-3 w-3 shrink-0" aria-hidden="true" />
                                取消
                              </button>
                            )}
                          </>
                        )}
                      </span>
                    ) : (
                      <span className="block text-[11px] text-foreground-muted">空闲</span>
                    )}
                  </span>
                </div>

                {confirming && run && onCancel && (
                  <BubbleConfirm
                    title="取消该运行？"
                    description={taskTitle || undefined}
                    confirmText="取消运行"
                    onCancel={() => setConfirmingCancel(null)}
                    onConfirm={() => {
                      onCancel(run.sessionId);
                      setCancelling((prev) => new Set(prev).add(run.sessionId));
                      setConfirmingCancel(null);
                    }}
                  />
                )}
              </li>
            );
          })}
        </ul>
      )}
    </PanelSection>
  );
}

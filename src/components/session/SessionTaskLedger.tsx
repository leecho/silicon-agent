import { useEffect, useMemo, useState } from "react";
import type { LucideIcon } from "lucide-react";
import { Ban, CheckCircle2, ChevronDown, ChevronRight, Circle, CircleX, Loader2 } from "lucide-react";

import { listThreadTasks, subscribeAgentStreamEvents } from "../../api";
import type { ProjectTask } from "../../types";
import { Tooltip } from "../ui/Tooltip";
import { PanelSection } from "./PanelSection";

const STATUS: Record<string, { text: string; className: string; icon: LucideIcon; spin?: boolean }> = {
  pending: { text: "待办", className: "text-foreground-muted", icon: Circle },
  in_progress: { text: "进行中", className: "text-primary", icon: Loader2, spin: true },
  done: { text: "完成", className: "text-success", icon: CheckCircle2 },
  failed: { text: "失败", className: "text-danger", icon: CircleX },
  cancelled: { text: "已取消", className: "text-foreground-muted", icon: Ban },
};

/**
 * 编排会话（项目/团队）的任务台账：一份计划。每条任务 = 状态 + 标题 + 负责人；委派项关联 child run，
 * 运行中显示实时步骤、点击进入该 run 的 SessionPage。名册外/未带 task_id 的 child 归「未关联运行」。
 */
export function SessionTaskLedger({
  threadSessionId,
  onOpen,
}: {
  threadSessionId: string;
  onOpen: (sessionId: string, expertName: string) => void;
}) {
  const [tasks, setTasks] = useState<ProjectTask[]>([]);

  useEffect(() => {
    let off = false;
    const reload = () => {
      void listThreadTasks(threadSessionId).then((t) => { if (!off) setTasks(t); }).catch(() => {});
    };
    reload();
    let un: (() => void) | undefined;
    void subscribeAgentStreamEvents((e) => {
      // 自身 update_tasks，或本会话下任何 run 起止/结果（可能改任务状态）→ 重取。
      if (e.kind === "tasks_updated" && e.sessionId === threadSessionId) reload();
      else if (e.kind === "run_started" || e.kind === "run_finished" || e.kind === "tool_result") reload();
    }).then((fn) => (off ? fn() : (un = fn)));
    return () => { off = true; un?.(); };
  }, [threadSessionId]);


  // 主任务（每轮基调，parentTaskId 为空）；最近一轮在前（当前轮），其余为历史轮。
  const mains = useMemo(
    () => tasks.filter((t) => !t.parentTaskId).sort((a, b) => b.sort - a.sort),
    [tasks],
  );
  const subsByMain = useMemo(() => {
    const m = new Map<string, ProjectTask[]>();
    for (const t of tasks) if (t.parentTaskId) (m.get(t.parentTaskId) ?? m.set(t.parentTaskId, []).get(t.parentTaskId)!).push(t);
    for (const arr of m.values()) arr.sort((a, b) => a.sort - b.sort);
    return m;
  }, [tasks]);

  // 每轮可折叠；默认仅展开当前轮（mains[0]），历史轮折叠。openIds=null 表示用默认。
  const [openIds, setOpenIds] = useState<string[] | null>(null);
  const openSet = openIds ?? (mains[0] ? [mains[0].id] : []);
  const toggleMain = (id: string) =>
    setOpenIds(openSet.includes(id) ? openSet.filter((x) => x !== id) : [...openSet, id]);

  const running = tasks.filter((t) => t.parentTaskId && t.status === "in_progress").length;

  // 子任务行：状态图标 + 标题。执行步骤/状态/取消由「专家」面板呈现，这里不再显示。
  const renderSub = (t: ProjectTask) => {
    const s = STATUS[t.status] ?? STATUS.pending;
    const Icon = s.icon;
    const clickable = !!t.runSessionId;
    return (
      <li key={t.id}>
        <Tooltip content={t.title}>
          <button
            type="button"
            onClick={() => { if (t.runSessionId) onOpen(t.runSessionId, t.assignee || t.title); }}
            disabled={!clickable}
            className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors ${clickable ? "hover:bg-accent" : "cursor-default"}`}
          >
            <Icon className={`h-3.5 w-3.5 shrink-0 ${s.className} ${s.spin ? "animate-spin" : ""}`} aria-hidden="true" />
            <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">{t.title}</span>
          </button>
        </Tooltip>
      </li>
    );
  };

  return (
    <PanelSection
      title="任务"
      right={running > 0 ? <span className="text-[11px] text-primary">进行中 {running}</span> : undefined}
    >
      {mains.length === 0 ? (
        <p className="px-2 py-3 text-[12px] text-foreground-muted">还没有任务。主持人会把计划列在这里。</p>
      ) : (
        // 按主任务（轮）分组，每轮可折叠：默认仅当前轮展开，历史轮折叠。
        <div className="flex flex-col gap-1">
          {mains.map((m) => {
            const subs = subsByMain.get(m.id) ?? [];
            const ms = STATUS[m.status] ?? STATUS.pending;
            const MIcon = ms.icon;
            const open = openSet.includes(m.id);
            return (
              <div key={m.id} className="flex flex-col gap-0.5">
                <button
                  type="button"
                  onClick={() => toggleMain(m.id)}
                  className="flex items-center gap-1 rounded-md px-1 py-1 text-left transition-colors hover:bg-accent"
                >
                  {open ? <ChevronDown className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" /> : <ChevronRight className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />}
                  <MIcon className={`h-3 w-3 shrink-0 ${ms.className} ${ms.spin ? "animate-spin" : ""}`} aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate text-[12px] font-medium text-foreground-secondary" title={m.title}>{m.title}</span>
                  <span className="shrink-0 text-[11px] text-foreground-muted">{subs.length}</span>
                </button>
                {open && (
                  <ul className="ml-2 flex flex-col gap-0.5 border-l border-border-subtle pl-1">
                    {subs.map(renderSub)}
                    {subs.length === 0 && (
                      <li className="px-2 py-1 text-[11px] text-foreground-muted">尚未拆分子任务。</li>
                    )}
                  </ul>
                )}
              </div>
            );
          })}
        </div>
      )}
    </PanelSection>
  );
}

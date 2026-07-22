import { PanelRightClose } from "lucide-react";
import type { ChildAgentSummary, TodoItem } from "../../types";
import { SessionExpertsPanel } from "./SessionExpertsPanel";
import { SessionChildAgentsPanel } from "./SessionChildAgentsPanel";
import { SessionTaskLedger } from "./SessionTaskLedger";
import { TodoPanel } from "./SessionTodo";
import { Tooltip } from "../ui/Tooltip";

export function SessionMonitorPanel({
  childAgents,
  childSteps,
  onCollapse,
  onOpenChildAgent,
  onCancelChildAgent,
  todos,
  sessionId,
  projectId,
  teamId,
  embedded,
}: {
  childAgents: ChildAgentSummary[];
  childSteps?: Record<string, string>;
  onCollapse: () => void;
  onOpenChildAgent: (sessionId: string, expertName: string) => void;
  onCancelChildAgent?: (sessionId: string) => void;
  todos: TodoItem[];
  /** 当前会话 id（任务台账按会话取任务用）。 */
  sessionId: string;
  /** 关联项目 id（非空=该会话属于某项目）。 */
  projectId?: string | null;
  /** 激活的团队 id（非空=团队模式会话）。 */
  teamId?: string | null;
  /** 嵌入 tab 壳：去掉自身标题头/收起按钮与外框（由 SessionSidePanel 承担），只渲染正文。 */
  embedded?: boolean;
}) {
  // 名册模式（项目/团队）：右侧合并为一份成员名册（成员带运行状态）；自由模式沿用按轮次的子代理面板。
  const rosterMode = !!projectId || !!teamId;
  return (
    <div
      className={
        embedded
          ? "flex h-full min-h-0 w-full flex-col gap-5 overflow-auto pb-3 pt-3 text-card-foreground"
          : "flex h-full w-[300px] flex-col gap-5 overflow-auto border-l border-border-subtle pb-3 text-card-foreground"
      }
    >
      {!embedded && (
        <div className="flex items-center justify-between gap-2 px-3 py-4 border-b border-border-subtle">
          <div className="text-sm font-semibold text-foreground">任务监控</div>
          <Tooltip content="收起任务监控">
            <button
              type="button"
              aria-label="收起任务监控"
              className="absolute right-3 grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-foreground"
              onClick={onCollapse}
            >
              <PanelRightClose className="h-[14px] w-[14px]" aria-hidden="true" />
            </button>
          </Tooltip>
        </div>
      )}
      <div className="flex-1 overflow-auto flex flex-col gap-3 px-3 ">
      {rosterMode ? (
        <>
          <SessionTaskLedger threadSessionId={sessionId} onOpen={onOpenChildAgent} />
          <SessionExpertsPanel
            threadSessionId={sessionId}
            projectId={projectId}
            teamId={teamId}
            childAgents={childAgents}
            steps={childSteps}
            onOpen={onOpenChildAgent}
            onCancel={onCancelChildAgent}
          />
        </>
      ) : (
        <>
          <TodoPanel todos={todos} />
          <SessionChildAgentsPanel
            members={childAgents}
            steps={childSteps}
            onOpen={onOpenChildAgent}
            onCancel={onCancelChildAgent}
          />
        </>
      )}
      </div>
      </div>
  );
}
